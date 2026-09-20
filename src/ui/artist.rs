//! The artist page: one singer's profile, hot songs and albums, drawn inside the shell's
//! content area.
//!
//! Every state the page can be in draws something. Loading says which artist it is waiting
//! for (and keeps the panes' titles, so the page does not look empty), and a failure carries
//! the error plus the way out — `r` retries, `Esc` leaves — instead of stranding the reader
//! on a page with nothing on it and nothing to press.

use std::sync::LazyLock;

use ncm_api::{ArtistAlbum, ArtistDetail, SongInfo};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, TableState, Wrap},
};
use ratatui_image::{Resize, StatefulImage};
use time::{OffsetDateTime, UtcOffset, format_description::FormatItem, macros::format_description};

use super::{
    BlockStyle,
    block::CornerBlock,
    scrollbar::calc_scroll_offset,
    skeleton::Skeleton,
    table,
    title::render_title,
};
use crate::{
    config::{ColumnDef, Theme},
    layout::ArtistLayout,
    state::{ArtistData, ArtistState, TableMode},
    utils::format_duration,
};

/// Anything the API did not tell us. One glyph for the whole page, the way `ui::content`
/// keeps its own for the fields it cannot fill.
const MISSING: &str = "—";

/// Width of the portrait column, in cells.
const PORTRAIT_WIDTH: u16 = 12;

/// The hot-song pane's columns. The page summarises an artist rather than replacing the main
/// table, so it has fixed columns of its own: reusing the configured song columns would make
/// the same page a different page after a config change, and the pane's width is whatever the
/// profile band left over.
static SONG_COLUMNS: LazyLock<Vec<ColumnDef>> = LazyLock::new(|| {
    vec![
        column("TITLE", "name", None, Some(18)),
        column("ALBUM", "album", None, Some(12)),
        column("LENGTH", "duration", Some(9), None),
    ]
});

/// The album pane's columns: name, track count, release date. The last two are sized to the
/// widest value they hold, so the pane's narrowest layout (40% of a 100-column terminal, minus
/// the scrollbar) still fits all three without clipping the date.
static ALBUM_COLUMNS: LazyLock<Vec<ColumnDef>> = LazyLock::new(|| {
    vec![
        column("ALBUM", "name", None, Some(14)),
        column("TRACKS", "size", Some(7), None),
        column("RELEASED", "publish_time", Some(10), None),
    ]
});

const RELEASE_FMT: &[FormatItem<'static>] = format_description!("[year]-[month]-[day]");

/// A column of one of the page's own tables. `field` is only a name here — the page builds its
/// cells itself — but it keeps the column definitions shaped like the configured ones.
fn column(header: &str, field: &str, width: Option<u16>, min_width: Option<u16>) -> ColumnDef {
    ColumnDef {
        header: header.to_string(),
        field: field.to_string(),
        width,
        min_width,
        ratio: None,
    }
}

pub(super) fn draw(
    f: &mut Frame,
    state: &mut ArtistState,
    bs: &BlockStyle<'_>,
    lay: &ArtistLayout,
) {
    match &state.data {
        ArtistData::Loading => draw_loading(f, state, bs, lay),
        ArtistData::Failed(error) => draw_failed(f, state, error, bs, lay),
        ArtistData::Ready { .. } => draw_ready(f, state, bs, lay),
    }
}

/// Waiting for the profile: the band names the artist and says what is happening; the two
/// panes keep their titles over a skeleton, the same way the main table loads.
fn draw_loading(f: &mut Frame, state: &ArtistState, bs: &BlockStyle<'_>, lay: &ArtistLayout) {
    let title = render_title("► 歌手 {name} ◄", &state.name, 0, 0);
    let inner = panel(f, bs, &title, lay.profile);
    let note = Line::from(Span::styled(
        "正在加载歌手信息…",
        Style::default().fg(bs.colors.muted),
    ));
    f.render_widget(Paragraph::new(note).wrap(Wrap { trim: true }), inner);

    skeleton_pane(f, bs, "► 热门曲目 ◄", lay.songs);
    skeleton_pane(f, bs, "► 专辑 ◄", lay.albums);
}

/// The profile request itself failed: nothing loaded, so the error is the page. It says what
/// went wrong and what can be done about it — a bare "错误" with no `r` is what leaves a reader
/// stuck.
fn draw_failed(
    f: &mut Frame,
    state: &ArtistState,
    error: &str,
    bs: &BlockStyle<'_>,
    lay: &ArtistLayout,
) {
    let title = render_title("► 歌手 {name} · 加载失败 ◄", &state.name, 0, 0);
    // The whole content area, not just the band: there are no lists to keep room for.
    let inner = panel(f, bs, &title, lay.profile.union(lay.albums));
    let lines = vec![
        Line::from(Span::styled(
            format!("错误: {error}"),
            Style::default().fg(bs.colors.error),
        )),
        Line::default(),
        Line::from(Span::styled(
            "按 r 重新加载，Esc 返回",
            Style::default().fg(bs.colors.muted),
        )),
    ];
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

/// The loaded page: the profile band over the hot songs and the albums.
fn draw_ready(f: &mut Frame, state: &mut ArtistState, bs: &BlockStyle<'_>, lay: &ArtistLayout) {
    // The band borrows the state mutably (the image protocol carries its own resize state), so
    // take the pieces the panes need before it runs.
    draw_profile(f, state, bs, lay.profile);

    let ArtistData::Ready { detail, albums } = &state.data else {
        return;
    };
    draw_songs(f, state, bs, lay.songs);
    draw_albums(f, detail, albums, bs, lay.albums);
}

/// The profile band: the portrait when it is decoded, then the name, the aliases, the sizes
/// and the biography. The biography wraps into the band's remaining rows and is clipped by
/// them, so no biography can push the lists off the screen.
fn draw_profile(f: &mut Frame, state: &mut ArtistState, bs: &BlockStyle<'_>, area: Rect) {
    let colors = bs.colors;
    let ArtistData::Ready { detail, .. } = &state.data else {
        return;
    };
    let title = render_title("► 歌手 {name} ◄", &detail.name, 0, 0);
    let inner = panel(f, bs, &title, area);
    if inner.is_empty() {
        return;
    }

    // The portrait only takes a column once there is one to draw: until then — and on a
    // terminal with no image support, where there never will be — the text gets the width.
    let text_area = match state.avatar.as_mut() {
        Some(protocol) => {
            let [portrait, text] = Layout::horizontal([
                Constraint::Length(PORTRAIT_WIDTH.min(inner.width)),
                Constraint::Min(1),
            ])
            .areas(inner);
            f.render_stateful_widget(
                StatefulImage::new().resize(Resize::Fit(None)),
                portrait,
                protocol,
            );
            text
        }
        None => inner,
    };

    let mut name_line = vec![Span::styled(
        detail.name.as_str(),
        Style::default()
            .fg(colors.accent)
            .add_modifier(Modifier::BOLD),
    )];
    if !detail.alias.is_empty() {
        name_line.push(Span::styled(
            format!("   {}", detail.alias.join(" / ")),
            Style::default().fg(colors.muted),
        ));
    }

    let mut lines = vec![
        Line::from(name_line),
        Line::from(Span::styled(
            format!("歌曲 {} · 专辑 {}", detail.music_size, detail.album_size),
            Style::default().fg(colors.muted),
        )),
        Line::default(),
    ];
    if detail.brief_desc.trim().is_empty() {
        lines.push(Line::from(Span::styled(
            "（暂无简介）",
            Style::default().fg(colors.muted),
        )));
    } else {
        lines.push(Line::from(detail.brief_desc.trim()));
    }

    f.render_widget(
        Paragraph::new(lines)
            .style(Style::default().fg(colors.text))
            .wrap(Wrap { trim: true }),
        text_area,
    );
}

/// The hot songs, with the cursor on one of them: Enter plays what the cursor is on.
fn draw_songs(f: &mut Frame, state: &ArtistState, bs: &BlockStyle<'_>, area: Rect) {
    let songs = state.hot_songs();
    let title = render_title("► 热门曲目 ({count}) ◄", "", songs.len(), 0);
    let inner = panel(f, bs, &title, area);
    if songs.is_empty() {
        note(f, "（没有热门曲目）", inner, bs.colors);
        return;
    }

    let sel = state.song_selected.min(songs.len() - 1);
    let visible = inner.height.saturating_sub(1).max(1) as usize;
    let offset = calc_scroll_offset(sel, visible, songs.len());
    let end = (offset + visible).min(songs.len());
    let mut table_state = TableState::default();
    // Only the visible window is materialized; the window is pre-scrolled, so the selection
    // ratatui sees is relative to it. Same trick, and same reason, as the main content table.
    table_state.select(Some(sel - offset));
    *table_state.offset_mut() = 0;

    table::render_table(
        f,
        &SONG_COLUMNS,
        song_rows(&songs[offset..end], bs.colors),
        &mut table_state,
        TableMode::Row,
        bs.colors,
        inner,
        songs.len(),
        sel,
    );
}

/// The albums, newest first. A failed album request says so in the pane: the profile above it
/// stays readable, so the page is not lost to it.
fn draw_albums(
    f: &mut Frame,
    detail: &ArtistDetail,
    albums: &Result<Vec<ArtistAlbum>, String>,
    bs: &BlockStyle<'_>,
    area: Rect,
) {
    let colors = bs.colors;
    let total = detail.album_size as usize;
    let list = match albums {
        Ok(list) => list,
        Err(error) => {
            let inner = panel(f, bs, "► 专辑 加载失败 ◄", area);
            note(f, &format!("错误: {error}（按 r 重试）"), inner, colors);
            return;
        }
    };

    let title = render_title("► 专辑 ({count}/{total}) ◄", "", list.len(), total);
    let inner = panel(f, bs, &title, area);
    if list.is_empty() {
        note(f, "（没有专辑）", inner, colors);
        return;
    }

    // The album list is read top-down rather than walked with a cursor, so it has no
    // selection of its own; the pane scrolls nothing and always shows the newest albums.
    let visible = inner.height.saturating_sub(1).max(1) as usize;
    let end = visible.min(list.len());
    let mut table_state = TableState::default();
    table_state.select(None);

    table::render_table(
        f,
        &ALBUM_COLUMNS,
        album_rows(&list[..end], colors),
        &mut table_state,
        TableMode::Row,
        colors,
        inner,
        list.len(),
        0,
    );
}

/// The visible hot-song rows. Borrowed fields stay borrowed; only the length is formatted.
fn song_rows<'a>(songs: &'a [SongInfo], colors: &Theme) -> Vec<Row<'a>> {
    songs
        .iter()
        .map(|song| {
            Row::new(vec![
                Cell::from(song.name.as_str()).style(Style::default().fg(colors.muted)),
                Cell::from(song.album.as_str()).style(Style::default().fg(colors.muted)),
                Cell::from(format_duration(song.duration)).style(Style::default().fg(colors.muted)),
            ])
            .height(1)
        })
        .collect()
}

/// The visible album rows.
fn album_rows<'a>(albums: &'a [ArtistAlbum], colors: &Theme) -> Vec<Row<'a>> {
    albums
        .iter()
        .map(|album| {
            Row::new(vec![
                Cell::from(album.name.as_str()).style(Style::default().fg(colors.muted)),
                Cell::from(album.size.to_string()).style(Style::default().fg(colors.muted)),
                Cell::from(release_date(album.publish_time))
                    .style(Style::default().fg(colors.muted)),
            ])
            .height(1)
        })
        .collect()
}

/// An album's release date. The API sends midnight China time, so the timestamp is moved into
/// the reader's own offset before it is cut down to a date — otherwise every album released
/// just after midnight would be listed a day early.
fn release_date(publish_time_ms: u64) -> String {
    if publish_time_ms == 0 {
        return MISSING.to_string();
    }
    let Ok(utc) = OffsetDateTime::from_unix_timestamp((publish_time_ms / 1000) as i64) else {
        return MISSING.to_string();
    };
    let local = UtcOffset::current_local_offset()
        .map(|offset| utc.to_offset(offset))
        .unwrap_or(utc);
    local
        .format(&RELEASE_FMT)
        .unwrap_or_else(|_| MISSING.to_string())
}

/// A pane's block, drawn; returns the area left for its body.
fn panel(f: &mut Frame, bs: &BlockStyle<'_>, title: &str, area: Rect) -> Rect {
    let block = CornerBlock::from_color(bs, bs.colors.bg).title(title, bs.colors);
    let inner = block.inner(area);
    f.render_widget(block, area);
    inner
}

/// A pane that has nothing to show yet: its title over a skeleton, which reads as "loading"
/// rather than as an empty box.
fn skeleton_pane(f: &mut Frame, bs: &BlockStyle<'_>, title: &str, area: Rect) {
    let inner = panel(f, bs, title, area);
    f.render_widget(
        Skeleton::new().bg(bs.colors.bg).surface(bs.colors.surface),
        inner,
    );
}

/// One muted line of text, for the empty and failed panes.
fn note(f: &mut Frame, text: &str, area: Rect, colors: &Theme) {
    let line = Line::from(Span::styled(
        text.to_string(),
        Style::default().fg(colors.muted),
    ));
    f.render_widget(Paragraph::new(line).wrap(Wrap { trim: true }), area);
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};
    use unicode_width::UnicodeWidthStr;

    use super::*;
    use crate::{
        config::{BorderConfig, ThemeRegistry},
        layout,
    };

    /// Render the page for one state and hand back what the reader would see.
    fn render(state: &mut ArtistState) -> Buffer {
        let colors = ThemeRegistry::new(Default::default())
            .get("catppuccin-mocha")
            .cloned()
            .unwrap_or_default();
        let border = BorderConfig::default();
        let bs = BlockStyle {
            colors: &colors,
            border: &border,
            tick: 0,
        };

        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("backend");
        terminal
            .draw(|f| {
                let lay = layout::artist(f.area());
                draw(f, state, &bs, &lay);
            })
            .expect("draw");
        terminal.backend().buffer().clone()
    }

    /// Everything drawn, row by row, as the reader reads it. A wide glyph (CJK) covers two
    /// cells and leaves the second one empty, so those cells are skipped — otherwise every
    /// Chinese word would come back with a space through it.
    fn shown(buffer: &Buffer) -> Vec<String> {
        let mut out = Vec::new();
        for y in 0..buffer.area.height {
            let mut row = String::new();
            let mut skip = 0usize;
            for x in 0..buffer.area.width {
                let symbol = buffer[(x, y)].symbol();
                if skip > 0 {
                    skip -= 1;
                    continue;
                }
                row.push_str(symbol);
                skip = UnicodeWidthStr::width(symbol).saturating_sub(1);
            }
            out.push(row.trim_end().to_string());
        }
        out
    }

    fn ready_state() -> ArtistState {
        let detail = ArtistDetail {
            id: 6452,
            name: "周杰伦".into(),
            alias: vec!["Jay Chou".into(), "周董".into()],
            brief_desc: "华语流行乐男歌手、音乐人、演员、导演。".into(),
            pic_url: "https://p3.music.126.net/portrait.jpg".into(),
            album_size: 44,
            music_size: 568,
            hot_songs: vec![
                SongInfo {
                    id: 1,
                    name: "布拉格广场".into(),
                    singer: "蔡依林".into(),
                    artist_id: 7219,
                    album: "看我72变".into(),
                    album_id: 21349,
                    pic_url: String::new(),
                    duration: 294_600,
                    copyright: ncm_api::SongCopyright::Free,
                    local_path: None,
                },
                SongInfo {
                    id: 2,
                    name: "七里香".into(),
                    singer: "周杰伦".into(),
                    artist_id: 6452,
                    album: "七里香".into(),
                    album_id: 21_600,
                    pic_url: String::new(),
                    duration: 299_000,
                    copyright: ncm_api::SongCopyright::Free,
                    local_path: None,
                },
            ],
        };
        let albums = vec![
            ArtistAlbum {
                id: 274_336_916,
                name: "即兴曲".into(),
                pic_url: String::new(),
                size: 1,
                publish_time: 1_749_139_200_000,
            },
            ArtistAlbum {
                id: 147_779_282,
                name: "最伟大的作品".into(),
                pic_url: String::new(),
                size: 12,
                publish_time: 1_657_814_400_000,
            },
        ];

        let mut state = ArtistState::default();
        state.id = 6452;
        state.name = "周杰伦".into();
        state.pic_url = "https://p3.music.126.net/portrait.jpg".into();
        state.data = ArtistData::Ready {
            detail,
            albums: Ok(albums),
        };
        state
    }

    /// The loaded page shows the artist, their biography, their hot songs and their albums —
    /// the four things the page exists for.
    #[test]
    fn a_loaded_page_shows_the_profile_songs_and_albums() {
        let mut state = ready_state();
        let buffer = render(&mut state);
        let rows = shown(&buffer);
        let all = rows.join("\n");

        assert!(all.contains("周杰伦"), "the artist is missing:\n{all}");
        assert!(
            all.contains("Jay Chou / 周董"),
            "the aliases are missing:\n{all}"
        );
        assert!(
            all.contains("歌曲 568 · 专辑 44"),
            "the sizes are missing:\n{all}"
        );
        assert!(
            all.contains("华语流行乐男歌手"),
            "the biography is missing:\n{all}"
        );
        assert!(
            all.contains("热门曲目 (2)"),
            "the hot-song pane is missing:\n{all}"
        );
        assert!(all.contains("布拉格广场"), "a hot song is missing:\n{all}");
        assert!(all.contains("04:54"), "a song length is missing:\n{all}");
        assert!(all.contains("专辑 (2/44)"), "the album pane is missing:\n{all}");
        assert!(
            all.contains("最伟大的作品"),
            "an album name is missing:\n{all}"
        );
        assert!(
            all.contains("2022-07-15"),
            "an album release date is missing:\n{all}"
        );
    }

    /// While the profile is on its way the page still says who it is about and that it is
    /// loading, so it is never a blank frame.
    #[test]
    fn a_loading_page_names_the_artist_and_says_it_is_loading() {
        let mut state = ArtistState::default();
        state.id = 6452;
        state.name = "周杰伦".into();

        let buffer = render(&mut state);
        let all = shown(&buffer).join("\n");

        assert!(all.contains("周杰伦"), "the artist is missing:\n{all}");
        assert!(
            all.contains("正在加载歌手信息"),
            "the loading state is missing:\n{all}"
        );
        assert!(
            all.contains("热门曲目") && all.contains("专辑"),
            "the panes lost their titles while loading:\n{all}"
        );
    }

    /// A failure has to name the artist, show the error and say how to leave or retry.
    #[test]
    fn a_failed_page_carries_the_error_and_the_way_out() {
        let mut state = ArtistState::default();
        state.id = 6452;
        state.name = "周杰伦".into();
        state.data = ArtistData::Failed("network unreachable".into());

        let buffer = render(&mut state);
        let all = shown(&buffer).join("\n");

        assert!(all.contains("周杰伦"), "the artist is missing:\n{all}");
        assert!(
            all.contains("加载失败"),
            "the failure is not stated:\n{all}"
        );
        assert!(
            all.contains("network unreachable"),
            "the error text is missing:\n{all}"
        );
        assert!(
            all.contains("r 重新加载") && all.contains("Esc 返回"),
            "the way out is missing:\n{all}"
        );
    }

    /// The album request is a second request: when it fails the profile and the hot songs are
    /// still shown, and only the album pane reports it.
    #[test]
    fn a_failed_album_request_leaves_the_rest_of_the_page() {
        let mut state = ready_state();
        if let ArtistData::Ready { albums, .. } = &mut state.data {
            *albums = Err("album request timed out".into());
        }

        let buffer = render(&mut state);
        let all = shown(&buffer).join("\n");

        assert!(all.contains("布拉格广场"), "the songs are gone:\n{all}");
        assert!(
            all.contains("华语流行乐男歌手"),
            "the biography is gone:\n{all}"
        );
        assert!(
            all.contains("专辑 加载失败"),
            "the album pane hides the failure:\n{all}"
        );
        assert!(
            all.contains("album request timed out"),
            "the album error is missing:\n{all}"
        );
    }
}

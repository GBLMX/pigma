//! The MV's poster column on the lyrics page: how wide it is, and what it draws.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use ratatui_image::{Resize, StatefulImage};

use crate::{
    config::{Pane, PanesConfig, Theme},
    layout::{Axis, clamp},
    state::mv::MvPanel,
    utils::{format::clip_long_text, format_duration},
};

/// The tallest poster the panel draws, in rows. A poster is a square picture, so at the roughly
/// 1:2 character cell this is 24 cells across — a quarter of the width on a 96-cell terminal,
/// which is a poster beside the lyrics rather than the page itself.
const MAX_POSTER_ROWS: u16 = 12;

/// The widest line the facts can make, in cells: `03:45 · 1993-09-09`, a length and a release
/// date. The poster is never narrower than this, so those facts are always written whole and the
/// blurb is the only thing a small page cuts.
const FACTS_WIDTH: u16 = 18;

/// The shortest poster the panel draws, in rows — as wide as the facts need, and a square
/// picture is half as many rows tall. Anything less is a thumbnail, and the page is better off
/// with the lyrics alone.
const MIN_POSTER_ROWS: u16 = FACTS_WIDTH / 2;

/// Rows the facts take under the poster: the title, the singer, and the length with the release
/// date.
const FACTS_ROWS: u16 = 3;

/// Every row of the column that is not the poster: the three lines of facts, the blank under
/// them, and the one row of blurb a panel is worth having for at all.
const PANEL_TEXT_ROWS: u16 = FACTS_ROWS + 1 + 1;

/// The shortest page that gets a panel: the smallest poster and the rows under it.
const MIN_PANEL_HEIGHT: u16 = MIN_POSTER_ROWS + PANEL_TEXT_ROWS;

/// Cells between the lyrics and the poster column, so a long lyric line never runs into the
/// poster.
const PANEL_GAP: u16 = 2;

/// What the lyrics keep of the page: narrower than this and the poster would be the page.
const MIN_LYRICS_WIDTH: u16 = 30;

/// Divide `inner` between the lyrics and the MV's column, which is the rightmost one.
///
/// The column's width is `[panes] mv` — the size the user dragged it to, or left at 0 for the
/// width the page can spare: as tall as the page has rows for, up to [`MAX_POSTER_ROWS`], and the
/// column twice that, since a poster is a square picture. So a page with rows to give gets the
/// big poster, a shorter one a smaller poster, and a page that was never dragged keeps exactly
/// what it drew before the panel was a pane.
///
/// Returns the lyrics area and `None` when there is no panel to draw, or no room for one: the
/// lyrics then keep the whole page, which is what makes a page without a poster — and a page too
/// small for one — exactly the page this drew before the panel existed.
pub(super) fn panel_split(inner: Rect, panes: &PanesConfig, has_panel: bool) -> (Rect, Option<Rect>) {
    if !has_panel || !panes.visible(Pane::Mv) || inner.height < MIN_PANEL_HEIGHT {
        return (inner, None);
    }

    // A dragged width is clamped the way the other panes are (their config stays usable on a
    // narrower terminal than the one it was written on); the automatic one is only used when the
    // page really has the room, so a page too small for a poster is still a page of lyrics.
    let width = match panes.size(Pane::Mv) {
        0 => poster_rows(inner.height) * 2,
        dragged => clamp(Pane::Mv, Axis::Columns, dragged, inner.width),
    };
    if inner.width < width + PANEL_GAP + MIN_LYRICS_WIDTH {
        return (inner, None);
    }

    let [lyrics, _, panel] = Layout::horizontal([
        Constraint::Min(MIN_LYRICS_WIDTH),
        Constraint::Length(PANEL_GAP),
        Constraint::Length(width),
    ])
    .areas(inner);
    (lyrics, Some(panel))
}

/// Rows the poster takes on a page of `height` rows: everything the text under it does not need,
/// up to the tallest poster the panel draws.
fn poster_rows(height: u16) -> u16 {
    height.saturating_sub(PANEL_TEXT_ROWS).min(MAX_POSTER_ROWS)
}

/// Draw the MV in its column: the poster, and under it what the API knows about it — the title,
/// the singer, the length with the release date, and as much of the blurb as the rows left under
/// those can hold. The blurb wraps into its own area and is cut by it, the way the artist page's
/// biography is, so it can never reach the frame or the lyrics beside it.
pub(super) fn draw_panel(f: &mut Frame, area: Rect, panel: &mut MvPanel, colors: &Theme) {
    // The column is twice as wide as the poster is tall, so this is the same square
    // [`panel_split`] gave the column; the facts start directly under it.
    let [poster, text] =
        Layout::vertical([Constraint::Length(area.width / 2), Constraint::Min(0)]).areas(area);

    // No mask: a poster is a rectangle, where the disc in the player bar is a circle.
    f.render_stateful_widget(
        StatefulImage::new().resize(Resize::Fit(None)),
        poster,
        &mut panel.poster,
    );

    let info = &panel.info;
    let width = usize::from(text.width);
    let mut facts = format_duration(info.duration);
    if !info.publish_time.is_empty() {
        facts.push_str(" · ");
        facts.push_str(&info.publish_time);
    }

    let [facts_area, _, blurb_area] = Layout::vertical([
        Constraint::Length(FACTS_ROWS),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(text);

    let lines = vec![
        Line::from(Span::styled(
            clip_long_text(&info.name, width),
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            clip_long_text(&info.artist_name, width),
            Style::default().fg(colors.text),
        )),
        Line::from(Span::styled(
            clip_long_text(&facts, width),
            Style::default().fg(colors.muted),
        )),
    ];
    f.render_widget(Paragraph::new(lines), facts_area);

    if blurb_area.is_empty() || info.desc.trim().is_empty() {
        return;
    }
    let blurb = Paragraph::new(info.desc.trim())
        .style(Style::default().fg(colors.muted))
        .wrap(Wrap { trim: true });
    f.render_widget(blurb, blurb_area);
}

#[cfg(test)]
mod panel_tests {
    use std::collections::HashSet;

    use ncm_api::SongInfo;
    use ratatui::{
        Terminal,
        backend::TestBackend,
        buffer::Buffer,
        style::Color,
    };
    use ratatui_image::picker::Picker;

    use super::super::{draw, lines::lyrics};
    use super::*;
    use crate::utils::Named;
    use crate::{
        config::{BorderConfig, LyricStyle, lyrics::LyricsConfig},
        layout::Dividers,
        playback::PlaybackState,
        state::{
            lyrics::LyricsState,
            mv::{self, fixtures},
        },
        ui::{BlockStyle, block::CornerBlock},
        utils::GradientPreset,
    };

    /// The page's title, so the tests' `inner` is the one `draw` computes from the same block.
    const TITLE: &str = "LYRICS";

    /// A song with an MV, sung on the page the tests render.
    fn player() -> PlaybackState {
        PlaybackState {
            current_song: Some(std::sync::Arc::new(SongInfo {
                id: 1,
                name: "test".into(),
                singer: String::new(),
                artist_id: 0,
                album: String::new(),
                album_id: 0,
                pic_url: String::new(),
                duration: 60_000,
                mv: 7,
                copyright: ncm_api::SongCopyright::Free,
                local_path: None,
            })),
            lyrics: Some(lyrics()),
            position_secs: 22.0,
            ..PlaybackState::default()
        }
    }

    /// Render `player`'s page at `width × height`, and hand back the frame with the area `draw`
    /// divides between the lyrics and the poster — computed the way it does, so an assertion
    /// about the panel is an assertion about the column it really gets.
    fn render(
        player: &PlaybackState,
        style: LyricStyle,
        width: u16,
        height: u16,
    ) -> (Buffer, Rect) {
        let (buffer, inner, _) = render_with(player, style, width, height, &mut Dividers::default());

        (buffer, inner)
    }

    /// The same, handing back the dividers the page registered — a pane the mouse cannot land on
    /// is a pane that cannot be dragged.
    fn render_with(
        player: &PlaybackState,
        style: LyricStyle,
        width: u16,
        height: u16,
        dividers: &mut Dividers,
    ) -> (Buffer, Rect, Dividers) {
        let theme = Theme::default();
        let bs = BlockStyle {
            colors: &theme,
            base: theme.bg,
            border: &BorderConfig::default(),
            tick: 0,
        };
        let frame = Rect::new(0, 0, width, height);
        let inner = CornerBlock::from_color(&bs, bs.base)
            .title(TITLE, bs.colors)
            .inner(frame);

        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("backend");
        terminal
            .draw(|f| {
                draw(
                    f,
                    player,
                    &bs,
                    &LyricsConfig {
                        style,
                        gradient: GradientPreset::Rainbow,
                        ktv_color: Color::Rgb(77, 166, 255),
                        show_translation: true,
                        title: TITLE,
                    },
                    &mut LyricsState::default(),
                    &PanesConfig::default(),
                    dividers,
                    f.area(),
                );
            })
            .expect("draw");
        (
            terminal.backend().buffer().clone(),
            inner,
            std::mem::take(dividers),
        )
    }

    /// Every cell of the frame as it will be painted — the symbol and both colours — so two
    /// frames compare the way the terminal would receive them, not just as text.
    fn frame_text(buffer: &Buffer) -> String {
        let mut out = String::with_capacity(buffer.content.len() * 8);
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                let cell = &buffer[(x, y)];
                out.push_str(cell.symbol());
                out.push_str(&format!("{:?}{:?}|", cell.fg, cell.bg));
            }
            out.push('\n');
        }
        out
    }

    /// The rows of `area` as text, which is what a reader sees in that column.
    fn rows_in(buffer: &Buffer, area: Rect) -> Vec<String> {
        (area.top()..area.bottom())
            .map(|y| {
                (area.left()..area.right())
                    .map(|x| buffer[(x, y)].symbol())
                    .collect()
            })
            .collect()
    }

    /// Whether any of `rows` spells `text`. The comparison ignores spacing, because a cell grid
    /// writes the gap behind a wide glyph as a cell of its own: `MV标题` comes back as
    /// `MV 标 题`.
    fn spells(rows: &[String], text: &str) -> bool {
        let squeezed = |s: &str| -> String { s.chars().filter(|c| !c.is_whitespace()).collect() };
        let wanted = squeezed(text);
        rows.iter().any(|row| squeezed(row).contains(&wanted))
    }

    /// The positions of the blurb's characters on the frame. Nothing else on the page spells
    /// them — the lyrics are `line0`…, the facts are digits — so where they are is where the
    /// blurb was drawn.
    fn blurb_cells(buffer: &Buffer) -> Vec<(u16, u16)> {
        let spelled: HashSet<char> = fixtures::DESC
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let mut cells = Vec::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                if buffer[(x, y)]
                    .symbol()
                    .chars()
                    .any(|c| spelled.contains(&c))
                {
                    cells.push((x, y));
                }
            }
        }
        cells
    }

    /// How many cells of `area` carry a colour the page's own text never uses. Every string is
    /// theme-coloured, so those cells are the poster's pixels.
    fn poster_cells(buffer: &Buffer, area: Rect, theme: &Theme) -> usize {
        let palette = [
            theme.bg,
            theme.surface,
            theme.text,
            theme.accent,
            theme.muted,
            theme.border,
            theme.error,
            Color::Reset,
            Color::White,
        ];
        (area.top()..area.bottom())
            .flat_map(|y| (area.left()..area.right()).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let cell = &buffer[(x, y)];
                !palette.contains(&cell.fg) || !palette.contains(&cell.bg)
            })
            .count()
    }

    /// The panel takes a column off the right edge only when there is one to draw and the page
    /// can hold it; every other answer is the lyrics area untouched, which is what keeps the
    /// page the page it was. The column is as wide as the page's height can afford, up to the
    /// tallest poster there is.
    #[test]
    fn the_panel_is_only_handed_a_column_when_the_page_can_hold_it() {
        let tall = Rect::new(0, 0, 100, 40);
        let (lyrics, panel) = panel_split(tall, &PanesConfig::default(), true);
        let panel = panel.expect("a 100×40 page holds the panel");
        assert_eq!(
            panel.width,
            MAX_POSTER_ROWS * 2,
            "a page with rows to spare gets the tallest poster"
        );
        assert_eq!(panel, Rect::new(100 - panel.width, 0, panel.width, 40));
        assert_eq!(
            lyrics.width,
            100 - panel.width - PANEL_GAP,
            "the lyrics keep everything the poster and the gap do not take"
        );

        let small = Rect::new(0, 0, 100, MIN_PANEL_HEIGHT);
        let (lyrics, panel) = panel_split(small, &PanesConfig::default(), true);
        let panel = panel.expect("the shortest page that holds the panel does hold it");
        assert_eq!(
            panel.width,
            MIN_POSTER_ROWS * 2,
            "a shorter page gets a smaller poster rather than none"
        );
        assert_eq!(lyrics.width, 100 - panel.width - PANEL_GAP);

        assert_eq!(
            panel_split(tall, &PanesConfig::default(), false),
            (tall, None),
            "a song with no poster is handed no column"
        );
        let short = Rect::new(0, 0, 100, MIN_PANEL_HEIGHT - 1);
        assert_eq!(panel_split(short, &PanesConfig::default(), true), (short, None));
        let narrow = Rect::new(
            0,
            0,
            MAX_POSTER_ROWS * 2 + PANEL_GAP + MIN_LYRICS_WIDTH - 1,
            40,
        );
        assert_eq!(panel_split(narrow, &PanesConfig::default(), true), (narrow, None));
        assert_eq!(
            panel_split(Rect::new(0, 0, 60, 12), &PanesConfig::default(), true),
            (Rect::new(0, 0, 60, 12), None),
            "the page the other tests draw on"
        );
    }

    /// The MV column is a pane like the sidebar: `[panes] mv` is its width, collapsing it hands
    /// the whole page to the lyrics, and a width the page cannot hold is clamped rather than
    /// taken out of the lyrics — a config written on a wide terminal has to stay usable on a
    /// narrow one.
    #[test]
    fn the_mv_column_is_a_pane() {
        let page = Rect::new(0, 0, 100, 30);
        let auto = panel_split(page, &PanesConfig::default(), true)
            .1
            .expect("the default is the width the page can spare");
        assert_eq!(auto.width, poster_rows(page.height) * 2);

        let dragged = PanesConfig {
            mv: 30,
            ..PanesConfig::default()
        };
        let panel = panel_split(page, &dragged, true)
            .1
            .expect("a dragged width is used");
        assert_eq!(panel.width, 30);
        assert_eq!(panel.right(), page.right(), "the column stays on the right");

        let collapsed = PanesConfig {
            collapsed: vec![Pane::Mv],
            ..PanesConfig::default()
        };
        assert_eq!(
            panel_split(page, &collapsed, true),
            (page, None),
            "a collapsed pane is not drawn, whatever the page could hold"
        );

        // Wider than the page: clamped, so the lyrics keep their columns.
        let too_wide = PanesConfig {
            mv: 200,
            ..PanesConfig::default()
        };
        let (lyrics, panel) = panel_split(page, &too_wide, true);
        assert!(panel.is_some(), "a clamped column is still a column");
        assert!(
            lyrics.width >= MIN_LYRICS_WIDTH,
            "the lyrics keep their columns: {lyrics:?}"
        );
    }

    /// The column's edge is the divider the mouse lands on, and dragging it left has to make the
    /// column wider — the pane is on the right, so its edge works the other way round from the
    /// sidebar's.
    #[tokio::test]
    async fn the_mv_edge_is_draggable_and_grows_leftwards() {
        let _turn = fixtures::turn().await;
        mv::clear();
        assert!(mv::install(
            mv::generation(),
            fixtures::panel(&Picker::halfblocks())
        ));

        let mut dividers = Dividers::default();
        let (_, inner, dividers) =
            render_with(&player(), LyricStyle::Window, 100, 30, &mut dividers);
        let (_, panel) = panel_split(inner, &PanesConfig::default(), true);
        let panel = panel.expect("a 100×30 page holds the panel");

        let divider = dividers
            .iter()
            .find(|divider| divider.pane == Pane::Mv)
            .expect("the page registers the MV's edge");
        assert_eq!(divider.axis, Axis::Columns);
        assert_eq!(divider.rect.x, panel.x - 1, "the edge is the column beside it");
        assert_eq!(divider.rect.height, panel.height);
        assert_eq!(
            divider.size_for(30, -4),
            34,
            "dragging the edge left makes the column wider"
        );

        mv::clear();
    }

    /// With a poster in the slot, the panel's column paints the poster and writes the MV's own
    /// facts in it, while the lyrics stay in the columns they had.
    #[tokio::test]
    async fn a_panel_paints_its_poster_and_its_facts_in_its_own_column() {
        let _turn = fixtures::turn().await;
        let theme = Theme::default();
        mv::clear();
        assert!(mv::install(
            mv::generation(),
            fixtures::panel(&Picker::halfblocks())
        ));

        let (buffer, inner) = render(&player(), LyricStyle::Window, 100, 30);
        let (lyrics_area, panel) = panel_split(inner, &PanesConfig::default(), true);
        let panel = panel.expect("a 100×30 page holds the panel");

        let poster_rows = panel.width / 2;
        let painted = poster_cells(&buffer, panel, &theme);
        assert!(
            painted >= panel.width as usize * poster_rows as usize / 2,
            "the poster should paint its half of the column, {painted} cells did"
        );

        let rows = rows_in(&buffer, panel);
        for fact in [
            fixtures::NAME,
            fixtures::ARTIST,
            &format_duration(fixtures::DURATION_MS),
            fixtures::PUBLISH_TIME,
        ] {
            assert!(
                spells(&rows, fact),
                "{fact:?} is not in the panel's column: {rows:?}"
            );
        }
        assert!(
            !spells(&rows, fixtures::DESC),
            "the blurb is a paragraph, not one line: {rows:?}"
        );
        assert!(
            blurb_cells(&buffer)
                .iter()
                .all(|&(x, _)| { x >= panel.left() && x < panel.right() }),
            "the blurb must stay in the panel's column"
        );

        let lyrics_rows = rows_in(&buffer, lyrics_area);
        assert!(
            spells(&lyrics_rows, "line4"),
            "the lyrics still draw beside the poster: {lyrics_rows:?}"
        );

        mv::clear();
    }

    /// The rows under the facts are all the blurb gets: what it holds is drawn there, and the
    /// rest of it is dropped rather than written past the panel.
    #[tokio::test]
    async fn the_blurb_is_cut_to_the_rows_the_panel_has_left() {
        let _turn = fixtures::turn().await;
        mv::clear();
        assert!(mv::install(
            mv::generation(),
            fixtures::panel(&Picker::halfblocks())
        ));

        // The shortest page that still holds the panel: poster, facts and one row of blurb.
        let (buffer, inner) = render(&player(), LyricStyle::Window, 100, MIN_PANEL_HEIGHT + 2);
        let (_, panel) = panel_split(inner, &PanesConfig::default(), true);
        let panel = panel.expect("this is the page the panel is measured against");

        let drawn = blurb_cells(&buffer).len();
        let whole = fixtures::DESC
            .chars()
            .filter(|c| !c.is_whitespace())
            .count();
        assert!(drawn > 0, "the row the blurb has should hold some of it");
        assert!(
            drawn < whole,
            "the blurb should be cut, not all {whole} characters of it drawn"
        );

        let rows = rows_in(&buffer, panel);
        assert!(
            spells(&rows, fixtures::PUBLISH_TIME),
            "the facts stay when the blurb is cut: {rows:?}"
        );

        mv::clear();
    }

    /// All four presentations keep working beside the panel: the lyrics stay in their column and
    /// the poster in its own, whichever one is drawn.
    #[tokio::test]
    async fn every_style_draws_beside_the_panel() {
        let _turn = fixtures::turn().await;
        let theme = Theme::default();
        mv::clear();
        assert!(mv::install(
            mv::generation(),
            fixtures::panel(&Picker::halfblocks())
        ));

        for style in LyricStyle::ALL.iter().copied() {
            let (buffer, inner) = render(&player(), style, 100, 30);
            let (lyrics_area, panel) = panel_split(inner, &PanesConfig::default(), true);
            let panel = panel.expect("a 100×30 page holds the panel");
            assert!(
                spells(&rows_in(&buffer, panel), fixtures::NAME),
                "{style:?} drew over the panel"
            );
            assert!(
                spells(&rows_in(&buffer, lyrics_area), "line4"),
                "{style:?} lost the line being sung"
            );
            assert!(
                poster_cells(&buffer, panel, &theme) > 0,
                "{style:?} painted the poster out of its column"
            );
        }

        mv::clear();
    }

    /// The poster belongs to the song, not to its lyrics: while the lyrics are still on their
    /// way — or when the song has none to fetch — the panel is drawn, and it is the page there
    /// is to look at.
    #[tokio::test]
    async fn the_panel_is_drawn_before_the_lyrics_arrive() {
        let _turn = fixtures::turn().await;
        mv::clear();
        assert!(mv::install(
            mv::generation(),
            fixtures::panel(&Picker::halfblocks())
        ));

        let mut pending = player();
        pending.lyrics = None;
        let (buffer, inner) = render(&pending, LyricStyle::Window, 100, 30);
        let (_, panel) = panel_split(inner, &PanesConfig::default(), true);
        let panel = panel.expect("the page holds the panel");
        assert!(
            spells(&rows_in(&buffer, panel), fixtures::NAME),
            "the panel is not drawn while the lyrics are loading"
        );

        mv::clear();
    }

    /// A page too small for the panel is byte for byte the page it was before the panel
    /// existed, in every style: the poster waiting in the slot changes nothing.
    #[tokio::test]
    async fn a_page_too_small_for_the_panel_is_the_page_without_one() {
        let _turn = fixtures::turn().await;
        mv::clear();
        let before: Vec<String> = LyricStyle::ALL
            .iter()
            .map(|style| frame_text(&render(&player(), *style, 60, 12).0))
            .collect();

        assert!(mv::install(
            mv::generation(),
            fixtures::panel(&Picker::halfblocks())
        ));
        for (style, page) in LyricStyle::ALL.iter().zip(before) {
            assert_eq!(
                frame_text(&render(&player(), *style, 60, 12).0),
                page,
                "{style:?} drew a different page with a poster in the slot"
            );
        }

        mv::clear();
    }

    /// The same is true of a page with no poster at all: nothing of the MV reaches it — not one
    /// of its words, and not its pixels.
    #[tokio::test]
    async fn a_page_without_a_panel_draws_none_of_the_mv() {
        let _turn = fixtures::turn().await;
        let theme = Theme::default();
        mv::clear();

        // The plain style paints no gradient, so the only colours on this page are the theme's:
        // any other colour in the frame would be a pixel of a poster that should not be there.
        let (buffer, inner) = render(&player(), LyricStyle::Plain, 100, 30);
        assert_eq!(
            panel_split(inner, &PanesConfig::default(), false),
            (inner, None),
            "the lyrics keep the whole page"
        );

        let rows = rows_in(&buffer, inner);
        for fact in [fixtures::NAME, fixtures::ARTIST, fixtures::PUBLISH_TIME] {
            assert!(
                !spells(&rows, fact),
                "{fact:?} is on a page that has no panel: {rows:?}"
            );
        }
        assert!(blurb_cells(&buffer).is_empty());
        assert_eq!(
            poster_cells(&buffer, inner, &theme),
            0,
            "a page without a panel paints nothing but the theme"
        );
    }
}


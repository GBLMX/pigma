//! Regenerate the README's screenshots.
//!
//! The pages are drawn by the app itself, offscreen, into the same `TestBackend` the tests use —
//! so a screenshot is what the app really renders, with a real theme, and can be regenerated
//! when the UI changes. Each page is written as HTML (one `<span>` per run of cells that share a
//! colour) next to a plain-text dump, and the HTML is what gets rasterised into `imgs/`.
//!
//! `#[ignore]`d because it writes files: run it deliberately.

use std::{fmt::Write as _, fs, path::PathBuf};

use ratatui::{
    Terminal,
    backend::TestBackend,
    style::{Color, Modifier},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::App,
    config::Config,
    state::{ArtistData, ContentState, Page},
    ui,
    utils::terminal::{ANSI_16, palette_rgb},
};

/// Cells per side: with the previews' 8×16 px cell that is 1280×768 — a terminal someone
/// might actually run, and wide enough for the panes to lay out the way the preview shows.
const COLS: u16 = 160;
const ROWS: u16 = 48;

/// The colour a ratatui `Color` stands for in CSS, or `None` for "whatever is behind it"
/// (`Color::Reset`), which the page's own background answers.
fn css(color: Color) -> Option<String> {
    let (r, g, b) = match color {
        Color::Reset => return None,
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Indexed(index) => palette_rgb(index),
        Color::Black => ANSI_16[0],
        Color::Red => ANSI_16[1],
        Color::Green => ANSI_16[2],
        Color::Yellow => ANSI_16[3],
        Color::Blue => ANSI_16[4],
        Color::Magenta => ANSI_16[5],
        Color::Cyan => ANSI_16[6],
        Color::Gray => ANSI_16[7],
        Color::DarkGray => ANSI_16[8],
        Color::LightRed => ANSI_16[9],
        Color::LightGreen => ANSI_16[10],
        Color::LightYellow => ANSI_16[11],
        Color::LightBlue => ANSI_16[12],
        Color::LightMagenta => ANSI_16[13],
        Color::LightCyan => ANSI_16[14],
        Color::White => ANSI_16[15],
    };
    Some(format!("#{r:02x}{g:02x}{b:02x}"))
}

fn escape(symbol: &str, out: &mut String) {
    for ch in symbol.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            ' ' => out.push_str("&nbsp;"),
            _ => out.push(ch),
        }
    }
}

/// The whole frame as one `<pre>`, one `<span>` per run of cells that share their look.
fn html(buffer: &ratatui::buffer::Buffer, background: Color, foreground: Color) -> String {
    let bg = css(background).unwrap_or_else(|| "#000000".to_string());
    let fg = css(foreground).unwrap_or_else(|| "#ffffff".to_string());
    let mut out = String::new();
    let _ = write!(
        out,
        "<!doctype html><meta charset=\"utf-8\">\
         <style>html,body{{margin:0;padding:0;background:{bg}}}\
         pre{{margin:0;padding:0;font:16px/16px 'MS Gothic',monospace;\
         white-space:pre;background:{bg};color:{fg}}}</style><pre>"
    );

    for y in 0..buffer.area.height {
        let mut run: Option<(Option<String>, Option<String>, Modifier)> = None;
        let mut text = String::new();
        let flush = |run: &mut Option<(Option<String>, Option<String>, Modifier)>,
                     text: &mut String,
                     out: &mut String| {
            if let Some((fg, bg, modifier)) = run.take() {
                let mut style = String::new();
                if let Some(fg) = fg {
                    let _ = write!(style, "color:{fg};");
                }
                if let Some(bg) = bg {
                    let _ = write!(style, "background:{bg};");
                }
                if modifier.contains(Modifier::BOLD) {
                    style.push_str("font-weight:bold;");
                }
                if modifier.contains(Modifier::ITALIC) {
                    style.push_str("font-style:italic;");
                }
                if modifier.contains(Modifier::UNDERLINED) {
                    style.push_str("text-decoration:underline;");
                }
                if modifier.contains(Modifier::DIM) {
                    style.push_str("opacity:0.6;");
                }
                if modifier.contains(Modifier::REVERSED) {
                    style.push_str("filter:invert(1);");
                }
                let _ = write!(out, "<span style=\"{style}\">");
                escape(text, out);
                out.push_str("</span>");
            }
            text.clear();
        };

        // A wide glyph occupies two cells, and ratatui fills the second one with a space.
        // The font draws the glyph two cells wide on its own, so that space would be a
        // third cell and every column after it would drift.
        let mut continuation = false;
        for x in 0..buffer.area.width {
            let cell = &buffer[(x, y)];
            let symbol = cell.symbol();
            if continuation {
                continuation = false;
                continue;
            }
            let look = (css(cell.fg), css(cell.bg), cell.modifier);
            if run.as_ref() != Some(&look) {
                flush(&mut run, &mut text, &mut out);
                run = Some(look);
            }
            text.push_str(symbol);
            continuation = UnicodeWidthStr::width(symbol) >= 2;
        }
        flush(&mut run, &mut text, &mut out);
        out.push('\n');
    }
    out.push_str("</pre>\n");
    out
}

fn out_dir() -> PathBuf {
    let dir = PathBuf::from("target/shots");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn shoot(name: &str, setup: fn(&mut App)) {
    let mut app = App::new(Config::default(), false).expect("app");
    // A screenshot is published: it must not carry anyone's account. The app reads the
    // real config directory, so the logged-in user has to be dropped before anything is
    // drawn — the topbar shows their name otherwise.
    app.state.navigation.user = None;
    setup(&mut app);
    app.state.navigation.user = None;
    // The app starts on the splash; a preview is of the page behind it unless the setup
    // asked for another one.
    if app.state.navigation.page == Page::Splash {
        app.state.navigation.page = Page::Main;
    }
    let theme = app.current_theme().clone();
    let mut terminal = Terminal::new(TestBackend::new(COLS, ROWS)).expect("backend");
    terminal.draw(|f| ui::draw(f, &mut app)).expect("draw");
    let buffer = terminal.backend().buffer().clone();

    let mut text = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    let dir = out_dir();
    fs::write(
        dir.join(format!("{name}.html")),
        html(&buffer, theme.bg, theme.text),
    )
    .expect("html");
    fs::write(dir.join(format!("{name}.txt")), text).expect("text");
    println!(
        "  {} -> {}",
        name,
        dir.join(format!("{name}.html")).display()
    );
}

/// A handful of songs, so the tables are not empty.
fn songs(count: u64) -> Vec<std::sync::Arc<ncm_api::SongInfo>> {
    (1..=count)
        .map(|id| {
            std::sync::Arc::new(ncm_api::SongInfo {
                id,
                name: format!("夜航星 {id}"),
                singer: "不才".into(),
                artist_id: 0,
                album: "三体 电视剧原声带".into(),
                album_id: 0,
                pic_url: String::new(),
                duration: 240_000,
                mv: 0,
                copyright: ncm_api::SongCopyright::Free,
                local_path: None,
            })
        })
        .collect()
}

#[tokio::test]
#[ignore = "writes target/shots/*.html for the README's previews"]
async fn render_the_readme_previews() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    shoot("main", |app| {
        app.state
            .navigation
            .set_content(ContentState::Songs(songs(18)));
    });
    shoot("artist", |app| {
        app.state.navigation.page = Page::Artist;
        let detail = ncm_api::ArtistDetail {
            id: 6452,
            name: "周杰伦".into(),
            alias: vec!["Jay Chou".into()],
            brief_desc: "华语流行歌手、音乐人。".into(),
            pic_url: String::new(),
            album_size: 42,
            music_size: 511,
            hot_songs: songs(12).into_iter().map(|song| (*song).clone()).collect(),
        };
        app.state.navigation.artist.id = 6452;
        app.state.navigation.artist.name = "周杰伦".into();
        app.state.navigation.artist.data = ArtistData::Ready {
            detail,
            albums: Ok((1..=8)
                .map(|n| ncm_api::ArtistAlbum {
                    id: n,
                    name: format!("专辑 {n}"),
                    pic_url: String::new(),
                    size: 10 + n,
                    publish_time: 1_500_000_000_000 + n * 86_400_000,
                })
                .collect()),
            similar: Ok(vec![
                ncm_api::SingerInfo {
                    id: 6452,
                    name: "林俊杰".into(),
                    pic_url: String::new(),
                },
                ncm_api::SingerInfo {
                    id: 2,
                    name: "王力宏".into(),
                    pic_url: String::new(),
                },
                ncm_api::SingerInfo {
                    id: 3,
                    name: "陈奕迅".into(),
                    pic_url: String::new(),
                },
            ]),
        };
    });
    shoot("settings", |app| {
        app.state.navigation.page = Page::Settings;
    });
    shoot("queue", |app| {
        app.state.navigation.page = Page::Playlist;
        app.playback
            .set_queue_songs(songs(24).into_iter().collect());
    });
    shoot("panel", |app| {
        app.state
            .navigation
            .set_content(ContentState::Songs(songs(18)));
        app.state.command_panel.open = true;
    });
}

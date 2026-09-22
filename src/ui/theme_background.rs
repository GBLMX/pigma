//! The theme has to own the background, not just the text colours.

use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, style::Color};

use crate::{
    app::App,
    config::Config,
    utils::terminal::{Background, BackgroundFill},
};

/// A frame drawn on a terminal whose own background is `terminal`, with `fill` deciding
/// whether the theme's background is painted over it — and the theme's own background colour.
///
/// The theme is a *light* one (`github-light`, whichever slot the config resolves to), so
/// "the theme and the terminal agree" is a state a test can set on any machine.
fn frame(fill: BackgroundFill, terminal: Background) -> (Buffer, Color) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let config = Config {
        default_theme: "github-light".to_string(),
        paint_background: fill,
        ..Config::default()
    };
    let mut app = App::new(config, false).expect("app");
    app.terminal_background = terminal;
    // The main page with its content still loading: what a song switch shows.
    app.state.navigation.page = crate::state::Page::Main;
    app.state.navigation.content = crate::state::ContentState::Loading.into();

    let theme = app
        .theme_registry
        .get("github-light")
        .expect("a built-in theme")
        .clone();
    assert_eq!(
        theme.background(),
        Background::Light,
        "the fixture's theme must be the light one"
    );

    let mut terminal = Terminal::new(TestBackend::new(80, 20)).expect("backend");
    terminal.draw(|f| super::draw(f, &mut app)).expect("draw");

    (terminal.backend().buffer().clone(), theme.bg)
}

/// Cells that leave the background to the terminal: what a translucent terminal shows
/// through, and what acrylic blurs.
///
/// The blank cell a wide glyph leaves behind it (`未` and the half-cell ratatui writes after
/// it) is deliberately not counted: it carries no style of its own because the terminal
/// paints it as part of the glyph, so nobody sees the terminal's background through it.
fn transparent(buffer: &Buffer) -> Vec<(u16, u16, String)> {
    use unicode_width::UnicodeWidthStr;

    let mut cells = Vec::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            if buffer[(x, y)].bg != Color::Reset {
                continue;
            }
            let after_wide = x > 0 && UnicodeWidthStr::width(buffer[(x - 1, y)].symbol()) > 1;
            if !after_wide {
                cells.push((x, y, buffer[(x, y)].symbol().to_string()));
            }
        }
    }
    cells
}

/// `always`: the theme's background over every cell, which is the fix for a light theme in a
/// dark terminal — thin light-grey text on a dark screen without it.
#[tokio::test]
async fn always_paints_the_theme_over_every_cell() {
    let (buffer, _) = frame(BackgroundFill::Always, Background::Dark);

    let showing = transparent(&buffer);
    assert!(
        showing.is_empty(),
        "{} cells still show the terminal's background, e.g. {:?}",
        showing.len(),
        &showing[..showing.len().min(6)]
    );
}

/// The default: with the theme's own background already the terminal's, the fill is
/// invisible while what it hides — the terminal's transparency — is not. So the base stays
/// the terminal's, and the surfaces the theme *does* paint are still painted.
#[tokio::test]
async fn auto_leaves_a_matching_background_to_the_terminal() {
    let (buffer, bg) = frame(BackgroundFill::Auto, Background::Light);

    assert!(
        !transparent(&buffer).is_empty(),
        "a matching theme painted over the terminal's own background anyway"
    );
    assert!(
        buffer
            .content
            .iter()
            .any(|cell| cell.bg != Color::Reset && cell.bg != bg),
        "nothing was painted at all: transparency is not the same as no theme"
    );
}

/// …and when the two disagree, the fill is what keeps the page readable.
#[tokio::test]
async fn auto_paints_when_the_theme_and_the_terminal_disagree() {
    let (buffer, _) = frame(BackgroundFill::Auto, Background::Dark);

    let showing = transparent(&buffer);
    assert!(
        showing.is_empty(),
        "a light theme on a dark terminal has to paint its background, but {} cells \
         still show the terminal's, e.g. {:?}",
        showing.len(),
        &showing[..showing.len().min(6)]
    );
}

/// `never`: the terminal keeps its background whatever the theme says — the mode for someone
/// who wants their blur more than the theme's background.
#[tokio::test]
async fn never_leaves_the_terminal_alone() {
    let (buffer, _) = frame(BackgroundFill::Never, Background::Dark);

    assert!(
        !transparent(&buffer).is_empty(),
        "the background was painted anyway"
    );
}

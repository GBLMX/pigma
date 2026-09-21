//! The `:messages` popup: what the app has said this session, newest first.
//!
//! The same window as the help popup — the keys and the scroll limit work the same way — because
//! it is the same kind of thing: a list longer than the screen, opened by a command and closed by
//! `Esc`.

use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Style,
    widgets::{Clear, Paragraph, Widget},
};

use super::{BlockStyle, block::CornerBlock};
use crate::{app::App, state::notices::Level};

const POPUP_WIDTH: u16 = 72;
const POPUP_HEIGHT: u16 = 18;

/// Draw the notices and hand back the scroll limit, so the caller can clamp exactly as the help
/// popup does.
pub(super) fn draw(f: &mut Frame, app: &App, area: Rect) -> usize {
    let colors = app.current_theme();
    let state = &app.state.messages;

    let popup_area = area.centered(
        Constraint::Length(POPUP_WIDTH),
        Constraint::Length(POPUP_HEIGHT),
    );

    let style = BlockStyle {
        colors,
        // A popup paints its own surface, so what is behind it never shows.
        base: colors.surface,
        border: &app.state.border,
        tick: app.state.tick,
    };
    let block = CornerBlock::from_color(&style, colors.surface)
        .title("\u{25BA} MESSAGES \u{25C4}", colors);
    let inner = block.inner(popup_area);

    f.render_widget(Clear, popup_area);
    block.render(popup_area, f.buffer_mut());

    let footer = format!(
        "{:>width$}",
        "Esc 关闭 · ↑↓ 滚动",
        width = (POPUP_WIDTH - 4) as usize
    );
    let footer_area = Rect {
        y: inner.y + inner.height.saturating_sub(1),
        height: 1,
        ..inner
    };
    f.render_widget(
        Paragraph::new(footer).style(Style::default().fg(colors.muted)),
        footer_area,
    );

    // Newest first: the reader opened this because of what just happened.
    let lines: Vec<(Level, String)> = app
        .state
        .notices
        .all()
        .rev()
        .map(|notice| (notice.level, notice.text.clone()))
        .collect();

    if lines.is_empty() {
        let line_area = Rect {
            y: inner.y,
            height: 1,
            ..inner
        };
        f.render_widget(
            Paragraph::new("  还没有消息").style(Style::default().fg(colors.muted)),
            line_area,
        );

        return 0;
    }

    let visible = (inner.height.saturating_sub(1)) as usize;
    let max_scroll = lines.len().saturating_sub(visible);
    for (i, (level, text)) in lines.iter().enumerate().skip(state.scroll).take(visible) {
        let line_area = Rect {
            y: inner.y + (i - state.scroll) as u16,
            height: 1,
            ..inner
        };
        let (mark, color) = match level {
            Level::Info => ("·", colors.text),
            Level::Warn => ("!", colors.accent),
            Level::Error => ("✗", colors.error),
        };
        f.render_widget(
            Paragraph::new(format!("  {mark} {text}")).style(Style::default().fg(color)),
            line_area,
        );
    }

    max_scroll
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::state::notices::Level;

    /// The popup shows what the app said, newest first, and the level is what marks a line:
    /// a list that could not tell an error from a confirmation would be a list of noise.
    // The app brings up a Tokio runtime, like the other tests that build one.
    #[tokio::test]
    async fn the_popup_lists_the_notices_newest_first() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut app = App::new(crate::config::Config::default(), false).expect("app");
        app.notice(Level::Info, "刚刚做了什么".to_string());
        app.notice(Level::Error, "失败了".to_string());
        app.state.messages.open = true;

        let mut terminal = Terminal::new(TestBackend::new(POPUP_WIDTH, POPUP_HEIGHT)).expect("tty");
        terminal
            .draw(|f| {
                draw(f, &app, f.area());
            })
            .expect("draw");

        let buffer = terminal.backend().buffer();
        // A wide character leaves the cell behind it empty, so the frame is compared with the
        // blanks taken out — the way the terminal shows it, not the way the buffer stores it.
        let text: String = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<String>>()
            .join("|")
            .replace(' ', "");

        assert!(text.contains("MESSAGES"), "{text}");
        assert!(text.contains("✗失败了"), "{text}");
        assert!(text.contains("·刚刚做了什么"), "{text}");
        assert!(
            text.find("失败了").unwrap() < text.find("刚刚做了什么").unwrap(),
            "the newest is the first line: {text}"
        );
    }
}

use ncm_api::LoginInfo;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

use super::{BlockStyle, block::CornerBlock};
use crate::{
    config::Theme,
    state::{PromptState, SearchState},
};

pub(super) fn draw(
    f: &mut Frame,
    user: Option<&LoginInfo>,
    search: &SearchState,
    prompt: &PromptState,
    bs: &BlockStyle<'_>,
    area: Rect,
) {
    let colors = bs.colors;
    let block = CornerBlock::from_color(bs, bs.colors.bg);
    let inner = block.inner(area);
    f.render_widget(block, area);

    if prompt.active {
        render_prompt(f, prompt, colors, inner);
        return;
    }

    if search.active {
        render_search(f, search, colors, inner);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(38),
            Constraint::Percentage(22),
        ])
        .split(inner);

    let logo = Line::from(vec![
        Span::styled("▓ ", Style::default().fg(colors.accent)),
        Span::styled(
            "BOXPIGMA",
            Style::default()
                .fg(colors.text)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    f.render_widget(Paragraph::new(logo), chunks[0]);

    let mut right_line = Line::default();

    if let Some(info) = user {
        right_line.push_span(Span::styled(
            &info.nickname,
            Style::default().fg(colors.text),
        ));
        right_line.push_span(Span::styled("  ", Style::default()));
        match info.vip_type {
            10 | 11 => {
                right_line.push_span(Span::styled(
                    "♛VIP",
                    Style::default()
                        .fg(colors.accent)
                        .add_modifier(Modifier::BOLD),
                ));
                right_line.push_span(Span::styled("  ", Style::default()));
            }
            _ => {}
        }
    } else {
        right_line.push_span(Span::styled(
            "未登录（按 L 登录）",
            Style::default().fg(colors.muted),
        ));
    }

    // right_line.push(Span::styled("v0.1.0", Style::default().fg(colors.muted)));

    let right_line = right_line.alignment(Alignment::Right);
    f.render_widget(Paragraph::new(right_line), chunks[2]);
}

fn render_prompt(f: &mut Frame, prompt: &PromptState, colors: &Theme, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            ":",
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        ))),
        chunks[0],
    );

    // The whole command line is `accent`, the same blue as the `:` that introduces it: the
    // typed text used to be `text` (a dark grey) and the hint `muted`, which read as two
    // different greys next to the coloured prompt.
    let display = if prompt.input.value.is_empty() {
        Line::from(Span::styled(
            "命令（Tab 补全，Enter 执行，Esc 取消）",
            Style::default().fg(colors.accent),
        ))
    } else {
        Line::from(Span::styled(
            prompt.input.value.as_str(),
            Style::default().fg(colors.accent),
        ))
    };
    f.render_widget(Paragraph::new(display), chunks[1]);
    prompt
        .input
        .show_cursor_at(f, chunks[1].x, chunks[1].y, true, false);
}

fn render_search(f: &mut Frame, search: &SearchState, colors: &Theme, area: Rect) {
    let provider_width = {
        let name = search.provider.display_name();
        name.width() + 2
    };
    let chunks = if search.filter_queue_only {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(2), Constraint::Min(1)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(2),
                Constraint::Min(1),
                Constraint::Length(provider_width as u16),
            ])
            .split(area)
    };

    let icon = Line::from(Span::styled("\u{F002}", Style::default().fg(colors.accent)));
    f.render_widget(Paragraph::new(icon), chunks[0]);

    let value = &search.input.value;

    let placeholder = if search.filter_queue_only {
        " 过滤播放队列..."
    } else {
        " 搜索歌曲..."
    };

    // Same as the command line: the field is `accent` too, so a text input never looks
    // like it belongs to the terminal rather than to the theme.
    let display = if value.is_empty() {
        Line::from(Span::styled(
            placeholder,
            Style::default().fg(colors.accent),
        ))
    } else {
        Line::from(Span::styled(
            value.as_str(),
            Style::default().fg(colors.accent),
        ))
    };

    f.render_widget(Paragraph::new(display), chunks[1]);
    search
        .input
        .show_cursor_at(f, chunks[1].x, chunks[1].y, search.active, false);

    if !search.filter_queue_only {
        let provider = Line::from(vec![
            Span::styled(" ", Style::default().fg(colors.text)),
            Span::styled(
                search.provider.display_name(),
                Style::default()
                    .fg(colors.accent)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .alignment(Alignment::Right);
        f.render_widget(Paragraph::new(provider), chunks[2]);
    }
}

#[cfg(test)]
mod input_colour {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    /// Every built-in theme, so "the input uses the theme's own accent" is checked against
    /// all of them rather than against whichever one happened to be open.
    fn themes() -> Vec<(String, Theme)> {
        let registry = crate::config::ThemeRegistry::new(Default::default());
        registry
            .all_names()
            .iter()
            .map(|name| {
                (
                    (*name).to_string(),
                    registry.get(name).cloned().unwrap_or_default(),
                )
            })
            .collect()
    }

    /// Everything the command line shows is the same colour as the `:` in front of it: the
    /// typed text used to be a dark grey and the hint `muted`, which read as two unrelated
    /// greys next to the coloured prompt.
    #[test]
    fn the_command_line_uses_the_themes_accent() {
        for (name, colors) in themes() {
            for typed in ["", "theme github-light"] {
                let mut prompt = PromptState::default();
                prompt.input.value = typed.to_string();

                let mut terminal = Terminal::new(TestBackend::new(40, 1)).expect("backend");
                terminal
                    .draw(|f| render_prompt(f, &prompt, &colors, f.area()))
                    .expect("draw");
                let buffer = terminal.backend().buffer().clone();

                assert_eq!(buffer[(0, 0)].fg, colors.accent, "{name}: the colon itself");
                let mut cells = 0;
                // x = 0 is the colon; everything after it belongs to the command line.
                for x in 1..buffer.area.width {
                    let cell = &buffer[(x, 0)];
                    if cell.symbol().trim().is_empty() {
                        continue;
                    }
                    assert_eq!(
                        cell.fg, colors.accent,
                        "{name}, {typed:?}: ({x},0) draws {:?}, not the prompt colour",
                        cell.fg
                    );
                    cells += 1;
                }
                assert!(cells > 0, "{name}, {typed:?}: nothing after the colon");
            }
        }
    }

    /// The search box gets the same treatment, so no text field falls back to the terminal's
    /// own foreground.
    #[test]
    fn the_search_box_uses_the_themes_accent() {
        for (name, colors) in themes() {
            for typed in ["", "taylor swift"] {
                let mut search = SearchState {
                    active: true,
                    ..SearchState::default()
                };
                search.input.value = typed.to_string();

                let mut terminal = Terminal::new(TestBackend::new(60, 1)).expect("backend");
                terminal
                    .draw(|f| render_search(f, &search, &colors, f.area()))
                    .expect("draw");
                let buffer = terminal.backend().buffer().clone();

                let mut cells = 0;
                // 0 is the magnifier and the right end holds the provider name, which has
                // its own colour.
                for x in 2..56 {
                    let cell = &buffer[(x, 0)];
                    if cell.symbol().trim().is_empty() {
                        continue;
                    }
                    assert_eq!(
                        cell.fg, colors.accent,
                        "{name}, {typed:?}: ({x},0) draws {:?}",
                        cell.fg
                    );
                    cells += 1;
                }
                assert!(cells > 0, "{name}, {typed:?}: the field rendered nothing");
            }
        }
    }
}

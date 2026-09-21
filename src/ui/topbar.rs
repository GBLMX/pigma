use ncm_api::LoginInfo;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

use ratatui_image::{Resize, StatefulImage};

use super::{BlockStyle, block::CornerBlock};
use crate::{
    config::Theme,
    state::{PromptState, SearchState, avatar},
};

/// Columns the portrait takes at the right end of the bar, and the column of space between it
/// and the user's line. A terminal cell is twice as tall as it is wide, so six columns over
/// the bar's three rows are a square picture — which is what a round face needs.
const PORTRAIT_WIDTH: u16 = 6;
const PORTRAIT_GAP: u16 = 1;

pub(super) fn draw(
    f: &mut Frame,
    user: Option<&LoginInfo>,
    search: &SearchState,
    prompt: &PromptState,
    bs: &BlockStyle<'_>,
    area: Rect,
) {
    let colors = bs.colors;
    let block = CornerBlock::from_color(bs, bs.base);
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

    // The user's line keeps its own width: the portrait only takes columns the line was not
    // using, so a bar too narrow for both draws the line alone — and a bar whose portrait has
    // not landed (or failed) draws exactly what it drew before there was one. The portrait
    // needs the bar's own rows rather than the block's inner ones: a face has three rows, and
    // the bordered block leaves one.
    let mut line_area = chunks[2];
    let mut portrait_area = None;
    if user.is_some()
        && line_area.width >= right_line.width() as u16 + PORTRAIT_GAP + PORTRAIT_WIDTH
    {
        let [line, _, portrait] = Layout::horizontal([
            Constraint::Min(1),
            Constraint::Length(PORTRAIT_GAP),
            Constraint::Length(PORTRAIT_WIDTH),
        ])
        .areas(line_area);
        line_area = line;
        portrait_area = Some(Rect {
            y: area.y,
            height: area.height,
            ..portrait
        });
    }

    let right_line = right_line.alignment(Alignment::Right);
    f.render_widget(Paragraph::new(right_line), line_area);

    if let Some(portrait) = portrait_area {
        render_portrait(f, portrait);
    }
}

/// Draw the portrait in the columns the bar reserved for it. Nothing is drawn when there is
/// none: those columns stay as empty as the rest of the bar.
fn render_portrait(f: &mut Frame, area: Rect) {
    let mut portrait = avatar::portrait();
    let Some(protocol) = portrait.as_mut() else {
        return;
    };
    f.render_stateful_widget(
        StatefulImage::new().resize(Resize::Fit(None)),
        area,
        protocol,
    );
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

#[cfg(test)]
mod portrait {
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

    use super::*;
    use crate::{
        config::{BorderConfig, Theme},
        state::avatar::fixtures,
    };

    /// The bar is three rows tall, so that is what a frame of it is.
    const HEIGHT: u16 = 3;

    fn user(nickname: &str) -> LoginInfo {
        LoginInfo {
            code: 200,
            uid: 7,
            nickname: nickname.to_string(),
            avatar_url: "https://p1.music.126.net/avatar.jpg".to_string(),
            vip_type: 0,
            msg: String::new(),
        }
    }

    /// One frame of the topbar, drawn the way the shell draws it.
    fn frame(width: u16, user: Option<&LoginInfo>) -> Buffer {
        let colors = Theme::default();
        let border = BorderConfig::default();
        let bs = BlockStyle {
            colors: &colors,
            base: colors.bg,
            border: &border,
            tick: 0,
        };
        let mut terminal = Terminal::new(TestBackend::new(width, HEIGHT)).expect("backend");
        terminal
            .draw(|f| {
                draw(
                    f,
                    user,
                    &SearchState::default(),
                    &PromptState::default(),
                    &bs,
                    f.area(),
                )
            })
            .expect("draw");
        terminal.backend().buffer().clone()
    }

    /// The bar as a reader sees it.
    fn text(buffer: &Buffer) -> String {
        (0..HEIGHT)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The words in the bar, without the blanks between them: a wide glyph covers two cells
    /// and the second one is a blank.
    fn words(buffer: &Buffer) -> String {
        text(buffer).replace(' ', "")
    }

    /// Every cell the image painted. Halfblocks draws a picture as `▀`/`▄` cells, and no text
    /// this bar draws uses either glyph — so they are where the portrait is and nowhere else.
    fn painted(buffer: &Buffer) -> Vec<(u16, u16)> {
        let mut cells = Vec::new();
        for y in 0..HEIGHT {
            for x in 0..buffer.area.width {
                if matches!(buffer[(x, y)].symbol(), "▀" | "▄") {
                    cells.push((x, y));
                }
            }
        }
        cells
    }

    /// The portrait is drawn only where it has columns of its own. With one, the bar's right
    /// end carries the picture and the user's line keeps every word it had; without one — no
    /// user, nothing landed, or not enough room — the bar draws what it drew before there was
    /// a portrait at all.
    #[tokio::test]
    async fn the_portrait_takes_its_own_columns_or_nothing_at_all() {
        // The slot is one for the whole process, so this test takes its turn at it.
        let _turn = fixtures::turn().await;
        let picker = ratatui_image::picker::Picker::halfblocks();
        let user = user("听歌的人");
        const WIDE: u16 = 100;
        // The bordered block leaves one column at each end of the bar, and the portrait has
        // the last columns before the right border.
        let columns = (WIDE - 1 - PORTRAIT_WIDTH)..(WIDE - 1);

        // A login whose portrait has not landed yet: no picture anywhere on the bar.
        avatar::clear();
        let plain = frame(WIDE, Some(&user));
        assert!(
            painted(&plain).is_empty(),
            "no portrait has landed yet:\n{}",
            text(&plain)
        );
        assert!(
            words(&plain).contains("听歌的人"),
            "plain bar:\n{}",
            text(&plain)
        );

        // The download landed: the picture is on the columns the bar reserved for it, and the
        // line moved left by them — it was not drawn under them.
        assert!(avatar::install(
            avatar::session(),
            fixtures::portrait(&picker)
        ));
        let drawn = frame(WIDE, Some(&user));
        let marks = painted(&drawn);
        assert!(
            !marks.is_empty(),
            "the portrait was not drawn:\n{}",
            text(&drawn)
        );
        for (x, _) in &marks {
            assert!(
                columns.contains(x),
                "the picture reached column {x}, outside {columns:?}:\n{}",
                text(&drawn)
            );
        }
        assert!(
            words(&drawn).contains("听歌的人"),
            "the line lost its words to the portrait:\n{}",
            text(&drawn)
        );
        for x in columns.clone() {
            for y in 0..HEIGHT {
                let symbol = drawn[(x, y)].symbol();
                assert!(
                    !"听歌的人".contains(symbol),
                    "the line was drawn under the portrait at ({x},{y}):\n{}",
                    text(&drawn)
                );
            }
        }

        // A bar with no room for both keeps the line, which is the whole reason the portrait
        // is measured before it is drawn.
        let narrow = frame(60, Some(&user));
        assert!(
            painted(&narrow).is_empty(),
            "a bar too narrow for both must draw the line alone:\n{}",
            text(&narrow)
        );
        assert!(words(&narrow).contains("听歌的人"));

        // A portrait has nobody to belong to when nobody is signed in.
        let anonymous = frame(WIDE, None);
        assert!(painted(&anonymous).is_empty(), "nobody is signed in");
        assert!(words(&anonymous).contains("未登录"));

        // And logging out forgets it, so the next frame has nothing to draw either.
        avatar::clear();
        let after = frame(WIDE, Some(&user));
        assert!(
            painted(&after).is_empty(),
            "the portrait outlived the session:\n{}",
            text(&after)
        );
    }
}

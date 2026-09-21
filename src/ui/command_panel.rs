use std::borrow::Cow;

use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    prelude::Widget,
    style::{Modifier, Style},
    widgets::{Clear, Paragraph},
};

use super::{BlockStyle, block::CornerBlock};
use crate::{app::App, state::CommandItem};

pub(super) fn draw(f: &mut Frame, app: &App, area: Rect) {
    let panel = &app.state.command_panel;
    let colors = app.current_theme();
    let Some(items) = panel.current_items() else {
        return;
    };

    let title = panel.current_title();
    let inner_height = items.len() as u16 + 2;
    let inner_width = 56u16;

    let popup_area = area.centered(
        Constraint::Length(inner_width),
        Constraint::Length(inner_height),
    );

    let style = BlockStyle {
        colors,
        // A popup paints its own surface, so what is behind it never shows: the base is the
        // surface it is drawn on, not the theme's background.
        base: colors.surface,
        border: &app.state.border,
        tick: app.state.tick,
    };
    let block = CornerBlock::from_color(&style, colors.surface).title(title, colors);
    let inner = block.inner(popup_area);

    f.render_widget(Clear, popup_area);
    block.render(popup_area, f.buffer_mut());

    for (i, item) in items.iter().enumerate() {
        if i >= inner.height as usize {
            break;
        }
        let line_area = Rect {
            y: inner.y + i as u16,
            height: 1,
            ..inner
        };

        // The name is what `:` would take, the key is what does the same thing without it —
        // the palette is a third door onto one command table, not a list of its own.
        let display: Cow<'_, str> = match item {
            CommandItem::Action {
                name, summary, key, ..
            } => {
                let key = key
                    .map(|k| format!("{k:<3}"))
                    .unwrap_or_else(|| "   ".into());
                Cow::Owned(format!("{name:<14}{key}{summary}"))
            }
            CommandItem::SubMenu { name, .. } => {
                Cow::Owned(format!("{name:<14}   {}", crate::config::symbols().submenu))
            }
        };

        let prefix = if i == panel.selected {
            // Owned: the cursor is a glyph from the symbol table, not a literal.
            format!("{} ", crate::config::symbols().selected)
        } else {
            "  ".to_string()
        };
        let style = if i == panel.selected {
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(colors.text)
        };

        f.render_widget(
            Paragraph::new(format!("{}{}", prefix, display)).style(style),
            line_area,
        );
    }
}

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use crate::{
    app::App,
    config::Theme,
    state::notices::Level,
};

pub(super) fn draw_toast(f: &mut Frame, app: &App, colors: &Theme) {
    // The newest notice, while its level's time lasts: an error is worth reading for longer than a
    // confirmation, and both are still in `:messages` once the toast has gone.
    let now = std::time::Instant::now();
    let Some(notice) = app.state.notices.latest(now) else {
        return;
    };

    let area = f.area();
    let display_w = unicode_width::UnicodeWidthStr::width(notice.text.as_str());
    let w = (display_w as u16 + 6).min(area.width);
    let h = 3u16;
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + area.height.saturating_sub(10);

    let toast_area = Rect {
        x,
        y,
        width: w,
        height: h,
    };

    f.render_widget(Clear, toast_area);

    let (border, text) = match notice.level {
        Level::Info => (colors.muted, colors.text),
        Level::Warn => (colors.accent, colors.text),
        Level::Error => (colors.error, colors.error),
    };
    let block = Block::default()
        .borders(Borders::TOP)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(colors.surface));

    let p = Paragraph::new(format!(" {} ", notice.text))
        .style(Style::default().fg(text))
        .block(block)
        .alignment(Alignment::Center);
    f.render_widget(p, toast_area);
}

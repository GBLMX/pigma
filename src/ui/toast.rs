use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use crate::{app::App, config::Theme, state::notices::Level};

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

    // A theme colours the levels, so a palette can make a warning loud and a confirmation quiet
    // without the code that raises them knowing anything about it.
    let looks = colors.looks();
    let text_look = match notice.level {
        Level::Info => looks.notice_info,
        Level::Warn => looks.notice_warn,
        Level::Error => looks.notice_error,
    };
    let text = text_look.fg.unwrap_or(colors.text);
    let block = Block::default()
        .borders(Borders::TOP)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(text))
        .style(Style::default().bg(colors.surface));

    let p = Paragraph::new(format!(" {} ", notice.text))
        .style(text_look.style())
        .block(block)
        .alignment(Alignment::Center);
    f.render_widget(p, toast_area);
}

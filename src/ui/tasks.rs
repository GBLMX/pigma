//! The `:tasks` popup: the work behind the navigation, in flight and finished.
//!
//! The same window as `:messages` — same keys, same scroll limit — because it is the same kind of
//! list. What it adds is what the app is *doing*: a load that arrived, a load that failed, and the
//! one still running, which is the thing a page that shows nothing cannot tell you (Yazi's task
//! layer, on the screen it puts it).

use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Style,
    widgets::{Clear, Paragraph, Widget},
};

use super::{BlockStyle, block::CornerBlock};
use crate::{app::App, state::tasks::TaskState};

const POPUP_WIDTH: u16 = 72;
const POPUP_HEIGHT: u16 = 16;

/// Draw the task list and hand back the scroll limit, as the other popups do.
pub(super) fn draw(f: &mut Frame, app: &App, area: Rect) -> usize {
    let colors = app.current_theme();
    let scroll = app.state.tasks_popup.scroll;

    let popup_area = area.centered(
        Constraint::Length(POPUP_WIDTH),
        Constraint::Length(POPUP_HEIGHT),
    );

    let style = BlockStyle {
        colors,
        base: colors.surface,
        border: &app.state.border,
        tick: app.state.tick,
    };
    let title = format!(
        "{} TASKS {}",
        crate::config::symbols().title_open,
        crate::config::symbols().title_close
    );
    let block = CornerBlock::from_color(&style, colors.surface)
        .title_styled(&title, colors, colors.looks().popup_title)
        .border_color(colors.looks().popup_border.fg.unwrap_or(colors.border));
    let inner = block.inner(popup_area);

    f.render_widget(Clear, popup_area);
    block.render(popup_area, f.buffer_mut());

    let running = app.state.tasks.running();
    let footer = format!(
        "{:>width$}",
        format!("{running} 进行中 · Esc 关闭 · ↑↓ 滚动 · x 清除已完成"),
        width = (POPUP_WIDTH - 4) as usize
    );
    let footer_area = Rect {
        y: inner.y + inner.height.saturating_sub(1),
        height: 1,
        ..inner
    };
    f.render_widget(
        Paragraph::new(footer).style(colors.looks().popup_footer.style()),
        footer_area,
    );

    if app.state.tasks.is_empty() {
        let line_area = Rect {
            y: inner.y,
            height: 1,
            ..inner
        };
        f.render_widget(
            Paragraph::new("  还没有任务").style(Style::default().fg(colors.muted)),
            line_area,
        );

        return 0;
    }

    // Newest first, like the messages: what just happened is what the reader came for.
    let lines: Vec<(TaskState, String)> = app
        .state
        .tasks
        .all()
        .rev()
        .map(|task| (task.state, task.label.clone()))
        .collect();

    let visible = (inner.height.saturating_sub(1)) as usize;
    let max_scroll = lines.len().saturating_sub(visible);
    for (i, (state, label)) in lines.iter().enumerate().skip(scroll).take(visible) {
        let line_area = Rect {
            y: inner.y + (i - scroll) as u16,
            height: 1,
            ..inner
        };
        let marks = crate::config::symbols();
        let (mark, color) = match state {
            TaskState::Running => (marks.task_running.as_str(), colors.accent),
            TaskState::Done => (marks.task_done.as_str(), colors.muted),
            TaskState::Failed => (marks.task_failed.as_str(), colors.error),
        };
        f.render_widget(
            Paragraph::new(format!("  {mark} {label}")).style(Style::default().fg(color)),
            line_area,
        );
    }

    max_scroll
}

use std::sync::LazyLock;

use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    prelude::Widget,
    style::Style,
    widgets::{Clear, Paragraph},
};

use super::{BlockStyle, block::CornerBlock};
use crate::{app::App, state::Page};

/// The key map the popup draws, built once: the fixed rows, with each page's row taken from
/// the page table and spliced in where that key is documented. A new page's row still has to
/// be placed here — `every_page_key_is_documented` is what says so when it is not.
static HELP_ITEMS: LazyLock<Vec<HelpRow>> = LazyLock::new(|| {
    let mut items = vec![
        row("Ctrl+C/q", "退出程序"),
        row(":", "命令模式（Tab 补全）"),
        row("Ctrl+P", "命令面板"),
    ];
    items.extend(page_row(Page::Login));
    items.extend([
        row("Tab / ⇧Tab", "登录页：切换输入框"),
        row("← / →", "登录页：切换登录方式"),
        row("Enter", "登录页：生成二维码 / 提交 / 发送验证码"),
        row("Esc", "登录页：返回主页"),
    ]);
    items.extend([
        row("w", "清空播放队列"),
        row("?", "帮助"),
        row("Esc", "返回"),
        row("Tab / ⇧Tab", "切换导航区块 / 搜索引擎"),
        row("↑ / ↓ 或 k / j", "上 / 下选择"),
        row("g / G", "跳转顶部 / 底部"),
        row("Enter", "播放选中 / 进入"),
        row("Space", "播放 / 暂停"),
        row("n / p", "下一首 / 上一首"),
        row("← / →", "上一列 / 快退，下一列 / 快进"),
        row("+ / -", "音量增大 / 减小"),
        row("z", "切换导航栏位置"),
        row("m", "循环模式"),
    ]);
    items.extend(page_row(Page::Lyrics));
    items.extend(page_row(Page::Playlist));
    items.extend([
        row("/", "搜索 / 过滤"),
        row("s", "喜欢选中歌曲"),
        row("S", "喜欢当前播放歌曲"),
        row("a", "添加到队列下一首播放"),
        row("d", "不感兴趣（每日推荐）/ 取消喜欢选中"),
        row("D", "取消喜欢当前播放歌曲"),
        row("c", "行 / 单元格模式"),
        row("b", "切换边框模式"),
        row("u", "上传缓存歌曲"),
        row("r", "手动刷新列表内容"),
        row("v", "频谱开关"),
        row("V", "音高读数开关"),
        row("y", "歌词译文开关"),
    ]);
    items
});

/// One row of the key map: the key, and what it does.
type HelpRow = (String, &'static str);

/// A row for a fixed key, one that is not a page's.
fn row(key: &'static str, desc: &'static str) -> HelpRow {
    (key.to_string(), desc)
}

/// The row a page contributes, straight from its table entry: the key it names, and the page.
fn page_row(page: Page) -> Option<HelpRow> {
    let spec = page.spec();
    Some((spec.key?.to_string(), spec.name))
}

const POPUP_WIDTH: u16 = 64;
const POPUP_HEIGHT: u16 = 24;
const KEY_COL_WIDTH: usize = 16;

/// Renders the popup and returns the scroll limit implied by the rendered
/// geometry, for the caller to persist into [`crate::state::HelpState`].
pub(super) fn draw(f: &mut Frame, app: &App, area: Rect) -> usize {
    let help = &app.state.help;
    let colors = app.current_theme();

    let popup_area = area.centered(
        Constraint::Length(POPUP_WIDTH),
        Constraint::Length(POPUP_HEIGHT),
    );

    let style = BlockStyle {
        colors,
        // A popup paints its own surface, so what is behind it never shows: the base is the
        // surface it is drawn on, not the theme's background.
        base: colors.surface,
        border: &app.state.border,
        tick: app.state.tick,
    };
    let block =
        CornerBlock::from_color(&style, colors.surface).title("\u{25BA} HELP \u{25C4}", colors);
    let inner = block.inner(popup_area);

    f.render_widget(Clear, popup_area);
    block.render(popup_area, f.buffer_mut());

    let footer = format!(
        "{:>width$}",
        "Esc 关闭 · : 命令行 · Ctrl+P 面板",
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

    let visible = (inner.height.saturating_sub(1)) as usize;
    let max_scroll = HELP_ITEMS.len().saturating_sub(visible);
    let scroll = help.scroll.min(max_scroll);
    for (i, (key, desc)) in HELP_ITEMS.iter().enumerate().skip(scroll).take(visible) {
        let line_area = Rect {
            y: inner.y + (i - scroll) as u16,
            height: 1,
            ..inner
        };
        let line = format!("  {:<width$}  {}", key, desc, width = KEY_COL_WIDTH);
        let style = if *key == "?" {
            Style::default().fg(colors.accent)
        } else {
            Style::default().fg(colors.text)
        };
        f.render_widget(Paragraph::new(line).style(style), line_area);
    }
    max_scroll
}

#[cfg(test)]
mod tests {
    use super::HELP_ITEMS;
    use crate::state::Page;

    /// Every page that owns a key is documented here. The rows come from the table, but the
    /// row of a new page still has to be spliced into the list where it is documented — this
    /// is what says so when it is not.
    #[test]
    fn every_page_key_is_documented() {
        for page in Page::ALL {
            let spec = page.spec();
            let Some(key) = spec.key else {
                continue;
            };
            assert!(
                HELP_ITEMS.contains(&(key.to_string(), spec.name)),
                "{} ({key}) is missing from the key map",
                spec.name
            );
        }
    }
}

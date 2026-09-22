//! 应用自己的按键词汇。
//!
//! crossterm 的按键事件只是把终端字节翻译成「按了什么」，而按了什么是应用自己才知道的事。
//! 所以那次翻译在这里发生一次，此后的 `input`、`state`、`ui` 只认 [`KeyPress`]——它们描述的是
//! 应用的行为，不该让终端库的类型出现在自己的签名里。翻译只在一个地方做：事件的边界
//! （`app::event`），那里 crossterm 的事件刚刚到达。
//!
//! [`KeyCode`] 只列出应用真的绑定了的键，外加一个 [`KeyCode::Other`]：终端能发出的键远不止
//! 这些，但让每个处理器都去辨认（再忽略）自己从未绑定过的键，就是把「不关心」抄成十几个
//! `_ =>` 分支之外还得提防新键落进某个现有的臂里。落到 `Other` 上，处理器保持穷尽。
//!
//! 修饰键只留 `ctrl` / `alt` / `shift` 三个布尔值，因为这是应用问得出来的全部问题。crossterm
//! 还区分 Super / Hyper / Meta，这里不再区分：没有任何绑定用到它们，丢弃它们换来的只是更简单
//! 的模型；唯一的代价见 [`Modifiers::is_ctrl_only`]。

use crossterm::event::{KeyCode as CtKeyCode, KeyEvent, KeyModifiers as CtKeyModifiers};

/// 一次按键：终端发来的一次「键 + 修饰键」，用应用自己的说法表示。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyPress {
    pub code: KeyCode,
    pub mods: Modifiers,
}

impl KeyPress {
    /// 按一次键的构造：测试要「按哪个键」时用它。
    pub const fn new(code: KeyCode, mods: Modifiers) -> Self {
        Self { code, mods }
    }
}

/// 应用会响应的键，加上「其余」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    /// 一个字符。Shift 的结果已经算在里面（`A` 就是 `A`），所以绑定写的是字符本身。
    Char(char),
    Enter,
    Esc,
    Tab,
    BackTab,
    Up,
    Down,
    Left,
    Right,
    Backspace,
    /// 应用没有绑定的键。终端能发出的键比上面列的多，它们统统落在这里，而不是让每个处理器
    /// 自己分辨一个它从不需要的键。
    Other,
}

/// 按键时按住的修饰键。
///
/// 只是三个布尔值而不是 crossterm 的位标志：绑定只问得出「按着 Ctrl 吗」「按着 Alt 吗」以及
/// 「Ctrl 且只有 Ctrl 吗」这三个问题，多出来的位没有答案可给。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Modifiers {
    /// 什么都没按。
    pub const NONE: Self = Self {
        ctrl: false,
        alt: false,
        shift: false,
    };

    /// 「Ctrl 且只有 Ctrl」——旧代码里 `modifiers == KeyModifiers::CONTROL` 的意思：Ctrl+C、
    /// Ctrl+P 只在没有同时按着别的修饰键时才该触发。
    ///
    /// 这里比 crossterm 少了 Super / Hyper / Meta：它们没有被任何绑定用到，所以 Ctrl+Super+X
    /// 在这里也被算作「只有 Ctrl」，而 crossterm 的相等判断会把它排除在外。这是本模块唯一一处
    /// 与 crossterm 语义不同的地方。
    pub const fn is_ctrl_only(self) -> bool {
        self.ctrl && !self.alt && !self.shift
    }
}

/// 边界上的翻译：全应用唯一读 crossterm 按键的地方。
///
/// 变体一一对应；没有列出来的键落到 [`KeyCode::Other`]（理由见 [`KeyCode`]），修饰键按是否包含取值。
impl From<KeyEvent> for KeyPress {
    fn from(event: KeyEvent) -> Self {
        let code = match event.code {
            CtKeyCode::Char(ch) => KeyCode::Char(ch),
            CtKeyCode::Enter => KeyCode::Enter,
            CtKeyCode::Esc => KeyCode::Esc,
            CtKeyCode::Tab => KeyCode::Tab,
            CtKeyCode::BackTab => KeyCode::BackTab,
            CtKeyCode::Up => KeyCode::Up,
            CtKeyCode::Down => KeyCode::Down,
            CtKeyCode::Left => KeyCode::Left,
            CtKeyCode::Right => KeyCode::Right,
            CtKeyCode::Backspace => KeyCode::Backspace,
            _ => KeyCode::Other,
        };

        let mods = event.modifiers;
        Self {
            code,
            mods: Modifiers {
                ctrl: mods.contains(CtKeyModifiers::CONTROL),
                alt: mods.contains(CtKeyModifiers::ALT),
                shift: mods.contains(CtKeyModifiers::SHIFT),
            },
        }
    }
}

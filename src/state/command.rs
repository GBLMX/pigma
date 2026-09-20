/// One thing the user can ask for, described once and reached three ways: by key, from the `:`
/// command line, and from the palette.
///
/// These used to be three separate lists that did not agree — the palette had four English
/// actions, the key map twenty-seven keys and the `:` line seventeen verbs. Switching the theme
/// existed in all three under three different names, save-on-play only in the palette, lyrics
/// and the player-bar layout only in `:`. A command is added here once and appears everywhere.
pub struct Command {
    /// The name it answers to on the `:` line, and what the palette shows.
    pub name: &'static str,
    /// One line, for the palette.
    pub summary: &'static str,
    /// A key that does the same thing, when one exists. Shown, never parsed.
    pub key: Option<&'static str>,
    /// What `:` would run — the command line's whole vocabulary, one text per entry.
    pub ex: &'static str,
    /// Whether it needs an argument. The palette lists these, but Enter opens the command line
    /// with the name filled in rather than running it and reporting a missing argument.
    pub needs_argument: bool,
    /// Whether the palette lists it at all. Aliases such as `q` are for the command line.
    pub in_palette: bool,
}

/// Shorthand for the common case: runs its own name, no argument, listed in the palette.
const fn simple(name: &'static str, summary: &'static str, key: Option<&'static str>) -> Command {
    Command {
        name,
        summary,
        key,
        ex: name,
        needs_argument: false,
        in_palette: true,
    }
}

/// Every command the UI offers, in the order the palette lists them.
///
/// `theme` is the entry the palette renders as a submenu (one item per known theme), built from
/// the theme registry rather than from static text, which is why it is the last one here.
pub const COMMANDS: &[Command] = &[
    simple("help", "快捷键面板", Some("?")),
    simple("login", "登录页（二维码）", Some("L")),
    simple("logout", "退出登录", None),
    simple("sign", "网易云每日签到", None),
    simple("visualizer", "频谱开关", Some("v")),
    simple("pitch", "音高读数开关", Some("V")),
    simple("lyrics", "歌词显示样式", None),
    simple("layout", "播放条布局", None),
    simple("border", "边框模式开关", Some("b")),
    simple("navpos", "切换导航栏位置", Some("z")),
    simple("saveonplay", "边听边存开关", None),
    simple("save", "立即写回配置", None),
    simple("quit", "退出程序", Some("q")),
    // Aliases the command line accepts; the palette shows the canonical name.
    Command {
        name: "q",
        summary: "退出程序（:quit 的别名）",
        key: None,
        ex: "q",
        needs_argument: false,
        in_palette: false,
    },
    // These need a value, so the palette opens the command line with the name filled in.
    Command {
        name: "theme",
        summary: "切换主题",
        key: None,
        ex: "theme",
        needs_argument: true,
        in_palette: true,
    },
    Command {
        name: "volume",
        summary: "音量（0-100 或 +5 / -5）",
        key: None,
        ex: "volume",
        needs_argument: true,
        in_palette: true,
    },
    Command {
        name: "seek",
        summary: "跳转（秒、+15、-30 或 50%）",
        key: None,
        ex: "seek",
        needs_argument: true,
        in_palette: true,
    },
    Command {
        name: "signin",
        summary: "账号密码登录",
        key: None,
        ex: "signin",
        needs_argument: true,
        in_palette: true,
    },
    Command {
        name: "sms",
        summary: "发送短信验证码",
        key: None,
        ex: "sms",
        needs_argument: true,
        in_palette: true,
    },
    Command {
        name: "smslogin",
        summary: "短信验证码登录",
        key: None,
        ex: "smslogin",
        needs_argument: true,
        in_palette: true,
    },
];

/// Every `:` name, in table order — what completion offers.
pub fn command_names() -> Vec<&'static str> {
    COMMANDS.iter().map(|command| command.name).collect()
}

/// What the palette's Enter asks the app to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelAction {
    /// Run this command line.
    Run(String),
    /// Open the command line with this text, cursor at the end.
    Prefill(String),
}

#[derive(Debug, Clone)]
pub enum CommandItem {
    Action {
        name: String,
        summary: &'static str,
        key: Option<&'static str>,
        /// The `:` line this runs, or prefills when `needs_argument`.
        ex: String,
        needs_argument: bool,
    },
    SubMenu {
        name: String,
        children: Vec<CommandItem>,
    },
}

pub struct CommandPanel {
    pub open: bool,
    pub selected: usize,
    pub levels: Vec<Vec<CommandItem>>,
}

impl Default for CommandPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandPanel {
    pub fn new() -> Self {
        Self {
            open: false,
            selected: 0,
            levels: Vec::new(),
        }
    }

    pub fn current_items(&self) -> Option<&Vec<CommandItem>> {
        self.levels.last()
    }

    pub fn current_title(&self) -> &str {
        if self.levels.len() > 1 {
            "\u{25BA} THEMES \u{25C4}"
        } else {
            "\u{25BA} COMMANDS \u{25C4}"
        }
    }

    /// What Enter on an item should do.
    pub fn enter(&mut self) -> Option<PanelAction> {
        let item = &self.current_items()?[self.selected];
        match item {
            CommandItem::Action {
                ex, needs_argument, ..
            } => Some(if *needs_argument {
                // The palette does not guess arguments: it opens the command line ready for one.
                PanelAction::Prefill(format!("{ex} "))
            } else {
                PanelAction::Run(ex.clone())
            }),
            CommandItem::SubMenu { children, .. } => {
                let children = children.clone();
                self.selected = 0;
                self.levels.push(children);
                None
            }
        }
    }

    pub fn back(&mut self) {
        if self.levels.len() > 1 {
            self.levels.pop();
            self.selected = 0;
        } else {
            self.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every entry the palette can run has to survive the command line, or it would offer
    /// something `:` cannot do.
    ///
    /// Entries that need an argument are the ones the palette prefills instead of running, and
    /// their argument handling is covered by the `:` parser's own tests.
    #[test]
    fn every_runnable_entry_parses_as_a_command_line() {
        for command in COMMANDS.iter().filter(|c| !c.needs_argument) {
            crate::input::ex::ExCommand::parse(command.ex)
                .unwrap_or_else(|error| panic!("{}: {error}", command.ex));
        }
    }

    /// Names are what `:` completion offers, so they have to be unique — and an entry that runs
    /// something else than its own name would make the palette lie.
    #[test]
    fn names_are_unique_and_run_themselves() {
        let mut names = command_names();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "two commands share a name");

        for command in COMMANDS {
            assert!(
                command.ex == command.name || command.ex.starts_with(&format!("{} ", command.name)),
                "{} runs {:?}",
                command.name,
                command.ex
            );
        }
    }

    /// The keys the table advertises are the ones the help panel lists, so a key rename cannot
    /// leave a stale hint behind.
    #[test]
    fn advertised_keys_are_free_of_colons() {
        for command in COMMANDS {
            if let Some(key) = command.key {
                assert!(!key.contains(':'), "{}: {key}", command.name);
            }
        }
    }
}

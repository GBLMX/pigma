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
    /// What `:` would run — the command line's whole vocabulary, one text per entry. It is
    /// also the palette's label, which is why it is a whole line rather than the verb: two
    /// entries can be two forms of one verb (`notify song_change`, `notify errors`), and the
    /// palette has to say which form an entry runs.
    pub ex: &'static str,
    /// Whether it needs an argument. The palette lists these, but Enter opens the command line
    /// with the name filled in rather than running it and reporting a missing argument.
    pub needs_argument: bool,
    /// Whether the palette lists it at all. Aliases such as `q` are for the command line.
    pub in_palette: bool,
    /// The palette section this entry sits in, `None` for the root list. Sections are the
    /// config's own blocks (`[notify]`, `[cache]`), so a setting is filed by naming the block
    /// it belongs to here; the palette builds one submenu per section and knows nothing else
    /// about them.
    pub group: Option<&'static str>,
}

/// Shorthand for the common case: runs its own name, no argument, listed in the palette, no
/// section of its own.
const fn simple(name: &'static str, summary: &'static str, key: Option<&'static str>) -> Command {
    Command {
        name,
        summary,
        key,
        ex: name,
        needs_argument: false,
        in_palette: true,
        group: None,
    }
}

/// Shorthand for a setting: listed in the palette inside `group`, and bare, it toggles or
/// cycles to the next value — which is why Enter can run it instead of prefilling the line.
///
/// `ex` is the command line it runs, its own name unless the entry is one form of a verb that
/// answers to several words.
const fn setting(
    name: &'static str,
    ex: &'static str,
    summary: &'static str,
    group: &'static str,
) -> Command {
    Command {
        name,
        summary,
        key: None,
        ex,
        needs_argument: false,
        in_palette: true,
        group: Some(group),
    }
}

/// Every command the UI offers, in the order the palette lists them.
///
/// `theme` is the entry the palette renders as a submenu (one item per known theme), built from
/// the theme registry rather than from static text, which is why it is the last one here. Entries
/// carrying a `group` are the settings, listed in the order their section first appears here.
pub const COMMANDS: &[Command] = &[
    simple("help", "快捷键面板", Some("?")),
    simple("login", "登录页（二维码 / 账号 / 短信 / 签到）", Some("L")),
    simple("logout", "退出登录", None),
    simple("sign", "在登录页里每日签到", None),
    simple("visualizer", "频谱开关", Some("v")),
    simple("pitch", "音高读数开关", Some("V")),
    simple("layout", "播放条布局", None),
    simple("border", "边框模式开关", Some("b")),
    simple("navpos", "切换导航栏位置", Some("z")),
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
        group: None,
    },
    // These need a value, so the palette opens the command line with the name filled in.
    Command {
        name: "theme",
        summary: "切换主题",
        key: None,
        ex: "theme",
        needs_argument: true,
        in_palette: true,
        group: None,
    },
    Command {
        name: "volume",
        summary: "音量（0-100 或 +5 / -5）",
        key: None,
        ex: "volume",
        needs_argument: true,
        in_palette: true,
        group: None,
    },
    Command {
        name: "seek",
        summary: "跳转（秒、+15、-30 或 50%）",
        key: None,
        ex: "seek",
        needs_argument: true,
        in_palette: true,
        group: None,
    },
    Command {
        name: "signin",
        summary: "在登录页里用账号密码登录",
        key: None,
        ex: "signin",
        needs_argument: true,
        in_palette: true,
        group: None,
    },
    Command {
        name: "sms",
        summary: "在登录页里发送短信验证码",
        key: None,
        ex: "sms",
        needs_argument: true,
        in_palette: true,
        group: None,
    },
    Command {
        name: "smslogin",
        summary: "在登录页里用短信验证码登录",
        key: None,
        ex: "smslogin",
        needs_argument: true,
        in_palette: true,
        group: None,
    },
    // The settings a user flips mid-session. Each one is read from the config at the moment it
    // is used — the notify switches when an event fires, the lyrics ones on every frame, the
    // terminal ones by the sequence its command writes — so the switch is live rather than
    // waiting for the next start. They are filed by the config block they come from, which is
    // what puts them in a section of the palette instead of the root list.
    setting("notify", "notify song_change", "切歌时通知当前曲目", "通知"),
    setting("notify", "notify errors", "播放出错时通知", "通知"),
    setting("mouse", "mouse", "鼠标捕获开关", "终端"),
    setting("cursor", "cursor", "输入框光标形状", "终端"),
    setting("lyrics", "lyrics", "歌词显示样式", "歌词"),
    setting("lyricgradient", "lyricgradient", "歌词渐变预设", "歌词"),
    setting("saveonplay", "saveonplay", "边听边存开关", "缓存"),
];

/// Every `:` name, in table order, once each — what completion offers.
///
/// A verb that answers to several entries (`notify song_change`, `notify errors`) is named
/// once: the line types the verb once.
pub fn command_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = Vec::new();
    for name in COMMANDS.iter().map(|command| command.name) {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// The palette's entries, every one of them from [`COMMANDS`]: the plain commands, then one
/// submenu per section.
///
/// The theme submenu is not here — its children are the themes this build can load, which only
/// the registry knows — so the caller puts it in front of these.
pub fn palette_items() -> Vec<CommandItem> {
    /// The entries of one section, `None` for the root list.
    fn section(group: Option<&str>) -> Vec<CommandItem> {
        COMMANDS
            .iter()
            // `theme` is the registry's submenu; its own entry would list it a second time.
            .filter(|command| {
                command.in_palette && command.name != "theme" && command.group == group
            })
            .map(|command| CommandItem::Action {
                name: command.ex.to_string(),
                summary: command.summary,
                key: command.key,
                ex: command.ex.to_string(),
                needs_argument: command.needs_argument,
            })
            .collect()
    }

    let mut items = section(None);

    // The sections, in the order their first entry appears in the table.
    let mut sections: Vec<&'static str> = Vec::new();
    for group in COMMANDS.iter().filter_map(|command| command.group) {
        if !sections.contains(&group) {
            sections.push(group);
        }
    }
    items.extend(sections.into_iter().map(|group| CommandItem::SubMenu {
        name: group.to_string(),
        children: section(Some(group)),
    }));

    items
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

/// One open level of the palette: the items on screen, and the title the popup's border shows
/// for them.
///
/// The title is the section's own name, so entering a submenu says where you are without a
/// second list of titles to keep in step with the submenus.
struct PanelLevel {
    title: String,
    items: Vec<CommandItem>,
}

impl PanelLevel {
    fn new(title: &str, items: Vec<CommandItem>) -> Self {
        Self {
            title: format!("\u{25BA} {title} \u{25C4}"),
            items,
        }
    }
}

pub struct CommandPanel {
    pub open: bool,
    pub selected: usize,
    /// One entry per open level, outermost first; always at least one.
    levels: Vec<PanelLevel>,
}

impl Default for CommandPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandPanel {
    /// An empty panel; the app builder fills it with [`Self::with_root`].
    pub fn new() -> Self {
        Self {
            open: false,
            selected: 0,
            levels: Vec::new(),
        }
    }

    /// A panel whose root lists `items` under `title`.
    pub fn with_root(title: &str, items: Vec<CommandItem>) -> Self {
        Self {
            open: false,
            selected: 0,
            levels: vec![PanelLevel::new(title, items)],
        }
    }

    pub fn current_items(&self) -> Option<&Vec<CommandItem>> {
        self.levels.last().map(|level| &level.items)
    }

    pub fn current_title(&self) -> &str {
        self.levels
            .last()
            .map(|level| level.title.as_str())
            .unwrap_or_default()
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
            CommandItem::SubMenu { name, children } => {
                let level = PanelLevel::new(name, children.clone());
                self.selected = 0;
                self.levels.push(level);
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

    /// A setting has to reach the palette exactly once, in the section its config block names —
    /// that is what makes the submenus a view of the table rather than a second list.
    #[test]
    fn every_palette_entry_lands_in_the_section_it_names() {
        let items = palette_items();

        let mut listed: Vec<&str> = Vec::new();
        let mut sections: Vec<(&str, Vec<&str>)> = Vec::new();
        for item in &items {
            match item {
                CommandItem::Action { name, .. } => listed.push(name),
                CommandItem::SubMenu { name, children } => sections.push((
                    name,
                    children
                        .iter()
                        .filter_map(|child| match child {
                            CommandItem::Action { name, .. } => Some(name.as_str()),
                            CommandItem::SubMenu { .. } => None,
                        })
                        .collect(),
                )),
            }
        }

        // `theme` is the registry's submenu, built by the app rather than from the table.
        for command in COMMANDS
            .iter()
            .filter(|command| command.in_palette && command.name != "theme")
        {
            let root = listed.iter().filter(|name| **name == command.ex).count();
            let filed: Vec<&str> = sections
                .iter()
                .filter(|(_, children)| children.contains(&command.ex))
                .map(|(name, _)| *name)
                .collect();

            match command.group {
                None => {
                    assert_eq!(root, 1, "{} is not in the root list once", command.ex);
                    assert!(
                        filed.is_empty(),
                        "{} names no section but is filed under {filed:?}",
                        command.ex
                    );
                }
                Some(group) => {
                    assert_eq!(
                        root, 0,
                        "{} belongs in {group}, not the root list",
                        command.ex
                    );
                    assert_eq!(
                        filed,
                        vec![group],
                        "{} should be under {group} once",
                        command.ex
                    );
                }
            }
        }
    }

    /// The section titles come from the submenus, so a new section needs no title of its own —
    /// and entering one says which section it is.
    #[test]
    fn a_section_title_is_the_section_name() {
        let mut panel = CommandPanel::with_root("COMMANDS", palette_items());
        assert_eq!(panel.current_title(), "\u{25BA} COMMANDS \u{25C4}");

        let index = panel
            .current_items()
            .expect("root")
            .iter()
            .position(|item| matches!(item, CommandItem::SubMenu { name, .. } if name == "通知"))
            .expect("the notify section");
        panel.selected = index;
        assert_eq!(panel.enter(), None, "a submenu opens, it does not run");
        assert_eq!(panel.current_title(), "\u{25BA} 通知 \u{25C4}");

        panel.back();
        assert_eq!(panel.current_title(), "\u{25BA} COMMANDS \u{25C4}");
    }
}

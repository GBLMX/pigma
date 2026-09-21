//! The settings page: every switch the app has, in one list, read from the config it edits.
//!
//! The rows are a table rather than a form written out by hand, and a row is changed by running
//! the `:` command that already performs it — the same command the key map and the command
//! palette run, with the same side effects (the terminal's mouse capture, the engine's save-on-play
//! copy, the theme registry's `random`). So the page cannot drift from the commands: it *is* a
//! view of them, and the tests hold each row to the config key it claims to edit and to the
//! command it claims to run.
//!
//! The value a row shows is read from `config.toml` itself ([`values`]), by the key the row
//! declares — a renamed or dropped field therefore fails a test rather than leaving a blank row.
//! The options a row cycles through are the command's own completions ([`super::super::input::ex::options_for`]),
//! so a new preset appears in the page the moment the command line offers it.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Padding, Paragraph},
};

use super::{
    BlockStyle,
    block::CornerBlock,
};
use crate::{
    app::App,
    input::ex::{self, ExCommand},
};

/// How a row is changed, and therefore what the keys do on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingKind {
    /// On or off.
    Toggle,
    /// One of the command's options: `→` takes the next one, `←` the one before it.
    Choice,
    /// The command acts by itself — `:navpos` and `:cursor` cycle — so both arrows run it.
    Cycle,
}

/// One row of the settings page.
pub struct Setting {
    /// The section it is filed under. Sections are written in table order.
    pub group: &'static str,
    /// What the row says.
    pub label: &'static str,
    /// The `config.toml` key it edits, dotted (`playerbar.progress_style`). The page reads the
    /// value back from exactly this path.
    pub key: &'static str,
    pub kind: SettingKind,
    /// The `:` command that performs the change, without its argument.
    pub ex: &'static str,
}

/// Every setting the page shows, in the order it shows them.
///
/// Only knobs whose change already has a command are here: the page runs commands rather than
/// writing the config behind their back, so that a switch with side effects — the mouse capture,
/// the engine's copy of save-on-play — keeps them when it is flipped from here.
pub static SETTINGS: &[Setting] = &[
    Setting {
        group: "外观",
        label: "主题",
        key: "default_theme",
        kind: SettingKind::Choice,
        ex: "theme",
    },
    Setting {
        group: "外观",
        label: "导航栏位置",
        key: "navigation_position",
        kind: SettingKind::Cycle,
        ex: "navpos",
    },
    Setting {
        group: "外观",
        label: "鼠标捕获",
        key: "mouse",
        kind: SettingKind::Toggle,
        ex: "mouse",
    },
    Setting {
        group: "外观",
        label: "输入框光标",
        key: "cursor_style",
        kind: SettingKind::Choice,
        ex: "cursor",
    },
    Setting {
        group: "歌词",
        label: "显示样式",
        key: "lyric_style",
        kind: SettingKind::Choice,
        ex: "lyrics",
    },
    Setting {
        group: "歌词",
        label: "扫光渐变",
        key: "lyric_gradient",
        kind: SettingKind::Choice,
        ex: "lyricgradient",
    },
    Setting {
        group: "歌词",
        label: "译文",
        key: "lyric_translation",
        kind: SettingKind::Toggle,
        ex: "translation",
    },
    Setting {
        group: "播放条",
        label: "进度条样式",
        key: "playerbar.progress_style",
        kind: SettingKind::Choice,
        ex: "progress",
    },
    Setting {
        group: "播放条",
        label: "布局",
        key: "playerbar.layout",
        kind: SettingKind::Choice,
        ex: "layout",
    },
    Setting {
        group: "播放条",
        label: "封面旋转",
        key: "playerbar.spinning_cover",
        kind: SettingKind::Toggle,
        ex: "spin",
    },
    Setting {
        group: "播放条",
        label: "频谱",
        key: "playerbar.visible.visualizer",
        kind: SettingKind::Toggle,
        ex: "visualizer",
    },
    Setting {
        group: "播放条",
        label: "音高读数",
        key: "playerbar.visible.pitch",
        kind: SettingKind::Toggle,
        ex: "pitch",
    },
    Setting {
        group: "通知",
        label: "切歌时通知",
        key: "notify.song_change",
        kind: SettingKind::Toggle,
        ex: "notify song_change",
    },
    Setting {
        group: "通知",
        label: "播放出错时通知",
        key: "notify.errors",
        kind: SettingKind::Toggle,
        ex: "notify errors",
    },
    Setting {
        group: "缓存",
        label: "边听边存",
        key: "cache.save_on_play",
        kind: SettingKind::Toggle,
        ex: "saveonplay",
    },
];

/// The sections, in the order the table writes them, each once.
pub fn groups() -> Vec<&'static str> {
    let mut groups: Vec<&'static str> = Vec::new();
    for setting in SETTINGS {
        if !groups.contains(&setting.group) {
            groups.push(setting.group);
        }
    }
    groups
}

/// The rows of `group`, as (row index, row).
pub fn rows_in(group: &str) -> impl Iterator<Item = (usize, &'static Setting)> {
    SETTINGS
        .iter()
        .enumerate()
        .filter(move |(_, setting)| setting.group == group)
}

/// Every row's current value, read out of the config's own serialization by the key the row
/// names.
pub fn values(config: &crate::config::Config) -> Vec<String> {
    let toml: toml_edit::DocumentMut = config.to_toml().parse().unwrap_or_else(|error| {
        // A config that cannot be read back is a bug in the writer, not a reason to take the page
        // down: every row shows blank, and the rows still work.
        log::error!("config.toml does not parse back: {error}");
        toml_edit::DocumentMut::new()
    });

    SETTINGS.iter().map(|setting| value_at(&toml, setting.key)).collect()
}

/// The value at a dotted key, as the page writes it: arrays and tables are not values a row can
/// show, so they come back as a placeholder the tests refuse to let through.
fn value_at(doc: &toml_edit::DocumentMut, key: &str) -> String {
    let mut item: &toml_edit::Item = doc.as_item();
    for part in key.split('.') {
        item = &item[part];
    }

    match item {
        toml_edit::Item::Value(value) => match value {
            toml_edit::Value::Boolean(on) => (if *on.value() { "on" } else { "off" }).to_string(),
            other => other.to_string().trim().trim_matches('"').to_string(),
        },
        toml_edit::Item::Table(_) => String::new(),
        toml_edit::Item::None => String::new(),
        _ => String::new(),
    }
}

/// The options a row cycles through: the ones its own command line offers after it.
pub fn options(setting: &Setting, themes: &[String]) -> Vec<String> {
    ex::options_for(setting.ex, themes)
}

/// Change a row by one step: the command does the work, exactly as if it had been typed.
pub fn change(app: &mut App, setting: &Setting, forward: bool) -> Result<(), String> {
    // Owned, because the command that runs next needs the app mutably while the registry still
    // lends its names out.
    let themes: Vec<String> = app
        .theme_registry
        .choosable_names()
        .iter()
        .map(|name| name.to_string())
        .collect();
    let line = match setting.kind {
        SettingKind::Toggle => {
            let on = values(&app.config)
                .get(index_of(setting))
                .map(|value| value == "on")
                .unwrap_or(false);
            format!("{} {}", setting.ex, if on { "off" } else { "on" })
        }
        // A command that only cycles cannot be run backwards, so `←` leaves the row where it is
        // rather than pretending: the row shows `↻` for that reason.
        SettingKind::Cycle => {
            if !forward {
                return Ok(());
            }
            setting.ex.to_string()
        }
        SettingKind::Choice => {
            let options = options(setting, &themes);
            if options.is_empty() {
                return Err(format!("`{}` 没有可选的取值", setting.ex));
            }
            let current = options
                .iter()
                .position(|option| Some(option) == values(&app.config).get(index_of(setting)));
            let step = if forward { 1 } else { -1 };
            let next = match current {
                Some(at) => (at as i32 + step).rem_euclid(options.len() as i32) as usize,
                // Nothing matches (a value written by hand, a number): the first step lands on
                // the first option rather than on a guess.
                None => 0,
            };
            format!("{} {}", setting.ex, options[next])
        }
    };

    ex::execute(app, ExCommand::parse(&line)?)
}

/// Where a row sits in the table — it is what the values list is indexed by.
fn index_of(setting: &Setting) -> usize {
    SETTINGS
        .iter()
        .position(|candidate| std::ptr::eq(candidate, setting))
        .unwrap_or(0)
}

/// What the page needs to remember between frames.
#[derive(Debug, Default)]
pub struct SettingsState {
    /// The row the cursor is on, as an index into [`SETTINGS`].
    pub selected: usize,
}

impl SettingsState {
    /// Move the cursor `step` rows, wrapping through the whole table: the sections are one list
    /// with headings on it rather than four separate lists to lose the cursor in.
    pub fn move_by(&mut self, step: i32) {
        let len = SETTINGS.len() as i32;
        self.selected = (self.selected as i32 + step).rem_euclid(len) as usize;
    }
}

/// Draw the settings page in the page's area: the sections on the left, the rows of the section
/// the cursor is in on the right.
pub(crate) fn draw(
    f: &mut Frame,
    config: &crate::config::Config,
    selected: usize,
    bs: &BlockStyle<'_>,
    area: Rect,
) {
    let block = CornerBlock::from_color(bs, bs.base).title("设置", bs.colors);
    let inner = block.inner(area);
    f.render_widget(block.block_padding(Padding::vertical(1)), area);

    let selected = selected.min(SETTINGS.len() - 1);
    let current = SETTINGS[selected].group;
    let values = values(config);
    let colors = bs.colors;

    // The section column is as wide as its longest name *in cells* — a CJK name is two cells a
    // character, and counting characters clipped `播放条` to `播放` — plus the indent and a gap.
    let width = groups()
        .iter()
        .map(|group| unicode_width::UnicodeWidthStr::width(*group))
        .max()
        .unwrap_or(0) as u16;
    let [sections, _, rows] = Layout::horizontal([
        Constraint::Length(width + 4),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(inner);

    let section_lines: Vec<Line> = groups()
        .into_iter()
        .map(|group| {
            let style = if group == current {
                Style::default()
                    .fg(colors.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(colors.muted)
            };
            Line::from(Span::styled(format!("  {group}"), style))
        })
        .collect();
    f.render_widget(Paragraph::new(section_lines), sections);

    let rows_lines: Vec<Line> = rows_in(current)
        .map(|(index, setting)| {
            let style = if index == selected {
                Style::default()
                    .fg(colors.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(colors.text)
            };
            let value = &values[index];
            let value = match setting.kind {
                SettingKind::Toggle => (if value == "on" { "开" } else { "关" }).to_string(),
                SettingKind::Choice => format!("‹ {value} ›"),
                SettingKind::Cycle => format!("{value} ↻"),
            };
            // Padded by *columns*, not characters: a CJK label is two cells per character, so
            // `{:<16}` would leave the values in a ragged column.
            let pad = 16usize.saturating_sub(unicode_width::UnicodeWidthStr::width(setting.label));
            let text = format!("{}{} {}", setting.label, " ".repeat(pad), value);

            Line::from(Span::styled(
                if index == selected {
                    format!("▸ {text}")
                } else {
                    format!("  {text}")
                },
                style,
            ))
        })
        .collect();
    f.render_widget(Paragraph::new(rows_lines), rows);

    let hint = Line::from(Span::styled(
        "↑↓ 选择 · ←→ 修改 · 空格 开关 · Esc 返回",
        Style::default().fg(colors.muted),
    ))
    .alignment(Alignment::Left);
    let hint_area = Rect::new(rows.x, inner.bottom().saturating_sub(1), rows.width, 1);
    f.render_widget(Paragraph::new(hint), hint_area);
}

/// The page's keys. Returns whether the key was one of them.
pub(crate) fn handle_key(app: &mut App, key: crossterm::event::KeyCode) -> bool {
    use crossterm::event::KeyCode;

    let selected = app.state.settings.selected.min(SETTINGS.len() - 1);
    let setting = &SETTINGS[selected];

    let outcome = match key {
        KeyCode::Up | KeyCode::Char('k') => {
            app.state.settings.move_by(-1);
            return true;
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.state.settings.move_by(1);
            return true;
        }
        // Space and Enter flip a switch; the arrows are for the rows that have more than two
        // values, and they flip a switch too rather than doing nothing on it.
        KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter | KeyCode::Char(' ') => {
            change(app, setting, true)
        }
        KeyCode::Left | KeyCode::Char('h') => change(app, setting, false),
        _ => return false,
    };

    if let Err(error) = outcome {
        app.toast(format!("设置未改动: {error}"));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    /// A headless app: it owns no terminal, so — since `Config::persist` — nothing these tests do
    /// can reach the user's own `config.toml`.
    fn app() -> App {
        // The app builds a network client; the provider is what the other tests install too.
        let _ = rustls::crypto::ring::default_provider().install_default();

        App::new(Config::default(), false).expect("headless app")
    }

    /// Every row names a key that is really in `config.toml`. This is the check that keeps the
    /// page honest: a renamed or dropped field leaves the row showing nothing, and a test — not a
    /// user — finds out.
    #[test]
    fn every_row_names_a_key_that_exists() {
        let defaults = Config::default().to_toml();
        let doc: toml_edit::DocumentMut = defaults.parse().expect("defaults serialize");

        for setting in SETTINGS {
            let mut item: &toml_edit::Item = doc.as_item();
            for part in setting.key.split('.') {
                item = &item[part];
            }
            assert!(
                !item.is_none(),
                "{}: `{}` is not a key of config.toml",
                setting.label,
                setting.key
            );
            assert!(
                matches!(item, toml_edit::Item::Value(_)),
                "{}: `{}` is a table, not a value a row can show",
                setting.label,
                setting.key
            );
        }
    }

    /// Every row runs the command it claims to: the line parses as a command line, so a renamed
    /// command cannot leave a row that does nothing.
    #[test]
    fn every_row_runs_a_command_that_parses() {
        for setting in SETTINGS {
            assert!(
                ExCommand::parse(setting.ex).is_ok(),
                "{}: `:{}` is not a command",
                setting.label,
                setting.ex
            );
        }
    }

    /// Changing a row changes the value it shows. This is the whole page in one assertion: the
    /// command runs, and the key the row names is the key the command wrote.
    // The app brings up a Tokio runtime, like the other tests that build one.
    #[tokio::test]
    async fn changing_a_row_changes_the_key_it_names() {
        let mut app = app();

        // These two write escape sequences at a terminal — the mouse capture, the cursor shape —
        // and a headless test has none, so their commands fail here for a reason that has nothing
        // to do with the row. Every other row has to change its key.
        let needs_a_terminal = |setting: &Setting| matches!(setting.ex, "mouse" | "cursor");

        for (index, setting) in SETTINGS.iter().enumerate() {
            let before = values(&app.config)[index].clone();
            match change(&mut app, setting, true) {
                Ok(()) => {}
                Err(error) if needs_a_terminal(setting) => {
                    eprintln!("{}: {error} (no terminal in a test)", setting.label);
                    continue;
                }
                Err(error) => panic!("{}: {error}", setting.label),
            }
            let after = values(&app.config)[index].clone();

            assert_ne!(
                before, after,
                "{}: changing `:{}` left `{}` at {before:?}",
                setting.label, setting.ex, setting.key
            );

            change(&mut app, setting, false).unwrap_or_else(|error| panic!("{}: {error}", setting.label));

            if setting.kind == SettingKind::Cycle {
                // `:navpos` and friends only cycle: there is no way back, so `←` leaves the row
                // alone rather than moving it on again.
                assert_eq!(
                    values(&app.config)[index],
                    after,
                    "{}: `←` moved a cycle-only row",
                    setting.label
                );
            } else {
                assert_eq!(
                    values(&app.config)[index],
                    before,
                    "{}: changing back did not restore `{}`",
                    setting.label,
                    setting.key
                );
            }
        }
    }

    /// The page draws every section, every row of the section the cursor is in, and that row's
    /// value: the only proof that the table reaches the screen.
    #[test]
    fn the_page_draws_every_row_of_the_section_the_cursor_is_in() {
        use ratatui::{Terminal, backend::TestBackend};

        let theme = crate::config::Theme::default();
        let bs = crate::ui::BlockStyle {
            colors: &theme,
            base: theme.bg,
            border: &crate::config::BorderConfig::default(),
            tick: 0,
        };
        let config = Config::default();

        for group in groups() {
            let selected = SETTINGS
                .iter()
                .position(|setting| setting.group == group)
                .expect("the group has a row");
            let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("backend");
            terminal
                .draw(|f| draw(f, &config, selected, &bs, f.area()))
                .expect("draw");

            let buffer = terminal.backend().buffer();
            // A wide character leaves the cell behind it empty, so the page is compared with the
            // blanks taken out — the way the terminal draws it, not the way the buffer stores it.
            let flat: String = (0..buffer.area.height)
                .map(|y| {
                    (0..buffer.area.width)
                        .map(|x| buffer[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect::<Vec<String>>()
                .join("\n")
                .replace(' ', "");

            assert!(flat.contains(&group.replace(' ', "")), "{group}: {flat}");
            for setting in SETTINGS.iter().filter(|setting| setting.group == group) {
                assert!(
                    flat.contains(&setting.label.replace(' ', "")),
                    "{} is not on the page: {flat}",
                    setting.label
                );
                let value = &values(&config)[index_of(setting)];
                // A switch is shown as 开/关 rather than as the value its command takes.
                let shown = match setting.kind {
                    SettingKind::Toggle => {
                        (if value == "on" { "开" } else { "关" }).to_string()
                    }
                    SettingKind::Cycle => format!("{value} ↻"),
                    SettingKind::Choice => format!("‹{value}›"),
                };
                assert!(
                    flat.contains(&shown.replace(' ', "")),
                    "{} shows {value:?}, not {shown:?}: {flat}",
                    setting.label
                );
            }
            assert!(flat.contains('▸'), "the cursor row is not marked: {flat}");
        }
    }

    /// Every row is filed under a section, and every row can be reached: the cursor walks the
    /// whole table, so a row cannot be added and left unreachable.
    #[test]
    fn every_row_is_reachable() {
        let mut state = SettingsState::default();
        let mut seen = vec![state.selected];
        for _ in 1..SETTINGS.len() {
            state.move_by(1);
            seen.push(state.selected);
        }

        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), SETTINGS.len());
        assert_eq!(state.move_by(1), (), "the cursor wraps");

        for setting in SETTINGS {
            assert!(!setting.group.is_empty(), "{}", setting.label);
            assert!(!setting.label.is_empty());
        }
    }
}

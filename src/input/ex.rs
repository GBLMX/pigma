//! Vim-style `:` command line: parsing, `Tab` completion and execution.
//!
//! The prompt mirrors the search field's editing behaviour (`TextInput`, `Esc`, `Enter`)
//! but executes ex commands instead of searching. Commands are named after what they do
//! rather than after a key, so `:volume +5` and the `+` key reach the same code, and the
//! volume grammar is the one `pigma msg volume` already accepts.

use crossterm::event::{KeyCode, KeyEvent};

use crate::{
    app::App,
    cli::parse_volume,
    event::{AppEvent, NavigationEvent},
    ipc::MsgAction,
    state::Page,
    text_input::TextInput,
};

/// Command names the prompt knows, used for `Tab` completion and suggestions.
const COMMANDS: [&str; 10] = [
    "help",
    "login",
    "pitch",
    "q",
    "quit",
    "save",
    "seek",
    "theme",
    "visualizer",
    "volume",
];

/// What a command line asks for.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum ExCommand {
    Quit,
    Help,
    Login,
    Save,
    Theme(String),
    Volume(String),
    Seek(String),
    Visualizer(bool),
    Pitch(bool),
}

impl ExCommand {
    /// Parse one command line, without the leading `:`.
    fn parse(line: &str) -> Result<Self, String> {
        let mut parts = line.split_whitespace();
        let Some(name) = parts.next() else {
            return Err("缺少命令".to_string());
        };
        let argument = parts.next();
        if parts.next().is_some() {
            return Err(format!("`{name}` 只接受一个参数"));
        }

        let required = |what: &str| {
            argument
                .map(str::to_string)
                .ok_or_else(|| format!("`{name}` 需要一个{what}"))
        };

        match name {
            "q" | "quit" => Ok(Self::Quit),
            "help" => Ok(Self::Help),
            "login" => Ok(Self::Login),
            "save" => Ok(Self::Save),
            "theme" => Ok(Self::Theme(required("主题名")?)),
            "volume" => Ok(Self::Volume(required("音量")?)),
            "seek" => Ok(Self::Seek(required("跳转位置")?)),
            "visualizer" => Ok(Self::Visualizer(parse_on_off(argument)?)),
            "pitch" => Ok(Self::Pitch(parse_on_off(argument)?)),
            other => Err(format!("未知命令: {other}")),
        }
    }
}

fn parse_on_off(argument: Option<&str>) -> Result<bool, String> {
    match argument {
        Some("on" | "true" | "1") => Ok(true),
        Some("off" | "false" | "0") => Ok(false),
        Some(other) => Err(format!("需要 on/off，得到 `{other}`")),
        None => Err("需要 on/off".to_string()),
    }
}

/// Split a command line into the part already fixed plus the word being typed:
/// `("theme ", "dra")` for `theme dra`, `("", "th")` while the name is still open.
fn split_head(line: &str) -> (String, &str) {
    match line.split_once(char::is_whitespace) {
        Some((name, rest)) => (format!("{name} "), rest.trim_start()),
        None => (String::new(), line.trim_start()),
    }
}

/// Completions for the word being typed: command names, theme names after `:theme`, and
/// `on`/`off` after the toggles.
fn candidate_names(head: &str, typed: &str, themes: &[&str]) -> Vec<String> {
    let name = head.trim();
    let pool: Vec<&str> = if name.is_empty() {
        COMMANDS.to_vec()
    } else {
        match name {
            "theme" => themes.to_vec(),
            "visualizer" | "pitch" => vec!["off", "on"],
            _ => Vec::new(),
        }
    };

    pool.into_iter()
        .filter(|candidate| candidate.starts_with(typed))
        .map(str::to_string)
        .collect()
}

/// Completed command line for `Tab`, or `None` when there is nothing to add.
fn completion(line: &str, themes: &[&str]) -> Option<String> {
    let (head, typed) = split_head(line);
    let candidates = candidate_names(&head, typed, themes);

    match candidates.as_slice() {
        [] => None,
        [only] => Some(format!("{head}{only} ")),
        many => {
            // Ambiguous: extend to the longest common prefix, as a shell would.
            let mut prefix = many[0].clone();
            for candidate in many {
                while !candidate.starts_with(&prefix) {
                    prefix.pop();
                }
            }
            (prefix.len() > typed.len()).then_some(format!("{head}{prefix}"))
        }
    }
}

/// Open the prompt on an empty line.
pub(super) fn open(app: &mut App) {
    app.state.prompt.active = true;
    app.state.prompt.input = TextInput::new();
}

/// Handle a key while the prompt is open; returns whether the key was consumed.
pub(super) fn handle_ex_key(app: &mut App, key_event: KeyEvent) -> bool {
    if !app.state.prompt.active {
        return false;
    }

    match key_event.code {
        KeyCode::Esc => close(app),
        KeyCode::Enter => {
            let line = app.state.prompt.input.value.trim().to_string();
            close(app);
            if !line.is_empty() {
                run(app, &line);
            }
        }
        KeyCode::Backspace => app.state.prompt.input.delete_char(),
        KeyCode::Left => app.state.prompt.input.move_left(),
        KeyCode::Right => app.state.prompt.input.move_right(),
        KeyCode::Tab => {
            // Theme names come from the live registry, so `:theme <Tab>` offers exactly
            // the themes this build can load, custom ones included.
            let mut themes = app.theme_registry.all_names();
            themes.sort_unstable();

            match completion(&app.state.prompt.input.value, &themes) {
                Some(completed) => {
                    let mut input = TextInput::new();
                    for ch in completed.chars() {
                        input.enter_char(ch);
                    }
                    app.state.prompt.input = input;
                }
                None => {
                    let (head, typed) = split_head(&app.state.prompt.input.value);
                    let matches = candidate_names(&head, typed, &themes);
                    if !matches.is_empty() {
                        app.toast(matches.join(" "));
                    }
                }
            }
        }
        KeyCode::Char(ch) => app.state.prompt.input.enter_char(ch),
        _ => {}
    }

    true
}

fn close(app: &mut App) {
    app.state.prompt.active = false;
    app.state.prompt.input = TextInput::new();
}

/// Run a command line, reporting the outcome the way vim reports errors.
fn run(app: &mut App, line: &str) {
    match ExCommand::parse(line) {
        Err(error) => app.toast(format!("E: {error}")),
        Ok(command) => {
            if let Err(error) = execute(app, command) {
                app.toast(format!("E: {error}"));
            }
        }
    }
}

fn execute(app: &mut App, command: ExCommand) -> Result<(), String> {
    match command {
        ExCommand::Quit => {
            app.state.events.send(AppEvent::Quit);
        }
        ExCommand::Help => app.state.help.toggle(),
        ExCommand::Login => app
            .state
            .events
            .send(NavigationEvent::Navigate(Page::Login)),
        ExCommand::Save => {
            app.config.save();
            app.toast("已保存配置".to_string());
        }
        ExCommand::Theme(name) => {
            app.config.default_theme = name.clone();
            app.config.save();
            app.toast(format!("主题: {name}"));
        }
        ExCommand::Volume(value) => match parse_volume(&value) {
            Err(error) => return Err(error.to_string()),
            Ok(MsgAction::Volume { absolute, delta }) => {
                let volume = match (absolute, delta) {
                    (Some(absolute), _) => absolute,
                    (None, Some(delta)) => app.playback.state.volume + delta,
                    (None, None) => return Err("音量参数无效".to_string()),
                };
                app.playback.set_volume(volume.clamp(0.0, 1.0));
            }
            Ok(_) => return Err("音量参数无效".to_string()),
        },
        ExCommand::Seek(value) => seek(app, &value)?,
        ExCommand::Visualizer(on) => {
            app.config.playerbar.visible.visualizer = on;
            app.config.save();
            app.toast(format!("频谱: {}", if on { "开" } else { "关" }));
        }
        ExCommand::Pitch(on) => {
            app.config.playerbar.visible.pitch = on;
            app.config.save();
            app.toast(format!("音高: {}", if on { "开" } else { "关" }));
        }
    }
    Ok(())
}

/// `:seek +15` / `:seek -30` move relative to the position, `:seek 90` jumps to a second,
/// and `:seek 50%` to a fraction of the track.
fn seek(app: &mut App, value: &str) -> Result<(), String> {
    let invalid = || format!("无效的跳转位置: {value}");

    if let Some(percent) = value.strip_suffix('%') {
        let percent: f64 = percent.parse().map_err(|_| invalid())?;
        app.playback.seek_to_fraction(percent / 100.0);
        return Ok(());
    }

    if value.starts_with('+') || value.starts_with('-') {
        let delta: f64 = value.parse().map_err(|_| invalid())?;
        app.playback.seek_relative(delta);
        return Ok(());
    }

    let seconds: f64 = value.parse().map_err(|_| invalid())?;
    let total_secs = app
        .playback
        .state
        .current_song
        .as_ref()
        .map(|song| song.duration as f64 / 1000.0)
        .filter(|total| *total > 0.0)
        .ok_or_else(|| "当前没有可跳转的歌曲".to_string())?;
    app.playback
        .seek_to_fraction((seconds / total_secs).clamp(0.0, 1.0));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_parse_into_their_arguments() {
        assert_eq!(ExCommand::parse("q"), Ok(ExCommand::Quit));
        assert_eq!(ExCommand::parse("quit"), Ok(ExCommand::Quit));
        assert_eq!(ExCommand::parse("save"), Ok(ExCommand::Save));
        assert_eq!(
            ExCommand::parse("theme dracula"),
            Ok(ExCommand::Theme("dracula".to_string()))
        );
        assert_eq!(
            ExCommand::parse("volume +5"),
            Ok(ExCommand::Volume("+5".to_string()))
        );
        assert_eq!(
            ExCommand::parse("seek 50%"),
            Ok(ExCommand::Seek("50%".to_string()))
        );
        assert_eq!(
            ExCommand::parse("visualizer on"),
            Ok(ExCommand::Visualizer(true))
        );
        assert_eq!(ExCommand::parse("pitch 0"), Ok(ExCommand::Pitch(false)));
        // surrounding whitespace is the user's business, not an error
        assert_eq!(ExCommand::parse("  q  "), Ok(ExCommand::Quit));
    }

    /// Each failure must explain itself: the toast is the only feedback there is.
    #[test]
    fn bad_lines_report_why() {
        let unknown = ExCommand::parse("frobnicate").unwrap_err();
        assert!(unknown.contains("frobnicate"), "got {unknown}");

        let missing = ExCommand::parse("theme").unwrap_err();
        assert!(missing.contains("主题名"), "got {missing}");

        let extra = ExCommand::parse("theme a b").unwrap_err();
        assert!(extra.contains("只接受一个参数"), "got {extra}");

        let bad_flag = ExCommand::parse("pitch maybe").unwrap_err();
        assert!(bad_flag.contains("on/off"), "got {bad_flag}");

        assert!(ExCommand::parse("   ").is_err());
    }

    /// Theme names the completion pool is fed from in these tests.
    const THEMES: [&str; 3] = ["catppuccin_mocha", "default", "dracula"];

    #[test]
    fn tab_completes_unambiguous_names() {
        assert_eq!(completion("h", &THEMES), Some("help ".to_string()));
        assert_eq!(completion("vol", &THEMES), Some("volume ".to_string()));
        // nothing to add once the name is complete
        assert_eq!(completion("help ", &THEMES), None);
        // an unknown name has no candidates
        assert_eq!(completion("zzz", &THEMES), None);
    }

    #[test]
    fn tab_extends_to_the_common_prefix_when_ambiguous() {
        // `s` could be save or seek, so nothing can be added
        assert_eq!(completion("s", &THEMES), None);
        assert_eq!(candidate_names("", "s", &THEMES), vec!["save", "seek"]);
        // an empty line offers every command
        assert_eq!(candidate_names("", "", &THEMES).len(), COMMANDS.len());
        assert_eq!(completion("", &THEMES), None);
    }

    /// `:theme` completes the names the registry actually has, and the toggles complete
    /// their own two keywords.
    #[test]
    fn arguments_complete_from_the_pool_of_their_command() {
        assert_eq!(
            completion("theme dra", &THEMES),
            Some("theme dracula ".to_string())
        );
        assert_eq!(
            completion("theme cat", &THEMES),
            Some("theme catppuccin_mocha ".to_string())
        );
        assert_eq!(completion("theme zzz", &THEMES), None);

        // every theme matches an empty argument: Tab lists them instead of completing
        assert_eq!(completion("theme ", &THEMES), None);
        assert_eq!(
            candidate_names("theme ", "", &THEMES),
            vec!["catppuccin_mocha", "default", "dracula"]
        );

        assert_eq!(
            completion("pitch of", &THEMES),
            Some("pitch off ".to_string())
        );
        // `on` shares its first letter with `off`, so prefix completion cannot reach it —
        // the same limit a shell has — but the already-complete form stays put
        assert_eq!(completion("pitch n", &THEMES), None);
        assert_eq!(
            completion("pitch on", &THEMES),
            Some("pitch on ".to_string())
        );
        assert_eq!(
            candidate_names("visualizer ", "n", &THEMES),
            Vec::<String>::new()
        );
        assert_eq!(
            candidate_names("visualizer ", "", &THEMES),
            vec!["off", "on"]
        );

        // a command without an argument pool has nothing to offer
        assert_eq!(completion("seek 1", &THEMES), None);
        assert!(candidate_names("seek ", "1", &THEMES).is_empty());
    }
}

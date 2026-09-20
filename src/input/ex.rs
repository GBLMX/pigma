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

/// Commands whose name starts with what has been typed, for the completion hint.
fn candidates(line: &str) -> Vec<&'static str> {
    let name = line.trim_start();
    if name.contains(' ') {
        return Vec::new(); // only the command name completes
    }
    COMMANDS
        .iter()
        .copied()
        .filter(|command| command.starts_with(name))
        .collect()
}

/// Completed command line for `Tab`, or `None` when there is nothing to add.
fn completion(line: &str) -> Option<String> {
    let candidates = candidates(line);
    match candidates.as_slice() {
        [] => None,
        [only] => Some(format!("{only} ")),
        many => {
            // Ambiguous: extend to the longest common prefix, as a shell would.
            let mut prefix = many[0].to_string();
            for candidate in many {
                while !candidate.starts_with(&prefix) {
                    prefix.pop();
                }
            }
            let typed = line.trim_start();
            (prefix.len() > typed.len()).then_some(prefix)
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
        KeyCode::Tab => match completion(&app.state.prompt.input.value) {
            Some(completed) => {
                let mut input = TextInput::new();
                for ch in completed.chars() {
                    input.enter_char(ch);
                }
                app.state.prompt.input = input;
            }
            None => {
                let matches = candidates(&app.state.prompt.input.value);
                if !matches.is_empty() {
                    app.toast(matches.join(" "));
                }
            }
        },
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

    #[test]
    fn tab_completes_unambiguous_names() {
        assert_eq!(completion("h"), Some("help ".to_string()));
        assert_eq!(completion("vol"), Some("volume ".to_string()));
        // nothing to add once the name is complete
        assert_eq!(completion("help "), None);
        // arguments do not complete
        assert_eq!(completion("theme dra"), None);
        // an unknown name has no candidates
        assert_eq!(completion("zzz"), None);
    }

    #[test]
    fn tab_extends_to_the_common_prefix_when_ambiguous() {
        // `s` could be save or seek, so nothing can be added
        assert_eq!(completion("s"), None);
        assert_eq!(candidates("s"), vec!["save", "seek"]);
        // an empty line offers every command
        assert_eq!(candidates("").len(), COMMANDS.len());
        assert_eq!(completion(""), None);
    }
}

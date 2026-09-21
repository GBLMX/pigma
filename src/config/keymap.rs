//! Which key runs which command.
//!
//! The key map is *derived*: every row of [`COMMANDS`](crate::state::command::COMMANDS) already
//! says which key does the same thing as its command, and that column is read here rather than
//! hand-written a second time — a command that gains a key gains it in the `:` line, the palette,
//! the help and the keyboard at once.
//!
//! `[keys]` in the config rebinds them: `visualizer = "w"`, or `visualizer = ""` to leave a
//! command to the `:` line alone. This is the shape Yazi and spotify-player use — a keymap that
//! names commands rather than code — and it is what makes rebinding possible without a rebuild.
//!
//! A binding may be a *sequence*, the way Helix's key trie allows `g g` and `Ctrl-w v`: the tokens
//! are separated by spaces (`spin = "z z"`, `spin = "ctrl+w s"`). Nothing has a timeout — an
//! unfinished sequence waits for its next key and `Esc` gives up on it — which is what a modal
//! keymap does, and what makes `g` followed by `g` a command rather than two commands.

use std::collections::BTreeMap;

use crate::config::Config;
use crate::state::command::COMMANDS;

/// One key of a binding: the character, and whether Ctrl or Alt is held with it. Shift is part of
/// the character (`G` is shift-`g` and is written `G`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    pub code: char,
    pub ctrl: bool,
    pub alt: bool,
}

impl Key {
    /// A plain character, for the table's own keys.
    pub fn char(code: char) -> Self {
        Self {
            code,
            ctrl: false,
            alt: false,
        }
    }

    /// One token of a binding: `z`, `ctrl+w`, `alt+x`.
    pub fn parse(token: &str) -> Option<Self> {
        let token = token.trim();
        let (ctrl, alt, rest) = match token.split_once('+') {
            Some((modifier, rest)) if modifier.eq_ignore_ascii_case("ctrl") => (true, false, rest),
            Some((modifier, rest)) if modifier.eq_ignore_ascii_case("alt") => (false, true, rest),
            _ => (false, false, token),
        };
        let mut chars = rest.chars();
        let code = chars.next()?;
        if chars.next().is_some() {
            return None;
        }

        Some(Self { code, ctrl, alt })
    }
}

/// A binding's keys: one, or a sequence.
pub type Chord = Vec<Key>;

/// What the key map made of a key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pressed {
    /// The keys typed so far are a command: run it. What was pending is finished with.
    Run(&'static str),
    /// They are the start of a longer binding: hold them and wait for the next key.
    Wait,
    /// No binding starts that way: forget what was pending and let the key through to the rest of
    /// the key map.
    FallThrough,
}

/// The command each key (or key sequence) runs.
#[derive(Debug, Default, Clone)]
pub struct Keymap {
    /// Bindings, in the table's order, with the config's overrides applied. A `Vec` rather than a
    /// map: there are a dozen of them, and the order is what the log and the tests read.
    bindings: Vec<(Chord, &'static str)>,
}

impl Keymap {
    /// The key map for `config`: the commands' own keys, with `[keys]` applied over them.
    pub fn from_config(config: &Config) -> Self {
        let mut bindings: Vec<(Chord, &'static str)> = COMMANDS
            .iter()
            .filter_map(|command| Some((chord_of(command.key?)?, command.name)))
            .collect();

        for (name, spec) in &config.keys {
            let Some(command) = COMMANDS.iter().find(|command| command.name == name) else {
                if crate::config::theme::report_unknown_field_once(&format!("key {name}")) {
                    log::warn!("[keys] names `{name}`, which is not a command; ignored");
                }
                continue;
            };

            match spec.trim() {
                // An empty value is how a user says "no key": the command keeps its `:` line and
                // its palette entry, and stops answering to a key stroke.
                "" => {
                    bindings.retain(|(_, bound)| *bound != command.name);
                }
                _ => match chord_of(spec) {
                    // A command answers to one binding, so this *moves* it: the binding it had in
                    // the table is freed, and the keys it is given are taken from whoever had them.
                    Some(chord) => {
                        bindings.retain(|(_, bound)| *bound != command.name);
                        bindings.retain(|(existing, _)| *existing != chord);
                        bindings.push((chord, command.name));
                    }
                    // Anything else is a value nobody can use: say so once and leave the key map as
                    // the table wrote it, rather than unbinding the command by accident.
                    None => {
                        if crate::config::theme::report_unknown_field_once(&format!(
                            "key {name}={spec}"
                        )) {
                            log::warn!("[keys] `{name} = \"{spec}\"` is not a key sequence; ignored");
                        }
                    }
                },
            }
        }

        Self { bindings }
    }

    /// What the keys typed so far (`pending`, which this may add to) mean now.
    ///
    /// The caller keeps `pending` between presses and clears it when this says [`Pressed::Run`] or
    /// [`Pressed::FallThrough`]; it clears it on `Esc` as well, which is what gives up on a
    /// half-typed sequence.
    pub fn advance(&self, pending: &mut Chord, key: Key) -> Pressed {
        pending.push(key);

        let mut longer = false;
        for (chord, name) in &self.bindings {
            if chord.len() < pending.len() || chord[..pending.len()] != pending[..] {
                continue;
            }
            if chord.len() == pending.len() {
                // An exact match runs, even when a longer binding starts the same way: a key that
                // is bound must do something when it is typed.
                pending.clear();
                return Pressed::Run(name);
            }
            longer = true;
        }

        if longer {
            Pressed::Wait
        } else {
            pending.clear();
            Pressed::FallThrough
        }
    }

    /// The command a single key runs, if it runs one: what a test asks when it wants to know
    /// whether one key is bound, without typing a sequence.
    pub fn command(&self, key: char) -> Option<&'static str> {
        self.bindings
            .iter()
            .find(|(chord, _)| chord.len() == 1 && chord[0] == Key::char(key))
            .map(|(_, command)| *command)
    }

    /// Every binding, for the tests and the help.
    pub fn bindings(&self) -> &[(Chord, &'static str)] {
        &self.bindings
    }
}

/// The keys a binding names: one token per key, space-separated. The table's help column also
/// carries spellings that are not keys at all (`Tab / ⇧Tab`) — a binding that cannot be read binds
/// nothing, and the caller says so.
fn chord_of(spec: &str) -> Option<Chord> {
    let spec = spec.trim();
    if spec.chars().count() == 1 && !spec.contains(' ') {
        return Some(vec![Key::char(spec.chars().next().expect("one character"))]);
    }

    let chord: Chord = spec.split_whitespace().map(Key::parse).collect::<Option<_>>()?;

    (!chord.is_empty()).then_some(chord)
}

/// The `[keys]` block, as written in the config: command name → the key it answers to.
pub type KeyBindings = BTreeMap<String, String>;

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with(bindings: &[(&str, &str)]) -> Config {
        Config {
            keys: bindings
                .iter()
                .map(|(name, spec)| ((*name).to_string(), (*spec).to_string()))
                .collect(),
            ..Config::default()
        }
    }

    /// Every key the command table advertises is bound, and to that command: the table is the
    /// key map, so a key that stops being bound means a row lost its `key`, not that the keyboard
    /// forgot.
    #[test]
    fn the_tables_keys_are_the_key_map() {
        let keymap = Keymap::from_config(&Config::default());

        for command in COMMANDS {
            let Some(spec) = command.key else {
                continue;
            };
            let Some(chord) = chord_of(spec) else {
                continue;
            };
            if chord.len() != 1 {
                continue;
            }
            let key = chord[0].code;
            assert_eq!(
                keymap.command(key),
                Some(command.name),
                "{key} is the key of `{}` in the table",
                command.name
            );
        }
    }

    /// No key runs two commands: the table has one row per key, and a `[keys]` entry that moves a
    /// command onto another's key takes it over rather than leaving both.
    #[test]
    fn no_key_runs_two_commands() {
        let keymap = Keymap::from_config(&config_with(&[("visualizer", "t")]));

        assert_eq!(keymap.command('t'), Some("visualizer"));
        assert_eq!(
            keymap
                .bindings()
                .iter()
                .filter(|(chord, _)| chord.len() == 1 && chord[0] == Key::char('t'))
                .count(),
            1,
            "one key, one command"
        );
        assert_eq!(
            keymap.command('v'),
            None,
            "and the command it moved off leaves its old key empty"
        );
    }

    /// `""` unbinds: the command is left to the `:` line, which is how a user who does not want a
    /// one-key toggle stops typing it by accident.
    #[test]
    fn an_empty_binding_unbinds() {
        let keymap = Keymap::from_config(&config_with(&[("spin", "")]));

        assert_eq!(keymap.command('t'), None);
    }

    /// A sequence waits for its next key, runs when it is complete, and is given up on — rather
    /// than half-run — when the next key cannot continue it.
    #[test]
    fn a_sequence_waits_and_then_runs() {
        let keymap = Keymap::from_config(&config_with(&[("spin", "e e"), ("pitch", "ctrl+w s")]));
        let mut pending = Vec::new();
        let e = Key::char('e');

        assert_eq!(keymap.advance(&mut pending, e), Pressed::Wait);
        assert_eq!(pending, vec![e], "the first key is held");
        assert_eq!(keymap.advance(&mut pending, e), Pressed::Run("spin"));
        assert!(pending.is_empty(), "a finished sequence is done with");

        // A key the sequence cannot continue: nothing ran, and nothing is held.
        pending.clear();
        assert_eq!(keymap.advance(&mut pending, e), Pressed::Wait);
        assert_eq!(
            keymap.advance(&mut pending, Key::char('x')),
            Pressed::FallThrough
        );
        assert!(pending.is_empty());

        // Modified keys are part of a sequence too.
        let mut pending = Vec::new();
        let ctrl_w = Key {
            code: 'w',
            ctrl: true,
            alt: false,
        };
        assert_eq!(keymap.advance(&mut pending, ctrl_w), Pressed::Wait);
        assert_eq!(
            keymap.advance(&mut pending, Key::char('s')),
            Pressed::Run("pitch")
        );
    }

    /// A key that is bound on its own runs at once, even when a longer binding starts the same
    /// way: a key the user can see in the help has to do something when it is pressed.
    #[test]
    fn an_exact_match_runs_before_a_longer_binding_waits() {
        let keymap = Keymap::from_config(&config_with(&[("spin", "z z")]));
        let mut pending = Vec::new();

        assert_eq!(
            keymap.advance(&mut pending, Key::char('z')),
            Pressed::Run("navpos"),
            "`z` is `navpos` in the table, and it wins over the sequence that starts with it"
        );
        assert!(pending.is_empty());
    }

    /// A name that is not a command, or a binding that is not one key, changes nothing: the key
    /// map stays as the table wrote it rather than losing the key it could not read.
    #[test]
    fn nonsense_bindings_are_ignored() {
        let keymap = Keymap::from_config(&config_with(&[
            ("no_such_command", "x"),
            ("pitch", "shift+V"),
        ]));

        assert_eq!(keymap.command('x'), None);
        assert_eq!(
            keymap.command('V'),
            Some("pitch"),
            "an unreadable binding leaves the table's own key alone"
        );
    }
}

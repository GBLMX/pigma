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

use std::collections::BTreeMap;

use crate::config::Config;
use crate::state::command::COMMANDS;

/// The command each key runs.
#[derive(Debug, Default, Clone)]
pub struct Keymap {
    /// Bound keys, in the table's order, with the config's overrides applied. A `Vec` rather than
    /// a map: there are a dozen of them, and the order is what the log and the tests read.
    bindings: Vec<(char, &'static str)>,
}

impl Keymap {
    /// The key map for `config`: the commands' own keys, with `[keys]` applied over them.
    pub fn from_config(config: &Config) -> Self {
        let mut bindings: Vec<(char, &'static str)> = COMMANDS
            .iter()
            .filter_map(|command| Some((one_key(command.key?)?, command.name)))
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
                // One character is the key. A command answers to one key, so this *moves* it: the
                // key it had in the table is freed, and the key it is given is taken from whoever
                // had it.
                one if one.chars().count() == 1 => {
                    let key = one.chars().next().expect("one character");
                    bindings.retain(|(_, bound)| *bound != command.name);
                    bindings.retain(|(bound, _)| *bound != key);
                    bindings.push((key, command.name));
                }
                // Anything else is a value nobody can use: say so once and leave the key map as
                // the table wrote it, rather than unbinding the command by accident.
                other => {
                    if crate::config::theme::report_unknown_field_once(&format!("key {name}={other}"))
                    {
                        log::warn!("[keys] `{name} = \"{other}\"` is not one key; ignored");
                    }
                }
            }
        }

        Self { bindings }
    }

    /// The command a key runs, if it runs one.
    pub fn command(&self, key: char) -> Option<&'static str> {
        self.bindings
            .iter()
            .find(|(bound, _)| *bound == key)
            .map(|(_, command)| *command)
    }

    /// Every binding, for the tests and the help.
    pub fn bindings(&self) -> &[(char, &'static str)] {
        &self.bindings
    }
}

/// The single character a binding names, if it names one. The table's help column also carries
/// spellings that are not one key (`Tab / ⇧Tab`); those bind nothing here.
fn one_key(spec: &str) -> Option<char> {
    let spec = spec.trim();
    (spec.chars().count() == 1).then(|| spec.chars().next().expect("one character"))
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
            let Some(key) = one_key(spec) else {
                continue;
            };
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
                .filter(|(key, _)| *key == 't')
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

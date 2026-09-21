//! Desktop notifications for things that happen while the user is looking elsewhere.

use serde::{Deserialize, Serialize};

use crate::utils::Named;

/// Which events raise a terminal notification. Off by default, like opencode's `attention`
/// block: an app that starts notifying on its own is worse than one that waits to be asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NotifyConfig {
    /// Announce the song that starts playing — the one thing a music player has to say.
    pub song_change: bool,
    /// Announce playback failures: a stream that cannot be opened, a lost output device.
    pub errors: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_announced_unless_asked() {
        let config = NotifyConfig::default();
        assert!(!config.song_change && !config.errors);

        let parsed: NotifyConfig = toml_edit::de::from_str("song_change = true").expect("parse");
        assert!(
            parsed.song_change && !parsed.errors,
            "the rest keeps its default"
        );
    }
}

/// One `[notify]` switch: the events the app can announce.
///
/// It lives here rather than with the `:notify` command because it *is* the config block: the two
/// variants are the two keys, so the switch, its name, its column in the config and the word the
/// command line takes are one thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifySwitch {
    /// Announce the song that starts playing.
    SongChange,
    /// Announce playback failures.
    Errors,
}

impl Named for NotifySwitch {
    const ALL: &'static [Self] = &[Self::SongChange, Self::Errors];

    fn name(self) -> &'static str {
        match self {
            Self::SongChange => "song_change",
            Self::Errors => "errors",
        }
    }

    fn describe(self) -> &'static str {
        match self {
            Self::SongChange => "切歌时通知当前曲目",
            Self::Errors => "播放出错时通知",
        }
    }
}

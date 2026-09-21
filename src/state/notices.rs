//! The notices the app has raised: what a toast says, and what `:messages` lists.
//!
//! One line on screen used to be the whole of it — `toast_msg` and the time it arrived — so a
//! second notice replaced the first before anyone could read it, and nothing could say *how* it
//! went wrong. A queue keeps them: the newest is what the toast shows, the rest are what
//! `:messages` shows, and the level is what colours both (Yazi's `notify` layer, which keeps a
//! list of messages with a level and a key of its own to read them).

use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

/// How bad a notice is: it decides what the toast looks like, and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    /// How long a notice of this level stays on screen. A failure is worth reading twice as long
    /// as a confirmation.
    pub fn linger(self) -> Duration {
        match self {
            Self::Info => Duration::from_secs(3),
            Self::Warn => Duration::from_secs(5),
            Self::Error => Duration::from_secs(8),
        }
    }
}

/// One notice.
#[derive(Debug, Clone)]
pub struct Notice {
    pub level: Level,
    pub text: String,
    pub at: Instant,
}

/// The notices, oldest first, capped: a session that runs for hours must not grow a list nobody
/// will ever read. What is dropped is the oldest, which is the one furthest from the screen.
#[derive(Debug, Default)]
pub struct Notices {
    entries: VecDeque<Notice>,
}

/// How many notices are kept for `:messages`.
const KEEP: usize = 100;

impl Notices {
    /// Raise a notice.
    pub fn push(&mut self, level: Level, text: impl Into<String>) {
        self.entries.push_back(Notice {
            level,
            text: text.into(),
            at: Instant::now(),
        });

        while self.entries.len() > KEEP {
            self.entries.pop_front();
        }
    }

    /// The notice the toast shows: the newest, while it is still worth reading.
    pub fn latest(&self, now: Instant) -> Option<&Notice> {
        let notice = self.entries.back()?;

        (now.saturating_duration_since(notice.at) < notice.level.linger()).then_some(notice)
    }

    /// Every notice kept, oldest first, for `:messages`.
    pub fn all(&self) -> impl DoubleEndedIterator<Item = &Notice> {
        self.entries.iter()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The newest notice is the one on screen, and it stops being shown once its level's time is
    /// up — that is what makes a toast a toast rather than a permanent line.
    #[test]
    fn the_toast_is_the_newest_notice_while_it_lasts() {
        let mut notices = Notices::default();
        let start = Instant::now();

        notices.push(Level::Info, "first");
        assert_eq!(
            notices.latest(start).map(|n| n.text.as_str()),
            Some("first")
        );

        notices.push(Level::Error, "second");
        assert_eq!(
            notices.latest(start).map(|n| n.text.as_str()),
            Some("second"),
            "the newest replaces the one before it"
        );

        let later = start + Level::Error.linger() + Duration::from_millis(1);
        assert!(notices.latest(later).is_none(), "and it fades");
    }

    /// An error is worth reading for longer than a confirmation.
    #[test]
    fn an_error_lingers_longer_than_a_confirmation() {
        assert!(Level::Error.linger() > Level::Info.linger());
    }

    /// The list keeps what was raised, oldest first, and drops the oldest when it is full: the
    /// history is for `:messages`, not for the memory to grow.
    #[test]
    fn the_history_keeps_the_newest_and_drops_the_oldest() {
        let mut notices = Notices::default();

        for i in 0..KEEP + 10 {
            notices.push(Level::Info, format!("notice {i}"));
        }

        let kept: Vec<&str> = notices.all().map(|n| n.text.as_str()).collect();
        assert_eq!(kept.len(), KEEP);
        assert_eq!(kept.first(), Some(&"notice 10"));
        assert_eq!(kept.last(), Some(&format!("notice {}", KEEP + 9).as_str()));
    }
}

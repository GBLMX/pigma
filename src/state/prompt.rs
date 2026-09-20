//! The vim-style `:` command line.

use crate::text_input::TextInput;

/// Single-line ex prompt: `:` opens it, `Enter` runs the line, `Esc` drops it.
#[derive(Debug, Default)]
pub struct PromptState {
    pub active: bool,
    pub input: TextInput,
    /// Lines that ran, most recent last. Reopening the prompt starts from a blank line but
    /// keeps this, so a command used once can be run again without retyping it.
    pub history: Vec<String>,
    /// Where in `history` the line being edited came from; `None` while it is a fresh draft.
    pub history_index: Option<usize>,
}

impl PromptState {
    /// Remember a line that ran: vim's `:` history, minus repeats of the line right before.
    pub fn remember(&mut self, line: &str) {
        if line.is_empty() || self.history.last().is_some_and(|last| last == line) {
            return;
        }
        self.history.push(line.to_string());
        self.history_index = None;
    }

    /// `Up`: step towards older lines, stopping at the oldest.
    pub fn recall_previous(&mut self) {
        let index = match self.history_index {
            None => self.history.len().checked_sub(1),
            Some(0) => None,
            Some(index) => Some(index - 1),
        };
        if let Some(index) = index {
            self.history_index = Some(index);
            let line = self.history[index].clone();
            self.set_input(&line);
        }
    }

    /// `Down`: step towards newer lines, and past the newest back to an empty draft.
    pub fn recall_next(&mut self) {
        let index = match self.history_index {
            None => return,
            Some(index) if index + 1 < self.history.len() => Some(index + 1),
            Some(_) => None,
        };
        self.history_index = index;
        let line = index.map_or_else(String::new, |index| self.history[index].clone());
        self.set_input(&line);
    }

    /// Typing leaves the history walk, so the next `Up` starts from the newest line again.
    pub fn leave_history(&mut self) {
        self.history_index = None;
    }

    fn set_input(&mut self, line: &str) {
        let mut input = TextInput::new();
        for ch in line.chars() {
            input.enter_char(ch);
        }
        self.input = input;
    }
}

#[cfg(test)]
mod tests {
    use super::PromptState;

    /// `Up`/`Down` walk the lines that ran, so the second `:` can rerun the first command
    /// instead of starting from nothing.
    #[test]
    fn history_walks_back_and_forth_to_the_draft() {
        let mut prompt = PromptState::default();
        prompt.remember("signin someone@example.com hunter2");
        prompt.remember("volume +5");
        prompt.remember("volume +5");
        assert_eq!(
            prompt.history,
            ["signin someone@example.com hunter2", "volume +5"]
        );

        prompt.recall_previous();
        assert_eq!(prompt.input.value, "volume +5");
        prompt.recall_previous();
        assert_eq!(prompt.input.value, "signin someone@example.com hunter2");
        prompt.recall_previous();
        assert_eq!(
            prompt.input.value, "signin someone@example.com hunter2",
            "oldest"
        );

        prompt.recall_next();
        assert_eq!(prompt.input.value, "volume +5");
        prompt.recall_next();
        assert_eq!(prompt.input.value, "", "past the newest is a fresh line");
        assert_eq!(prompt.history_index, None);

        prompt.recall_previous();
        assert_eq!(prompt.input.value, "volume +5");
        prompt.leave_history();
        assert_eq!(prompt.history_index, None);
    }

    /// An empty history must not panic or invent lines.
    #[test]
    fn an_empty_history_recalls_nothing() {
        let mut prompt = PromptState::default();
        prompt.recall_previous();
        prompt.recall_next();
        assert_eq!(prompt.input.value, "");
        assert!(prompt.history.is_empty());
    }
}

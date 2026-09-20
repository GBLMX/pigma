//! The vim-style `:` command line.

use crate::text_input::TextInput;

/// Single-line ex prompt: `:` opens it, `Enter` runs the line, `Esc` drops it.
#[derive(Debug, Default)]
pub struct PromptState {
    pub active: bool,
    pub input: TextInput,
}

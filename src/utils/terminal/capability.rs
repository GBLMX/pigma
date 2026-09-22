//! What the terminal is: the colour depth it can show, the graphics protocol it can draw
//! covers with, and the cursor shape it was asked for.
//!
//! Everything here is a fact about the terminal, or a decision made from one. The bytes that
//! act on those facts live in `sequences`.

use std::{env, sync::LazyLock};

use serde::{Deserialize, Serialize};

/// A graphics protocol a terminal can draw cover art with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageProtocol {
    Kitty,
    ITerm2,
    Sixel,
}

/// Shape of the terminal's cursor while an input field has focus.
///
/// Kitty's `tui.json` and opencode's are the model: the app asks for a shape, and the
/// terminal's own default stays available as a choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CursorStyle {
    /// Whatever the user configured in the terminal.
    #[default]
    Default,
    Block,
    Underline,
    Bar,
}

impl crate::utils::Named for CursorStyle {
    const ALL: &'static [Self] = &[Self::Default, Self::Block, Self::Underline, Self::Bar];

    fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Block => "block",
            Self::Underline => "underline",
            Self::Bar => "bar",
        }
    }
}

impl CursorStyle {
    /// The escape sequence this style is written as.
    pub fn command(self) -> crossterm::cursor::SetCursorStyle {
        use crossterm::cursor::SetCursorStyle;
        match self {
            Self::Default => SetCursorStyle::DefaultUserShape,
            Self::Block => SetCursorStyle::SteadyBlock,
            Self::Underline => SetCursorStyle::SteadyUnderScore,
            Self::Bar => SetCursorStyle::SteadyBar,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageProtocolChoice {
    /// Ask the terminal and trust what it says.
    #[default]
    Auto,
    /// Force the kitty graphics protocol.
    Kitty,
    /// Force the iTerm2 inline-image protocol.
    #[serde(rename = "iterm2")]
    ITerm2,
    /// Force sixels.
    Sixel,
    /// Never use graphics: draw covers with half blocks.
    Halfblocks,
}

/// Decide how covers are drawn.
///
/// `queried` is what the terminal answered when asked (the kitty graphics query, or
/// the sixel flag in DA1). That answer is the only signal that tells a graphics-capable
/// terminal apart from a shell that merely inherited the variables, so it is trusted
/// ahead of anything read from the environment — with two exceptions, both of which
/// come from upstream's compatibility matrix:
///
/// * WezTerm, Rio and iTerm2 answer the kitty graphics query, but that matrix is
///   explicit that only iTerm2's own protocol renders bug-free there ("would support
///   Sixel and Kitty, but only iTerm2 actually works bug-free").
/// * Inside tmux, graphics only reach the real terminal when the user turned
///   passthrough on, and there is no way to ask. Guessing wrong paints graphics over
///   the UI, so half blocks win; `image_protocol` overrides that.
pub fn choose_image_protocol(
    choice: ImageProtocolChoice,
    queried: Option<ImageProtocol>,
    tmux_detected: bool,
    lookup: impl Fn(&str) -> Option<String>,
) -> Option<ImageProtocol> {
    match choice {
        ImageProtocolChoice::Kitty => return Some(ImageProtocol::Kitty),
        ImageProtocolChoice::ITerm2 => return Some(ImageProtocol::ITerm2),
        ImageProtocolChoice::Sixel => return Some(ImageProtocol::Sixel),
        ImageProtocolChoice::Halfblocks => return None,
        ImageProtocolChoice::Auto => {}
    }

    if tmux_detected {
        return None;
    }

    if matches!(
        lookup("TERM_PROGRAM").as_deref(),
        Some("WezTerm" | "rio" | "iterm.app")
    ) {
        return Some(ImageProtocol::ITerm2);
    }

    if let Some(protocol) = queried {
        return Some(protocol);
    }

    // The query went unanswered (or the terminal only reported half blocks): fall back
    // to what the environment says about the terminals known to render sixels.
    if sixel_available(&lookup) {
        return Some(ImageProtocol::Sixel);
    }

    // Kitty and Ghostty are the reference implementations of the kitty protocol. Both
    // are recognised from their own variables, because `TERM_PROGRAM` is not set on
    // every launch path (a shell that did not set it does not pass it on).
    if is_kitty_terminal(&lookup) {
        return Some(ImageProtocol::Kitty);
    }

    None
}

pub(super) fn is_kitty_terminal(lookup: &impl Fn(&str) -> Option<String>) -> bool {
    if lookup("KITTY_WINDOW_ID").is_some()
        || lookup("KITTY_PID").is_some()
        || lookup("GHOSTTY_RESOURCES_DIR").is_some()
    {
        return true;
    }

    if matches!(lookup("TERM_PROGRAM").as_deref(), Some("kitty" | "ghostty")) {
        return true;
    }

    matches!(
        lookup("TERM").as_deref(),
        Some(t) if t.to_lowercase().contains("kitty") || t == "xterm-ghostty"
    )
}

fn sixel_available(lookup: &impl Fn(&str) -> Option<String>) -> bool {
    if lookup("FOOT_VERSION").is_some() {
        return true;
    }

    if lookup("WT_SESSION").is_some() {
        return true;
    }

    match lookup("TERM_PROGRAM").as_deref() {
        Some("vscode") => {
            if version_gte(
                &lookup("TERM_PROGRAM_VERSION").unwrap_or_default(),
                1,
                80,
                0,
            ) {
                return true;
            }
        }
        Some("rio") => {
            // Rio started supporting the graphics protocol reasonably well after 0.0.12
            if version_gte(
                &lookup("TERM_PROGRAM_VERSION").unwrap_or_default(),
                0,
                0,
                12,
            ) {
                return true;
            }
        }
        Some("mintty") => return true,
        Some("WezTerm") => {
            if wezterm_sixel_supported(&lookup("WEZTERM_VERSION").unwrap_or_default()) {
                return true;
            }
        }
        // Konsole is deliberately absent: upstream's compatibility matrix lists its
        // sixel support as not really fixed (as of 24.12), so it is left to the
        // terminal's own answer instead of being assumed capable here.
        Some("WindowsTerminal" | "Windows_Terminal")
            if version_gte(
                &lookup("TERM_PROGRAM_VERSION").unwrap_or_default(),
                1,
                22,
                0,
            ) =>
        {
            return true;
        }
        _ => {}
    }

    matches!(
        lookup("TERM").as_deref(),
        Some(t) if t.to_lowercase().starts_with("foot") || t.to_lowercase().starts_with("mlterm")
    )
}

fn version_gte(version_str: &str, major: u32, minor: u32, patch: u32) -> bool {
    let parts: Vec<u32> = version_str
        .split('.')
        .map(|s| {
            s.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
        })
        .filter_map(|s| s.parse().ok())
        .collect();

    let v_major = parts.first().copied().unwrap_or(0);
    let v_minor = parts.get(1).copied().unwrap_or(0);
    let v_patch = parts.get(2).copied().unwrap_or(0);

    (v_major, v_minor, v_patch) >= (major, minor, patch)
}

fn wezterm_sixel_supported(version: &str) -> bool {
    if let Some(date_part) = version.split('-').next()
        && let Ok(date_num) = date_part.parse::<u32>()
    {
        return date_num >= 20220600;
    }
    false
}

/// How many colors the terminal can display; themes are down-sampled to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorMode {
    #[default]
    TrueColor,
    Ansi256,
    Basic,
}

/// Terminals that advertise true color in their own font/graphics era even when
/// `COLORTERM` is missing (older builds, ssh sessions without the variable).
const TRUE_COLOR_PROGRAMS: [&str; 6] = [
    "kitty",
    "wezterm",
    "alacritty",
    "ghostty",
    "iTerm.app",
    "vscode",
];

/// Color capability of the current terminal, detected once from the environment.
pub static COLOR_MODE: LazyLock<ColorMode> =
    LazyLock::new(|| color_mode_from(|key| env::var(key).ok()));

/// Detect the color capability from environment values.
///
/// `COLORTERM`/`WT_SESSION` mean true color; `dumb` and the Linux VGA console only have
/// the 16 base colors; `-256color` terms get the 256-color palette; anything else is
/// treated as 256 colors, which every terminal of the last two decades supports.
pub fn color_mode_from(lookup: impl Fn(&str) -> Option<String>) -> ColorMode {
    if let Some(value) = lookup("COLORTERM") {
        let value = value.to_ascii_lowercase();
        if value.contains("truecolor") || value.contains("24bit") {
            return ColorMode::TrueColor;
        }
    }
    if lookup("WT_SESSION").is_some() {
        return ColorMode::TrueColor;
    }

    let term = lookup("TERM").unwrap_or_default().to_ascii_lowercase();
    if term.is_empty() || term == "dumb" || term == "linux" {
        return ColorMode::Basic;
    }
    if term.contains("256") {
        return ColorMode::Ansi256;
    }
    if let Some(program) = lookup("TERM_PROGRAM")
        && TRUE_COLOR_PROGRAMS
            .iter()
            .any(|known| program.eq_ignore_ascii_case(known))
    {
        return ColorMode::TrueColor;
    }
    ColorMode::Ansi256
}

#[cfg(test)]
mod terminal_mode_tests {
    use std::collections::HashMap;

    use super::super::{
        begin_synchronized_update, disable_terminal_modes, enable_terminal_modes,
        end_synchronized_update,
    };
    use super::*;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        move |key: &str| map.get(key).cloned()
    }

    fn auto(env: &[(&str, &str)], queried: Option<ImageProtocol>) -> Option<ImageProtocol> {
        choose_image_protocol(ImageProtocolChoice::Auto, queried, false, env_of(env))
    }

    /// Kitty is recognised from the variables kitty itself sets, not only from
    /// `TERM_PROGRAM` — which is absent when the shell that launched it did not set it.
    #[test]
    fn kitty_is_recognised_from_its_own_variables() {
        for env in [
            vec![("TERM", "xterm-kitty"), ("KITTY_WINDOW_ID", "1")],
            vec![("KITTY_PID", "42")],
            vec![("TERM_PROGRAM", "ghostty")],
        ] {
            assert_eq!(auto(&env, None), Some(ImageProtocol::Kitty), "{env:?}");
        }

        assert_eq!(
            auto(&[("TERM", "foot")], None),
            Some(ImageProtocol::Sixel),
            "sixel terminals are the fallback when the query did not answer"
        );
        assert_eq!(
            auto(&[("TERM", "xterm-256color")], None),
            None,
            "half blocks"
        );
    }

    /// What the terminal answers is the only signal that separates a terminal with
    /// graphics from a shell that merely inherited the variables, so it wins over the
    /// environment — including for terminals boxpigma has never heard of.
    #[test]
    fn the_terminals_own_answer_wins_over_the_environment() {
        assert_eq!(
            auto(&[("TERM", "xterm-256color")], Some(ImageProtocol::Kitty)),
            Some(ImageProtocol::Kitty),
            "an unknown terminal that answered the kitty query gets kitty graphics"
        );
        assert_eq!(
            auto(&[("TERM", "xterm-kitty")], Some(ImageProtocol::Sixel)),
            Some(ImageProtocol::Sixel),
            "and a terminal that answered sixel gets sixels"
        );
    }

    /// Upstream's compatibility matrix is explicit that WezTerm, Rio and iTerm2 accept
    /// the kitty graphics query while only their own protocol renders bug-free there,
    /// so the documented mapping has to override the answer they give.
    #[test]
    fn terminals_that_answer_more_than_they_render_are_corrected() {
        for program in ["WezTerm", "rio", "iterm.app"] {
            assert_eq!(
                auto(&[("TERM_PROGRAM", program)], Some(ImageProtocol::Kitty)),
                Some(ImageProtocol::ITerm2),
                "{program}"
            );
        }
    }

    /// Inside tmux, graphics only reach the real terminal when the user enabled
    /// passthrough, and that cannot be probed — so the safe fallback wins, unless the
    /// config forces a protocol.
    #[test]
    fn tmux_falls_back_unless_the_protocol_is_forced() {
        let env = [("TERM", "xterm-kitty"), ("KITTY_WINDOW_ID", "1")];
        assert_eq!(
            choose_image_protocol(
                ImageProtocolChoice::Auto,
                Some(ImageProtocol::Kitty),
                true,
                env_of(&env)
            ),
            None,
            "half blocks inside tmux"
        );
        assert_eq!(
            choose_image_protocol(
                ImageProtocolChoice::Kitty,
                Some(ImageProtocol::Kitty),
                true,
                env_of(&env)
            ),
            Some(ImageProtocol::Kitty),
            "image_protocol = \"kitty\" overrides it (tmux with allow-passthrough)"
        );
    }

    /// The config can force every protocol, including turning graphics off.
    #[test]
    fn the_config_can_force_a_protocol() {
        let env = [("TERM", "xterm-256color")];
        for (choice, expected) in [
            (ImageProtocolChoice::Kitty, Some(ImageProtocol::Kitty)),
            (ImageProtocolChoice::ITerm2, Some(ImageProtocol::ITerm2)),
            (ImageProtocolChoice::Sixel, Some(ImageProtocol::Sixel)),
            (ImageProtocolChoice::Halfblocks, None),
        ] {
            assert_eq!(
                choose_image_protocol(choice, None, false, env_of(&env)),
                expected,
                "{choice:?}"
            );
        }
    }

    /// These bytes are the contract with the terminal, and the pop is the one that
    /// matters most: a terminal left in the keyboard protocol feeds the shell after
    /// boxpigma `CSI u` encodings instead of plain keys.
    #[test]
    fn terminal_modes_are_entered_and_left_with_the_documented_sequences() {
        let mut out = Vec::new();
        enable_terminal_modes(&mut out).expect("enable");
        assert_eq!(
            String::from_utf8(out).expect("utf8"),
            "\u{1b}[?2004h\u{1b}[>1u",
            "bracketed paste on, then keyboard protocol with disambiguation"
        );

        let mut out = Vec::new();
        disable_terminal_modes(&mut out).expect("disable");
        // The spec's pop is `CSI < number u`, number defaulting to 1, so the explicit
        // form is the documented one. It has to be written while still on the
        // alternate screen: the main and alternate screens keep separate stacks.
        assert_eq!(
            String::from_utf8(out).expect("utf8"),
            "\u{1b}[?2004l\u{1b}[<1u",
            "paste off, keyboard protocol popped"
        );
    }

    /// One frame lives between these two, and kitty drops a frame whose end never
    /// arrives — so the loop must always write both.
    #[test]
    fn synchronized_updates_bracket_a_frame() {
        let mut out = Vec::new();
        begin_synchronized_update(&mut out).expect("begin");
        end_synchronized_update(&mut out).expect("end");
        assert_eq!(
            String::from_utf8(out).expect("utf8"),
            "\u{1b}[?2026h\u{1b}[?2026l"
        );
    }
}

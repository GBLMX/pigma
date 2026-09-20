use std::{
    env,
    io::{self, BufRead, BufReader, Write},
    sync::LazyLock,
};

use crossterm::{
    event::{
        DisableBracketedPaste, EnableBracketedPaste, KeyboardEnhancementFlags,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate},
};
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

/// Ask the terminal to raise a desktop notification.
///
/// `OSC 9` is the form every terminal with notifications implements — kitty (where `OSC 99`
/// is the richer one, this is the compatible one), WezTerm, foot, iTerm2, Windows Terminal —
/// and one that has no notifications simply drops the sequence, so nothing needs probing.
///
/// Control characters are stripped first: a song title is remote data, and a stray `BEL` or
/// `ESC` in it would otherwise end the sequence early or start another one.
pub fn notify<W: Write>(out: &mut W, text: &str) -> io::Result<()> {
    let clean: String = text.chars().filter(|c| !c.is_control()).take(240).collect();
    write!(out, "\x1b]9;{clean}\x07")?;
    out.flush()
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

fn is_kitty_terminal(lookup: &impl Fn(&str) -> Option<String>) -> bool {
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

/// Put the terminal into the modes the UI relies on, and take them back out again.
///
/// Both sequences are ignored by terminals that do not implement them, so neither
/// needs a capability probe and neither can break a terminal that lacks them:
///
/// * `CSI > 1 u` — the kitty keyboard protocol, flag 1 (disambiguate escape codes):
///   "pressing the Esc key generates the byte 0x1b which also is used to indicate the
///   start of an escape code", which is how an `Esc` press gets read as `Alt+<key>`
///   when another key follows it. It is implemented by kitty, ghostty, foot, wezterm,
///   alacritty, iTerm2, Windows Terminal and others. Only that one flag is pushed:
///   the rest are for features Pigma does not use.
/// * `CSI ? 2004 h` — bracketed paste, so a paste arrives as one delimited block
///   instead of a burst of keystrokes (see [`crate::input::handle_paste`]).
pub fn enable_terminal_modes<W: Write>(out: &mut W) -> io::Result<()> {
    execute!(
        out,
        EnableBracketedPaste,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    )
}

/// Undo [`enable_terminal_modes`]. Runs on the way out, including after a panic.
pub fn disable_terminal_modes<W: Write>(out: &mut W) -> io::Result<()> {
    execute!(out, DisableBracketedPaste, PopKeyboardEnhancementFlags)
}

/// Open a synchronized update (`DECSET 2026`): the terminal buffers everything
/// drawn until [`end_synchronized_update`] and puts it on screen in one go.
///
/// `ratatui` does not do this itself, and without it a frame is visible while it is
/// being written — most obviously while the spectrum redraws many times a second.
/// Terminals without the mode ignore the sequence; kitty discards a frame whose end
/// never arrives, so every open must be closed.
pub fn begin_synchronized_update<W: Write>(out: &mut W) -> io::Result<()> {
    execute!(out, BeginSynchronizedUpdate)
}

/// Close the update opened by [`begin_synchronized_update`].
pub fn end_synchronized_update<W: Write>(out: &mut W) -> io::Result<()> {
    execute!(out, EndSynchronizedUpdate)
}

/// How many colors the terminal can display; themes are down-sampled to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorMode {
    #[default]
    TrueColor,
    Ansi256,
    Basic,
}

/// Terminal background, used to pick the light or dark theme slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Background {
    #[default]
    Dark,
    Light,
}

/// Config-facing setting: `auto` follows the terminal, the others force a slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackgroundMode {
    #[default]
    Auto,
    Dark,
    Light,
}

impl BackgroundMode {
    /// Apply the setting to what the terminal reported.
    pub fn resolve(self, detected: Background) -> Background {
        match self {
            BackgroundMode::Auto => detected,
            BackgroundMode::Dark => Background::Dark,
            BackgroundMode::Light => Background::Light,
        }
    }
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

/// Nearest xterm-256 index for an RGB color (`16..=255`).
///
/// Grays use the dedicated 24-step ramp — the color cube's gray diagonal is coarse
/// enough to be visible in borders and dim text.
pub fn rgb_to_256(r: u8, g: u8, b: u8) -> u8 {
    if r == g && g == b {
        return match r {
            0..=7 => 16,
            248..=255 => 231,
            _ => 232 + ((u16::from(r) - 8) * 24 / 247) as u8,
        };
    }
    let level = |v: u8| (u16::from(v) * 5 + 127) / 255;
    16 + (36 * level(r) + 6 * level(g) + level(b)) as u8
}

/// Nearest index into the 16 ANSI colors, for terminals without a 256-color palette.
pub fn rgb_to_16(r: u8, g: u8, b: u8) -> u8 {
    const PALETTE: [(u8, u8, u8); 16] = [
        (0, 0, 0),
        (128, 0, 0),
        (0, 128, 0),
        (128, 128, 0),
        (0, 0, 128),
        (128, 0, 128),
        (0, 128, 128),
        (192, 192, 192),
        (128, 128, 128),
        (255, 0, 0),
        (0, 255, 0),
        (255, 255, 0),
        (0, 0, 255),
        (255, 0, 255),
        (0, 255, 255),
        (255, 255, 255),
    ];

    let mut best = 0u8;
    let mut best_distance = u32::MAX;
    for (index, (pr, pg, pb)) in PALETTE.iter().enumerate() {
        let distance = i32::from(r).abs_diff(i32::from(*pr)).pow(2)
            + i32::from(g).abs_diff(i32::from(*pg)).pow(2)
            + i32::from(b).abs_diff(i32::from(*pb)).pow(2);
        if distance < best_distance {
            best_distance = distance;
            best = index as u8;
        }
    }
    best
}

/// Parse `COLORFGBG` (`"<fg>;<bg>"`) into the background lightness.
///
/// The background entry is a 0-15 palette index; indices 8 and above are the bright
/// half, which is the light background.
pub fn parse_colorfgbg(value: &str) -> Option<Background> {
    let background = value.rsplit(';').next()?.trim();
    let index = background.parse::<u8>().ok()?;
    Some(if index >= 8 {
        Background::Light
    } else {
        Background::Dark
    })
}

/// Parse an OSC 11 answer (`ESC ] 11 ; rgb:RRRR/GGGG/BBBB BEL`) into relative luminance.
pub fn parse_osc11_luminance(reply: &str) -> Option<f64> {
    let start = reply.find("rgb:")? + "rgb:".len();
    let mut channels = reply[start..].split('/');
    let mut values = [0.0f64; 3];
    for value in &mut values {
        let channel = channels.next()?;
        let digits: String = channel
            .chars()
            .take_while(char::is_ascii_hexdigit)
            .collect();
        if digits.is_empty() {
            return None;
        }
        let max = u32::from_str_radix(&"f".repeat(digits.len()), 16).ok()?;
        let parsed = u32::from_str_radix(&digits, 16).ok()?;
        *value = f64::from(parsed) / f64::from(max);
    }
    Some(0.2126 * values[0] + 0.7152 * values[1] + 0.0722 * values[2])
}

/// Send the OSC 11 query and read the terminal's answer.
///
/// Generic over the streams so the parsing path is testable; in the running app the
/// caller passes the controlling terminal.
pub fn query_background_luminance<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
) -> Option<f64> {
    write_osc11_query(writer)?;
    read_osc11_reply(reader).and_then(|reply| parse_osc11_luminance(&reply))
}

fn write_osc11_query<W: Write>(writer: &mut W) -> Option<()> {
    writer.write_all(b"\x1b]11;?\x1b\\").ok()?;
    writer.flush().ok()?;
    Some(())
}

/// Read up to the terminator of a reply (`BEL` or `ESC \`), bounded so a terminal that
/// never answers cannot make the caller spin.
fn read_osc11_reply<R: BufRead>(reader: &mut R) -> Option<String> {
    const MAX_REPLY: usize = 64;

    let mut reply = Vec::new();
    let mut byte = [0u8; 1];
    while reply.len() < MAX_REPLY {
        if reader.read_exact(&mut byte).is_err() {
            return None;
        }
        match byte[0] {
            0x07 => return String::from_utf8(reply).ok(),
            0x1b => {
                let mut next = [0u8; 1];
                if reader.read_exact(&mut next).is_err() {
                    return None;
                }
                if next[0] == b'\\' {
                    return String::from_utf8(reply).ok();
                }
                reply.push(0x1b);
                reply.push(next[0]);
            }
            other => reply.push(other),
        }
    }
    None
}

/// Background of the current terminal, detected once.
///
/// `COLORFGBG` covers terminals that export it; otherwise the terminal is asked
/// directly (OSC 11). Callers that run before the UI takes over the terminal should
/// touch this eagerly so the query happens before the event loop starts reading input.
pub static BACKGROUND: LazyLock<Background> = LazyLock::new(detect_background);

fn detect_background() -> Background {
    if let Ok(value) = env::var("COLORFGBG")
        && let Some(background) = parse_colorfgbg(&value)
    {
        return background;
    }

    #[cfg(all(unix, target_os = "linux"))]
    if let Some(luminance) = probe_tty_background() {
        return if luminance > 0.5 {
            Background::Light
        } else {
            Background::Dark
        };
    }

    Background::Dark
}

/// Ask the controlling terminal for its background color with a bounded wait.
///
/// A terminal that does not implement OSC 11 simply never answers, so poll first and
/// give up after a frame instead of blocking startup.
#[cfg(all(unix, target_os = "linux"))]
fn probe_tty_background() -> Option<f64> {
    use std::{fs::OpenOptions, os::fd::AsRawFd};

    const REPLY_TIMEOUT_MS: libc::c_int = 120;

    let tty = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .ok()?;
    let mut writer = tty.try_clone().ok()?;
    write_osc11_query(&mut writer)?;

    let mut poll_fd = libc::pollfd {
        fd: tty.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: `poll_fd` is a valid, initialised pollfd for the duration of the call.
    if unsafe { libc::poll(&mut poll_fd, 1, REPLY_TIMEOUT_MS) } <= 0 {
        return None;
    }

    let mut reader = BufReader::new(tty);
    read_osc11_reply(&mut reader).and_then(|reply| parse_osc11_luminance(&reply))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_from(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        move |key: &str| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    }

    #[test]
    fn color_mode_follows_the_environment() {
        assert_eq!(
            color_mode_from(env_from(&[("COLORTERM", "truecolor"), ("TERM", "xterm")])),
            ColorMode::TrueColor
        );
        assert_eq!(
            color_mode_from(env_from(&[("COLORTERM", "24bit"), ("TERM", "screen")])),
            ColorMode::TrueColor
        );
        // Windows Terminal only advertises itself
        assert_eq!(
            color_mode_from(env_from(&[("WT_SESSION", "1"), ("TERM", "xterm")])),
            ColorMode::TrueColor
        );
        assert_eq!(
            color_mode_from(env_from(&[("TERM", "xterm-256color")])),
            ColorMode::Ansi256
        );
        assert_eq!(
            color_mode_from(env_from(&[("TERM", "linux")])),
            ColorMode::Basic
        );
        assert_eq!(
            color_mode_from(env_from(&[("TERM", "dumb")])),
            ColorMode::Basic
        );
        // an ssh session that lost COLORTERM
        assert_eq!(
            color_mode_from(env_from(&[("TERM", "xterm"), ("TERM_PROGRAM", "kitty")])),
            ColorMode::TrueColor
        );
        // unknown legacy terminal: assume the modern default, never the 16-color one
        assert_eq!(
            color_mode_from(env_from(&[("TERM", "vt100")])),
            ColorMode::Ansi256
        );
    }

    #[test]
    fn rgb_to_256_matches_the_xterm_palette() {
        assert_eq!(rgb_to_256(0, 0, 0), 16);
        assert_eq!(rgb_to_256(255, 255, 255), 231);
        assert_eq!(rgb_to_256(255, 0, 0), 196);
        assert_eq!(rgb_to_256(0, 255, 0), 46);
        assert_eq!(rgb_to_256(0, 0, 255), 21);
        // grays take the ramp, not the cube
        assert!(rgb_to_256(128, 128, 128) >= 232);
    }

    #[test]
    fn rgb_to_16_picks_the_nearest_ansi_color() {
        assert_eq!(rgb_to_16(0, 0, 0), 0);
        assert_eq!(rgb_to_16(255, 255, 255), 15);
        assert_eq!(rgb_to_16(255, 0, 0), 9);
        assert_eq!(rgb_to_16(8, 8, 8), 0);
        assert_eq!(rgb_to_16(250, 250, 250), 15);
    }

    #[test]
    fn colorfgbg_selects_the_background() {
        assert_eq!(parse_colorfgbg("15;0"), Some(Background::Dark));
        assert_eq!(parse_colorfgbg("0;15"), Some(Background::Light));
        assert_eq!(parse_colorfgbg("15;8"), Some(Background::Light));
        assert_eq!(parse_colorfgbg("15;default"), None);
        assert_eq!(parse_colorfgbg(""), None);
        assert_eq!(parse_colorfgbg("garbage"), None);
    }

    #[test]
    fn osc11_replies_map_to_luminance() {
        let dark = parse_osc11_luminance("\x1b]11;rgb:1e1e/1e1e/1e1e\x07").unwrap();
        assert!(dark < 0.2, "dark background luminance was {dark}");
        let light = parse_osc11_luminance("\x1b]11;rgb:ffff/ffff/ffff\x07").unwrap();
        assert!(light > 0.9, "light background luminance was {light}");
        // short form, and the ESC \ terminator
        assert_eq!(
            parse_osc11_luminance("\x1b]11;rgb:00/00/00\x1b\\"),
            Some(0.0)
        );
        assert_eq!(parse_osc11_luminance("\x1b]11;rgb:zz/00/00\x07"), None);
        assert_eq!(parse_osc11_luminance("no reply"), None);
    }

    #[test]
    fn query_round_trip_reads_the_bel_terminated_reply() {
        let mut reply = std::io::Cursor::new(b"\x1b]11;rgb:ffff/ffff/ffff\x07".to_vec());
        let mut sent = Vec::new();
        let luminance = query_background_luminance(&mut reply, &mut sent).unwrap();
        assert_eq!(sent, b"\x1b]11;?\x1b\\");
        assert!(luminance > 0.9);
    }

    #[test]
    fn query_gives_up_on_a_terminal_that_never_answers() {
        let mut silent = std::io::Cursor::new(Vec::new());
        let mut sent = Vec::new();
        assert_eq!(query_background_luminance(&mut silent, &mut sent), None);
    }
}

#[cfg(test)]
mod terminal_mode_tests {
    use std::collections::HashMap;

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
    /// environment — including for terminals Pigma has never heard of.
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
    /// Pigma `CSI u` encodings instead of plain keys.
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

#[cfg(test)]
mod notification_tests {
    use super::*;

    /// `OSC 9` is the form the terminals implement, and the payload is remote data: a `BEL`
    /// or `ESC` in a song title would otherwise end the sequence early or start another one.
    #[test]
    fn a_notification_is_one_clean_osc_9() {
        let mut out = Vec::new();
        notify(&mut out, "歌名 — 歌手").expect("notify");
        assert_eq!(
            String::from_utf8(out).expect("utf8"),
            "\u{1b}]9;歌名 — 歌手\u{7}"
        );
    }

    #[test]
    fn control_characters_cannot_escape_the_notification() {
        let mut out = Vec::new();
        notify(&mut out, "a\u{7}b\u{1b}]9;evil").expect("notify");
        let text = String::from_utf8(out).expect("utf8");
        assert_eq!(text, "\u{1b}]9;ab]9;evil\u{7}");
        assert_eq!(text.matches('\u{7}').count(), 1, "exactly one terminator");
    }

    /// The config's choices have to land on distinct terminal shapes.
    #[test]
    fn cursor_styles_are_distinct() {
        let shapes: Vec<String> = [
            CursorStyle::Default,
            CursorStyle::Block,
            CursorStyle::Underline,
            CursorStyle::Bar,
        ]
        .iter()
        .map(|style| format!("{:?}", style.command()))
        .collect();

        let mut unique = shapes.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), shapes.len(), "{shapes:?}");
    }
}

use std::{
    env,
    io::{self, BufRead, Write},
    sync::LazyLock,
};

use crossterm::{
    execute,
    terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate},
};
use ratatui::style::Color;
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

/// Ask the terminal to raise a desktop notification.
///
/// Two forms, both out-of-band — they change no cells, so they cannot disturb the frame
/// being drawn around them:
///
/// * `OSC 99` — kitty's own, and the richer one. It carries a title and a body as separate
///   payloads, names the application (`f=`, which is what a user filters by), and takes the
///   payloads Base64-encoded (`e=1`). Base64 is why a song title containing `;`, `BEL` or
///   `ESC` — a title is remote data — arrives intact instead of being stripped.
/// * `OSC 9` — the iTerm2 form, which every terminal with notifications implements (kitty,
///   WezTerm, foot, Windows Terminal among them). It carries one plain-text string, so
///   control characters are stripped and the length is capped; a terminal without
///   notifications drops the sequence, so nothing needs probing.
///
/// kitty is recognised from the environment, not from a query: this sequence has no reply,
/// so probing would mean waiting for a timeout on every start.
pub fn notify<W: Write>(out: &mut W, title: &str, body: &str) -> io::Result<()> {
    let kitty = is_kitty_terminal(&|key: &str| std::env::var(key).ok());
    notify_as(out, title, body, kitty)
}

/// The payload limit kitty documents is 2048 bytes before encoding. A song line is far
/// shorter, but the text comes from the network, so it is clipped rather than trusted.
const OSC99_MAX_PAYLOAD: usize = 2048;

/// [`notify`] with the terminal's identity passed in instead of probed.
fn notify_as<W: Write>(out: &mut W, title: &str, body: &str, kitty: bool) -> io::Result<()> {
    if kitty {
        return notify_osc99(out, title, body);
    }

    let text = if title.is_empty() {
        body.to_string()
    } else {
        format!("{title} — {body}")
    };
    let clean: String = text.chars().filter(|c| !c.is_control()).take(240).collect();
    write!(out, "\x1b]9;{clean}\x07")?;
    out.flush()
}

fn notify_osc99<W: Write>(out: &mut W, title: &str, body: &str) -> io::Result<()> {
    use base64::Engine as _;

    let encode = |text: &str| {
        let mut used = 0usize;
        let clipped: String = text
            .chars()
            .take_while(|c| {
                used += c.len_utf8();
                used <= OSC99_MAX_PAYLOAD
            })
            .collect();
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(clipped.as_bytes())
    };

    // `i=` is the notification id, so a later notification replaces the previous one instead
    // of stacking up. `d=0` holds the notification back until the body arrives with `d=1`,
    // which is what makes title and body one notification rather than two.
    let app = base64::engine::general_purpose::STANDARD_NO_PAD.encode(b"boxpigma");
    const ID: &str = "boxpigma";

    if !title.is_empty() {
        write!(
            out,
            "\x1b]99;i={ID}:f={app}:e=1:d=0;{}\x1b\\",
            encode(title)
        )?;
    }
    write!(
        out,
        "\x1b]99;i={ID}:f={app}:p=body:e=1:d=1;{}\x1b\\",
        encode(body)
    )?;
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

/// `CSI ? 2004 h` / `CSI ? 2004 l` — bracketed paste on and off.
const BRACKETED_PASTE_ON: &[u8] = b"\x1b[?2004h";
const BRACKETED_PASTE_OFF: &[u8] = b"\x1b[?2004l";
/// `CSI > 1 u` pushes kitty keyboard flag 1; `CSI < 1 u` pops it again.
const PUSH_DISAMBIGUATE_ESCAPE_CODES: &[u8] = b"\x1b[>1u";
const POP_KEYBOARD_ENHANCEMENT_FLAGS: &[u8] = b"\x1b[<1u";

/// Put the terminal into the modes the UI relies on, and take them back out again.
///
/// The sequences are written directly rather than through `crossterm`'s command types,
/// for one reason that matters on Windows: there those commands go through the legacy
/// console API instead of writing bytes, and with no console attached they do not
/// degrade — `PushKeyboardEnhancementFlags` returns
/// "Keyboard progressive enhancement not implemented for the legacy Windows API". The
/// rest of the frame already talks to the terminal this way (`OSC 11`, `OSC 99`,
/// `DECSET 2026`), so this is the same mouth speaking, and the bytes are exactly what
/// the Unix implementation emitted before.
///
/// Neither sequence is probed for: a terminal that does not implement one ignores it,
/// so there is nothing to break.
///
/// * `CSI ? 2004 h` — bracketed paste, so a paste arrives as one delimited block
///   instead of a burst of keystrokes (see [`crate::input::handle_paste`]).
/// * `CSI > 1 u` — the kitty keyboard protocol, flag 1 (disambiguate escape codes):
///   "pressing the Esc key generates the byte 0x1b which also is used to indicate the
///   start of an escape code", which is how an `Esc` press gets read as `Alt+<key>`
///   when another key follows it. Kitty, ghostty, foot, wezterm, alacritty and iTerm2
///   implement it; a terminal that does not simply ignores the sequence and `Esc` keeps
///   the ambiguity it always had. Only that one flag is pushed: the rest are for
///   features boxpigma does not use.
pub fn enable_terminal_modes<W: Write>(out: &mut W) -> io::Result<()> {
    out.write_all(BRACKETED_PASTE_ON)?;
    out.write_all(PUSH_DISAMBIGUATE_ESCAPE_CODES)?;
    out.flush()
}

/// Undo [`enable_terminal_modes`]. Runs on the way out, including after a panic.
///
/// `CSI < 1 u` and not the defaulted `CSI < u`: the explicit form is the documented one,
/// and it has to be written while still on the alternate screen, since the main and
/// alternate screens keep separate stacks.
pub fn disable_terminal_modes<W: Write>(out: &mut W) -> io::Result<()> {
    out.write_all(BRACKETED_PASTE_OFF)?;
    out.write_all(POP_KEYBOARD_ENHANCEMENT_FLAGS)?;
    out.flush()
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

/// Whether the app paints its own background over the terminal's.
///
/// The app paints the whole frame with the theme's background so that a theme reads as a theme:
/// without it, every cell the theme does not explicitly paint keeps the terminal's colours, and
/// a light theme in a dark terminal becomes thin light-grey text on a dark screen. What that
/// fill also covers is the terminal's own background — a translucent one, Windows Terminal's
/// acrylic — which is a thing the user *can* see, unlike a fill that matches the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackgroundFill {
    /// Paint it only when the theme's background and the terminal's disagree — the case the fill
    /// exists for. When they agree the fill is invisible while the transparency it hides is not.
    #[default]
    Auto,
    /// Always paint it.
    Always,
    /// Never paint it: the terminal's background, and its blur, show through everywhere the
    /// theme does not deliberately paint a surface. On a terminal whose background does not
    /// match the theme, text is then as readable as the user's choice makes it.
    Never,
}

impl BackgroundFill {
    /// Whether the frame should be filled, given the theme's background and the terminal's.
    pub fn paints(self, theme: Background, terminal: Background) -> bool {
        match self {
            Self::Always => true,
            Self::Never => false,
            Self::Auto => theme != terminal,
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

/// The sixteen ANSI colors, in palette order. Terminals disagree about the exact values; these
/// are the ones [`rgb_to_16`] maps to, so the two directions agree with each other.
pub(crate) const ANSI_16: [(u8, u8, u8); 16] = [
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

/// The xterm 256-color palette's value for an index: the sixteen ANSI colors, then a 6×6×6
/// cube, then a 24-step grey ramp with a 10-per-step gap from 8.
///
/// The inverse of [`rgb_to_256`] in the direction a classifier needs: given the index a theme
/// was down-sampled to, what colour is the screen actually showing.
pub(crate) fn palette_rgb(index: u8) -> (u8, u8, u8) {
    const CUBE: [u8; 6] = [0, 95, 135, 175, 215, 255];
    match index {
        0..=15 => ANSI_16[usize::from(index)],
        16..=231 => {
            let i = u16::from(index) - 16;
            (
                CUBE[(i / 36) as usize],
                CUBE[((i % 36) / 6) as usize],
                CUBE[(i % 6) as usize],
            )
        }
        // 232..=255: the grey ramp.
        _ => {
            let level = 8 + 10 * (u16::from(index) - 232);
            (level as u8, level as u8, level as u8)
        }
    }
}

/// What a colour says about the screen it is drawn on, when it says anything.
///
/// `Rgb` is exact. A palette index is looked up in the table it comes from — a theme that was
/// down-sampled for a 256-color terminal is still a light theme, and this is what keeps it from
/// being classified as a dark one. The sixteen names are the ANSI colors they stand for.
/// `Reset` is whatever the terminal already had on screen, so it has no answer of its own; the
/// caller's fallback is then the same assumption [`Background::default`] makes.
pub(crate) fn color_luminance(color: Color) -> Option<f64> {
    let (r, g, b) = match color {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Indexed(index) => palette_rgb(index),
        Color::Black => ANSI_16[0],
        Color::Red => ANSI_16[1],
        Color::Green => ANSI_16[2],
        Color::Yellow => ANSI_16[3],
        Color::Blue => ANSI_16[4],
        Color::Magenta => ANSI_16[5],
        Color::Cyan => ANSI_16[6],
        Color::Gray => ANSI_16[7],
        Color::DarkGray => ANSI_16[8],
        Color::LightRed => ANSI_16[9],
        Color::LightGreen => ANSI_16[10],
        Color::LightYellow => ANSI_16[11],
        Color::LightBlue => ANSI_16[12],
        Color::LightMagenta => ANSI_16[13],
        Color::LightCyan => ANSI_16[14],
        Color::White => ANSI_16[15],
        Color::Reset => return None,
    };
    Some(rgb_luminance(
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
    ))
}

/// Nearest index into the 16 ANSI colors, for terminals without a 256-color palette.
pub fn rgb_to_16(r: u8, g: u8, b: u8) -> u8 {
    let mut best = 0u8;
    let mut best_distance = u32::MAX;
    for (index, (pr, pg, pb)) in ANSI_16.iter().enumerate() {
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
    Some(rgb_luminance(values[0], values[1], values[2]))
}

/// WCAG relative luminance of an sRGB colour with channels in `0.0..=1.0`.
///
/// One implementation for the two ways a background arrives — the terminal's OSC 11 answer
/// and, on Windows, the console's colour table.
pub(crate) fn rgb_luminance(r: f64, g: f64, b: f64) -> f64 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// What a luminance says about the background.
pub(crate) fn background_from_luminance(luminance: f64) -> Background {
    if luminance > 0.5 {
        Background::Light
    } else {
        Background::Dark
    }
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

    #[cfg(unix)]
    if let Some(luminance) = probe_tty_background() {
        return background_from_luminance(luminance);
    }

    #[cfg(windows)]
    if let Some(luminance) = probe_console_background() {
        return background_from_luminance(luminance);
    }

    Background::Dark
}

/// Ask the Windows console for its background colour.
///
/// There is no `/dev/tty` and no OSC 11 answer to wait for (ConPTY is in the way), but the
/// console API knows the answer: the screen buffer's attributes carry the background palette
/// index and `ColorTable` carries the palette, which Windows Terminal fills in from the color
/// scheme the tab was started with. Best effort — a process without a console fails the call
/// and the caller keeps the dark default.
#[cfg(windows)]
fn probe_console_background() -> Option<f64> {
    use windows_sys::Win32::System::Console::{
        CONSOLE_SCREEN_BUFFER_INFOEX, GetConsoleScreenBufferInfoEx, GetStdHandle, STD_OUTPUT_HANDLE,
    };

    let mut info = CONSOLE_SCREEN_BUFFER_INFOEX {
        cbSize: std::mem::size_of::<CONSOLE_SCREEN_BUFFER_INFOEX>() as u32,
        ..Default::default()
    };
    // SAFETY: `info` is an initialised CONSOLE_SCREEN_BUFFER_INFOEX whose `cbSize` the API
    // requires; the handle is stdout, and the call reports failure instead of faulting when
    // that is not a console.
    if unsafe { GetConsoleScreenBufferInfoEx(GetStdHandle(STD_OUTPUT_HANDLE), &mut info) } == 0 {
        return None;
    }

    // The low nibble of the attributes is the background: 0-15, an index into `ColorTable`.
    let index = usize::from(info.wAttributes & 0x000F);
    let color = *info.ColorTable.get(index)?;
    Some(console_color_luminance(color))
}

/// `COLORREF` is `0x00BBGGRR` — green in the middle, which is the opposite of the web order.
#[cfg(windows)]
fn console_color_luminance(color: u32) -> f64 {
    let channel = |shift: u32| f64::from((color >> shift) & 0xFF) / 255.0;
    rgb_luminance(channel(0), channel(8), channel(16))
}

/// Ask the controlling terminal for its background color with a bounded wait.
///
/// A terminal that does not implement OSC 11 simply never answers, so poll first and
/// give up after a frame instead of blocking startup.
#[cfg(unix)]
fn probe_tty_background() -> Option<f64> {
    use std::fs::OpenOptions;

    let tty = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .ok()?;
    query_background_on_tty(&tty)
}

/// The probe itself, against an already-open tty.
///
/// Split out so a test can hand it a pty whose other end plays the terminal
/// (`the_background_probe_reads_a_reply_without_a_newline`).
///
/// The reply — `ESC ] 11 ; rgb:… ESC \` — carries **no newline**, so in the canonical mode
/// a tty starts in, the line discipline holds it back and the poll below times out on
/// every terminal that does answer. The probe therefore reads the tty the way the event
/// loop later will: non-canonical, no echo, byte-at-a-time, under one overall deadline.
/// Measured on a pty: canonical never delivers the reply, non-canonical delivers it at once.
#[cfg(unix)]
fn query_background_on_tty(tty: &std::fs::File) -> Option<f64> {
    use std::{io::BufReader, os::fd::AsRawFd};

    let _no_stop = NoTtyStopSignals::block();

    let fd = tty.as_raw_fd();
    let _raw = RawTty::new(fd);

    let mut writer = tty.try_clone().ok()?;
    write_osc11_query(&mut writer)?;

    let mut reader = BufReader::new(TtyReaderWithDeadline::new(fd, REPLY_BUDGET));
    read_osc11_reply(&mut reader).and_then(|reply| parse_osc11_luminance(&reply))
}

/// Blocks `SIGTTIN`/`SIGTTOU` for the calling thread, and restores the mask on drop.
///
/// Both are raised by exactly the two things this probe does to a terminal it is not the
/// foreground process group of — reading from it, and changing its modes — and the default
/// action of both is to **stop** the process. A stopped process is a hang with no error,
/// which is the worst outcome a startup probe can produce: it is what stopped the whole test
/// suite the first time this probe ran against a pty it did not own.
///
/// POSIX lets the read and the `tcsetattr` through when the signal is blocked (or ignored) in
/// the calling thread, so blocking is enough — and unlike a process-wide `SIG_IGN` it cannot
/// race with another thread's handler.
#[cfg(unix)]
struct NoTtyStopSignals {
    previous: libc::sigset_t,
}

#[cfg(unix)]
impl NoTtyStopSignals {
    fn block() -> Option<Self> {
        let mut set = std::mem::MaybeUninit::<libc::sigset_t>::uninit();
        let mut previous = std::mem::MaybeUninit::<libc::sigset_t>::uninit();
        // SAFETY: both are valid, writable `sigset_t`s; `sigemptyset`/`sigaddset` initialise
        // `set` before it is read, and `pthread_sigmask` writes `previous` when it returns 0.
        unsafe {
            if libc::sigemptyset(set.as_mut_ptr()) != 0 {
                return None;
            }
            let mut set = set.assume_init();
            if libc::sigaddset(&mut set, libc::SIGTTIN) != 0
                || libc::sigaddset(&mut set, libc::SIGTTOU) != 0
            {
                return None;
            }
            if libc::pthread_sigmask(libc::SIG_BLOCK, &set, previous.as_mut_ptr()) != 0 {
                return None;
            }
            Some(Self {
                previous: previous.assume_init(),
            })
        }
    }
}

#[cfg(unix)]
impl Drop for NoTtyStopSignals {
    fn drop(&mut self) {
        // SAFETY: `self.previous` is the mask `pthread_sigmask` filled in on creation.
        unsafe {
            libc::pthread_sigmask(libc::SIG_SETMASK, &self.previous, std::ptr::null_mut());
        }
    }
}

/// Overall budget for reading the terminal's answer to the OSC 11 query.
#[cfg(unix)]
const REPLY_BUDGET: std::time::Duration = std::time::Duration::from_millis(200);

/// A tty reader that gives up once `deadline` passes.
///
/// `read_osc11_reply` reads byte by byte, so the wait has to be bounded per byte *and* in
/// total: the poll before each read waits for the time that is left, and once nothing is
/// left the read fails — which the reply parser turns into "no answer".
#[cfg(unix)]
struct TtyReaderWithDeadline {
    fd: std::os::fd::RawFd,
    deadline: std::time::Instant,
}

#[cfg(unix)]
impl TtyReaderWithDeadline {
    fn new(fd: std::os::fd::RawFd, budget: std::time::Duration) -> Self {
        Self {
            fd,
            deadline: std::time::Instant::now() + budget,
        }
    }
}

#[cfg(unix)]
impl std::io::Read for TtyReaderWithDeadline {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let remaining = self
            .deadline
            .saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "no OSC 11 reply within the budget",
            ));
        }

        let mut poll_fd = libc::pollfd {
            fd: self.fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: `poll_fd` is a valid, initialised pollfd for the duration of the call.
        let ready = unsafe { libc::poll(&mut poll_fd, 1, remaining.as_millis() as libc::c_int) };
        if ready <= 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "no OSC 11 reply within the budget",
            ));
        }

        // SAFETY: `buf` is a valid, writable slice of `buf.len()` bytes and `self.fd` is an
        // open tty for as long as the caller holds it.
        let count = unsafe { libc::read(self.fd, buf.as_mut_ptr().cast(), buf.len()) };
        if count < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(count as usize)
    }
}

/// Put the controlling terminal into the mode the probe needs, and put it back on drop.
///
/// Only canonical input buffering and echo change: the probe runs before the UI takes the
/// terminal over, so everything else is left exactly as the user had it, and the previous
/// settings are restored even when the probe gives up early.
#[cfg(unix)]
struct RawTty {
    fd: std::os::fd::RawFd,
    saved: libc::termios,
}

#[cfg(unix)]
impl RawTty {
    fn new(fd: std::os::fd::RawFd) -> Option<Self> {
        let mut saved = std::mem::MaybeUninit::<libc::termios>::uninit();
        // SAFETY: `saved` is a valid pointer to writable memory for one `termios`.
        if unsafe { libc::tcgetattr(fd, saved.as_mut_ptr()) } != 0 {
            return None;
        }
        // SAFETY: `tcgetattr` returned 0, so it initialised `saved`.
        let saved = unsafe { saved.assume_init() };

        let mut raw = saved;
        raw.c_lflag &= !(libc::ICANON | libc::ECHO);
        raw.c_cc[libc::VMIN] = 1;
        raw.c_cc[libc::VTIME] = 0;
        // SAFETY: `fd` is an open tty and `raw` is an initialised `termios`.
        if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 {
            return None;
        }

        Some(Self { fd, saved })
    }
}

#[cfg(unix)]
impl Drop for RawTty {
    fn drop(&mut self) {
        // SAFETY: `self.fd` is the tty this guard was created for and `self.saved` is the
        // `termios` it had at that moment.
        unsafe { libc::tcsetattr(self.fd, libc::TCSANOW, &self.saved) };
    }
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

    /// Down-sampling a colour for a palette terminal must not change what it *means*: this is
    /// what keeps a light theme reading as light in a 256-colour terminal (CI caught the
    /// absence of it, where the terminal reports no true-colour support).
    #[test]
    fn a_palette_colour_still_says_what_the_screen_shows() {
        assert_eq!(palette_rgb(16), (0, 0, 0), "the cube's corner");
        assert_eq!(palette_rgb(231), (255, 255, 255), "the cube's other corner");
        assert_eq!(palette_rgb(255), (238, 238, 238), "the grey ramp's end");
        assert_eq!(palette_rgb(15), ANSI_16[15], "the ANSI colours come first");

        let classify = |color| {
            background_from_luminance(color_luminance(color).expect("a measurable colour"))
        };
        assert_eq!(classify(Color::Indexed(231)), Background::Light);
        assert_eq!(classify(Color::Indexed(232)), Background::Dark);
        assert_eq!(classify(Color::White), Background::Light);
        assert_eq!(color_luminance(Color::Reset), None);

        // The contract that matters: whatever `rgb_to_256` picks for a colour classifies the
        // way the colour itself does.
        for (r, g, b) in [(255, 255, 255), (250, 250, 250), (0, 0, 0), (20, 20, 20)] {
            assert_eq!(
                classify(Color::Indexed(rgb_to_256(r, g, b))),
                classify(Color::Rgb(r, g, b)),
                "({r},{g},{b}) must survive the trip through the palette"
            );
        }
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
    fn background_thresholds_at_half_luminance() {
        assert_eq!(background_from_luminance(0.0), Background::Dark);
        assert_eq!(background_from_luminance(0.5), Background::Dark);
        assert_eq!(background_from_luminance(0.51), Background::Light);
        assert_eq!(background_from_luminance(1.0), Background::Light);
    }

    /// `COLORREF` packs `0x00BBGGRR`, so the byte order has to be read backwards: blue is the
    /// high byte and the darkest of the three primaries by weight, which is what makes the
    /// usual `#282c34`-style background come out dark instead of bright red.
    #[cfg(windows)]
    #[test]
    fn a_console_colour_is_read_as_bgr() {
        assert!(console_color_luminance(0x0000_0000) < 0.01, "black");
        assert!(console_color_luminance(0x00FF_FFFF) > 0.99, "white");
        let blue = console_color_luminance(0x00FF_0000);
        let green = console_color_luminance(0x0000_FF00);
        let red = console_color_luminance(0x0000_00FF);
        assert!(blue < red && red < green, "{blue} {red} {green}");
        assert_eq!(background_from_luminance(blue), Background::Dark);
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

    /// The reply to OSC 11 has no newline, so a tty in canonical mode withholds it: the
    /// poll always timed out and `BACKGROUND` fell back to "dark" on every terminal that
    /// does answer (kitty does not export `COLORFGBG`). The probe now reads the tty in the
    /// mode the event loop later uses, and this test is what pins that: a pty plays the
    /// terminal, the probe runs against the pty's other end.
    ///
    /// Linux only, deliberately. What the test asserts is a *kernel* behaviour — a
    /// canonical-mode read does not see a reply that carries no newline — and that is what
    /// was measured and reproduced here. The macOS pty layer is a different implementation
    /// of the same idea and does not exist on the machine this was written on; the CI macOS
    /// job is what first said so. The *code* under test is still compiled on every Unix: if
    /// this is ever run on a Mac, expect to adjust the pty setup, not the probe.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_background_probe_reads_a_reply_without_a_newline() {
        use std::os::fd::FromRawFd;

        let mut master: libc::c_int = 0;
        let mut slave: libc::c_int = 0;
        // SAFETY: both out-parameters point to valid, writable `c_int`s; the optional
        // name/termios/winsize arguments are genuinely optional and passed as null.
        let created = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        assert_eq!(
            created,
            0,
            "openpty failed: {}",
            std::io::Error::last_os_error()
        );

        let terminal = std::thread::spawn(move || {
            let mut query = [0u8; 64];
            // SAFETY: `query` is a valid, writable buffer of `query.len()` bytes.
            let read = unsafe { libc::read(master, query.as_mut_ptr().cast(), query.len()) };
            assert!(read > 0, "the probe sent no query");
            assert!(
                query[..read as usize].starts_with(b"\x1b]11;?"),
                "unexpected query: {:?}",
                &query[..read as usize]
            );

            let reply = b"\x1b]11;rgb:ffff/ffff/ffff\x1b\\";
            // SAFETY: `reply` is a valid, readable buffer of `reply.len()` bytes.
            let written = unsafe { libc::write(master, reply.as_ptr().cast(), reply.len()) };
            assert_eq!(written, reply.len() as isize, "the reply was not delivered");
        });

        // SAFETY: `slave` came from `openpty` above and is not owned by anything else.
        let tty = unsafe { std::fs::File::from_raw_fd(slave) };
        let luminance = query_background_on_tty(&tty).expect("the white reply must be parsed");
        terminal.join().expect("the terminal thread");

        assert!(
            luminance > 0.9,
            "a white background reads as light: {luminance}"
        );
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

#[cfg(test)]
mod notification_tests {
    use super::*;

    /// `OSC 9` is the form the terminals implement, and the payload is remote data: a `BEL`
    /// or `ESC` in a song title would otherwise end the sequence early or start another one.
    /// Title and body are joined with a dash because this form carries a single string.
    #[test]
    fn a_notification_is_one_clean_osc_9() {
        let mut out = Vec::new();
        notify_as(&mut out, "歌名", "歌手", false).expect("notify");
        assert_eq!(
            String::from_utf8(out).expect("utf8"),
            "\u{1b}]9;歌名 — 歌手\u{7}"
        );
    }

    #[test]
    fn control_characters_cannot_escape_the_notification() {
        let mut out = Vec::new();
        notify_as(&mut out, "", "a\u{7}b\u{1b}]9;evil", false).expect("notify");
        let text = String::from_utf8(out).expect("utf8");
        assert_eq!(text, "\u{1b}]9;ab]9;evil\u{7}");
        assert_eq!(text.matches('\u{7}').count(), 1, "exactly one terminator");
    }

    /// kitty's own form keeps the halves apart, and Base64 is what lets a title containing
    /// `;` or `ESC` through instead of being filtered away.
    #[test]
    fn the_kitty_form_keeps_title_and_body_apart() {
        let mut out = Vec::new();
        notify_as(&mut out, "a;b\u{1b}c", "artist", true).expect("notify");
        let text = String::from_utf8(out).expect("utf8");

        let payloads = osc99_payloads(&text);
        assert_eq!(payloads.len(), 2, "a title payload and a body payload");
        assert_eq!(decode(&payloads[0]), "a;b\u{1b}c");
        assert_eq!(decode(&payloads[1]), "artist");

        assert!(text.contains("p=body"), "the body says what it is");
        assert!(text.contains("f="), "the application names itself");
        // `d=0` holds the notification back until `d=1` arrives: the other order is two
        // notifications rather than one.
        assert!(text.find("d=0").expect("d=0") < text.find("d=1").expect("d=1"));
    }

    /// A notification with nothing to put in the title is one sequence, not an empty first.
    #[test]
    fn a_body_only_notification_has_no_title_sequence() {
        let mut out = Vec::new();
        notify_as(&mut out, "", "播放失败", true).expect("notify");
        let text = String::from_utf8(out).expect("utf8");

        assert_eq!(osc99_payloads(&text).len(), 1);
        assert_eq!(text.matches("\u{1b}]99;").count(), 1);
        assert!(!text.contains("d=0"));
    }

    /// The clip is counted in bytes — a CJK title is three bytes a character, so a character
    /// count would overshoot kitty's documented 2048 — and it must not split a character.
    #[test]
    fn the_payload_stops_at_kitty_s_byte_limit() {
        let long = "歌".repeat(2000);
        let mut out = Vec::new();
        notify_as(&mut out, &long, "body", true).expect("notify");
        let text = String::from_utf8(out).expect("utf8");

        let title = decode(&osc99_payloads(&text)[0]);
        assert!(title.len() <= OSC99_MAX_PAYLOAD, "{}", title.len());
        assert_eq!(title.chars().count(), 682, "2048 / 3, rounded down");
        assert!(
            title.chars().all(|c| c == '歌'),
            "no broken character at the cut"
        );
    }

    fn osc99_payloads(text: &str) -> Vec<String> {
        text.split("\u{1b}\\")
            .filter(|sequence| sequence.starts_with("\u{1b}]99;"))
            .map(|sequence| {
                sequence
                    .rsplit(';')
                    .next()
                    .expect("every sequence has a payload")
                    .to_string()
            })
            .collect()
    }

    fn decode(payload: &str) -> String {
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD_NO_PAD
            .decode(payload)
            .expect("base64 payload");
        String::from_utf8(bytes).expect("utf8 payload")
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

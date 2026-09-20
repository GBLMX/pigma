use std::env;
use std::io::{BufRead, BufReader, Write};
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageProtocol {
    Kitty,
    Sixel,
}

pub fn best_image_protocol() -> Option<ImageProtocol> {
    if kitty_available() {
        Some(ImageProtocol::Kitty)
    } else if sixel_available() {
        Some(ImageProtocol::Sixel)
    } else {
        None
    }
}

fn kitty_available() -> bool {
    if env::var("KITTY_WINDOW_ID").is_ok()
        || env::var("KITTY_PID").is_ok()
        || env::var("GHOSTTY_RESOURCES_DIR").is_ok()
    {
        return true;
    }

    match env::var("TERM_PROGRAM").as_deref() {
        Ok("kitty" | "ghostty" | "rio" | "WezTerm") => return true,
        Ok("iterm.app") => {
            if version_gte(
                &env::var("TERM_PROGRAM_VERSION").unwrap_or_default(),
                3,
                5,
                0,
            ) {
                return true;
            }
        }
        Ok("konsole") if is_konsole_version_gte(22, 4, 0) => {
            return true;
        }
        _ => {}
    }

    matches!(
        env::var("TERM").as_deref(),
        Ok(t) if t.to_lowercase().contains("kitty") || t == "xterm-ghostty"
    )
}

fn sixel_available() -> bool {
    if env::var("FOOT_VERSION").is_ok() {
        return true;
    }

    if env::var("WT_SESSION").is_ok() {
        return true;
    }

    match env::var("TERM_PROGRAM").as_deref() {
        Ok("vscode") => {
            if version_gte(
                &env::var("TERM_PROGRAM_VERSION").unwrap_or_default(),
                1,
                80,
                0,
            ) {
                return true;
            }
        }
        Ok("rio") => {
            // Rio started supporting the graphics protocol reasonably well after 0.0.12
            if version_gte(
                &env::var("TERM_PROGRAM_VERSION").unwrap_or_default(),
                0,
                0,
                12,
            ) {
                return true;
            }
        }
        Ok("mintty") => return true,
        Ok("WezTerm") => {
            if wezterm_sixel_supported(&env::var("WEZTERM_VERSION").unwrap_or_default()) {
                return true;
            }
        }
        Ok("konsole") => {
            if is_konsole_version_gte(22, 4, 0) {
                return true;
            }
        }
        Ok("WindowsTerminal" | "Windows_Terminal")
            if version_gte(
                &env::var("TERM_PROGRAM_VERSION").unwrap_or_default(),
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
        env::var("TERM").as_deref(),
        Ok(t) if t.to_lowercase().starts_with("foot") || t.to_lowercase().starts_with("mlterm")
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

fn is_konsole_version_gte(major: u32, minor: u32, patch: u32) -> bool {
    let ver_str = env::var("KONSOLE_VERSION").unwrap_or_default();
    if ver_str.contains('.') {
        version_gte(&ver_str, major, minor, patch)
    } else if let Ok(num) = ver_str.parse::<u32>() {
        let target = major * 10000 + minor * 100 + patch;
        num >= target
    } else {
        false
    }
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
    use std::fs::OpenOptions;
    use std::os::fd::AsRawFd;

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

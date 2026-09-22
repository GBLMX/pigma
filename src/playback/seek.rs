//! The seek-value grammar shared by the `:seek` command and `boxpigma msg seek`.
//!
//! One parser for both front-ends, the way [`crate::cli::parse_volume`] is the one volume
//! grammar: a value the TUI command rejects must be rejected by the CLI too, instead of being
//! sent to the daemon to die in a toast the caller never sees. Each front-end keeps its own
//! wording for the failure (the command speaks Chinese, the CLI English).

/// What a seek value asks for, once [`parse_seek`] has settled its shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SeekTarget {
    /// `90` — a second of the track.
    Absolute(f64),
    /// `+15` / `-30` — seconds relative to the position.
    Relative(f64),
    /// `50%` — a fraction of the track.
    Fraction(f64),
}

/// Parse `+15` / `-30` / `50%` / `90` into a [`SeekTarget`].
///
/// The `Err` carries the message `:seek` shows, so the wording lives here with the grammar
/// rather than being spelled out again at the call site.
pub fn parse_seek(value: &str) -> Result<SeekTarget, String> {
    let invalid = || format!("无效的跳转位置: {value}");
    let number = |text: &str| -> Result<f64, String> {
        let number: f64 = text.parse().map_err(|_| invalid())?;
        // `f64` also parses "nan"/"inf". Neither is a position, and a NaN would poison
        // `progress` for every later frame, so the grammar refuses them both here.
        if number.is_finite() {
            Ok(number)
        } else {
            Err(invalid())
        }
    };

    if let Some(percent) = value.strip_suffix('%') {
        return Ok(SeekTarget::Fraction(number(percent)? / 100.0));
    }
    if value.starts_with(['+', '-']) {
        return Ok(SeekTarget::Relative(number(value)?));
    }
    Ok(SeekTarget::Absolute(number(value)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_four_shapes() {
        assert_eq!(parse_seek("90").unwrap(), SeekTarget::Absolute(90.0));
        assert_eq!(parse_seek("+15").unwrap(), SeekTarget::Relative(15.0));
        assert_eq!(parse_seek("-30").unwrap(), SeekTarget::Relative(-30.0));
        assert_eq!(parse_seek("50%").unwrap(), SeekTarget::Fraction(0.5));
    }

    #[test]
    fn rejects_what_is_not_a_position() {
        for bad in ["", "abc", "+", "50%%", "90s", "nan", "inf", "-inf", "1e999"] {
            assert!(parse_seek(bad).is_err(), "`{bad}` 不该被当成跳转位置");
        }
    }

    /// `:seek` shows this string verbatim, so it counts as the command's contract.
    #[test]
    fn the_error_names_the_offending_value() {
        assert_eq!(parse_seek("abc").unwrap_err(), "无效的跳转位置: abc");
    }
}

//! `[audio]`: what the player does to the sound between the decoder and the device.

use serde::{Deserialize, Serialize};

/// What the player does to the sound on its way to the device.
///
/// The default converts the sample rate — and only when the device does not run at the file's
/// rate, which leaves playback untouched (bit-perfect) for a file the device can take as it is.
/// Everything else is opt-in: an empty EQ and no loudness are a flat, unaltered signal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    /// Ask `rubato` for the sample-rate conversion instead of leaving it to the audio backend.
    /// A backend resamples to keep a stream alive; this one resamples to keep the recording
    /// intact. A file that already runs at the device's rate is never converted either way.
    pub resample: bool,
    /// The parametric EQ: one peaking band per entry, applied in order. Empty is flat.
    pub eq: Vec<EqBand>,
    /// Loudness normalization. `None` leaves the level alone.
    pub loudness: Option<LoudnessConfig>,
}

/// One EQ band: a peaking filter at `freq` Hz, cut or boosted by `gain_db`, with width `q`.
///
/// A cut and a boost of the same size are not the same thing to the ear, and a narrow band
/// (`q` above ~2) starts to ring: these are values to nudge by a few dB, not to sculpt with.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EqBand {
    /// Centre frequency, in Hz.
    pub freq: f32,
    /// How much to cut (negative) or boost (positive), in dB.
    pub gain_db: f32,
    /// Filter width: the centre frequency divided by the bandwidth. 1.0 is a gentle band.
    #[serde(default = "default_q")]
    pub q: f32,
}

fn default_q() -> f32 {
    1.0
}

/// EBU R128 loudness normalization: pull every track toward one level, so a quiet recording and
/// a loud one do not arrive 15 dB apart.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LoudnessConfig {
    /// The level the player aims for, in LUFS. The streaming services sit around -14; broadcast
    /// is -23.
    pub target_lufs: f32,
    /// Ceiling on how far a quiet track is lifted: past some point the correction starts
    /// amplifying the noise floor rather than the music.
    #[serde(default = "default_max_gain")]
    pub max_gain_db: f32,
}

fn default_max_gain() -> f32 {
    12.0
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            resample: true,
            eq: Vec::new(),
            loudness: None,
        }
    }
}

impl AudioConfig {
    /// Whether the chain would change anything at all. When it would not, the player hands the
    /// decoded samples straight to the sink.
    pub fn is_transparent(&self) -> bool {
        self.eq.is_empty() && self.loudness.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_leaves_the_sound_alone() {
        let config = AudioConfig::default();
        assert!(
            config.resample,
            "the rate conversion is the one thing on by default"
        );
        assert!(config.is_transparent(), "and nothing else is");
    }

    #[test]
    fn the_toml_shape_is_the_one_the_example_shows() {
        let config: AudioConfig = toml_edit::de::from_str(
            r#"
            resample = false

            [[eq]]
            freq = 105.0
            gain_db = -3.0

            [loudness]
            target_lufs = -14.0
            "#,
        )
        .expect("the documented shape parses");

        assert!(!config.resample);
        assert_eq!(
            config.eq,
            vec![EqBand {
                freq: 105.0,
                gain_db: -3.0,
                q: 1.0,
            }],
            "a band without `q` gets the default width"
        );
        assert_eq!(
            config.loudness,
            Some(LoudnessConfig {
                target_lufs: -14.0,
                max_gain_db: 12.0,
            })
        );
        assert!(!config.is_transparent());
    }
}

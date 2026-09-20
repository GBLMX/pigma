//! Dominant pitch of what is playing, shown as a note readout.
//!
//! The estimator is YIN (difference function, cumulative mean normalisation, parabolic
//! interpolation) implemented here instead of pulled in as a dependency: the
//! autocorrelation YIN needs is computed through the FFT the spectrum already uses, which
//! brings the naive O(N²) difference function down to O(N log N) and needs no extra
//! crate. `d[τ] = m[0] + m[τ] − 2·r[τ]` is the identity that makes this work: `r[τ]`
//! comes from the transform, `m` from prefix sums over the frame.
//!
//! A pitch only means something for a monophonic signal, so the readout is labelled as
//! the *dominant* pitch: on a full mix the estimate wanders between the bass line and
//! the melody. The last estimate is held for a moment so the display does not flicker.

use super::dsp::{Complex, fft, ifft};

/// Samples per estimate: several periods of the lowest note, and a power of two so the
/// padded transform stays one.
pub const WINDOW: usize = 2048;

/// Low end of the range the estimator reports, around A1.
const MIN_HZ: f64 = 55.0;
/// High end, around C7 — above the fundamental of most instruments.
const MAX_HZ: f64 = 2100.0;
/// YIN's absolute threshold on the normalised difference.
const YIN_THRESHOLD: f64 = 0.15;
/// When nothing dips below the threshold, a dip this good is still reported; anything
/// worse is noise rather than a note.
const ACCEPT_CLARITY: f64 = 0.35;
/// Frames quieter than this are silence, whatever the detector would make of them.
const MIN_RMS: f64 = 1e-4;
/// Frames an estimate survives while detection returns nothing (~0.5s at 30 fps).
const MAX_MISSES: u32 = 15;

const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// A frequency resolved to the nearest equal-tempered note.
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub name: &'static str,
    pub octave: i32,
    /// Deviation from the exact note, in hundredths of a semitone.
    pub cents: i32,
    pub frequency: f64,
    /// Confidence reported by the estimator, `0.0..=1.0`.
    pub clarity: f64,
}

impl Note {
    /// Note name with octave, e.g. `A4`.
    pub fn label(&self) -> String {
        format!("{}{}", self.name, self.octave)
    }
}

/// Resolve a frequency to the nearest note, using A4 = 440 Hz.
pub fn note_from_frequency(frequency: f64) -> Option<Note> {
    if !frequency.is_finite() || frequency <= 0.0 {
        return None;
    }

    let midi = 69.0 + 12.0 * (frequency / 440.0).log2();
    if !midi.is_finite() {
        return None;
    }
    let rounded = midi.round();
    let index = rounded as i64;

    Some(Note {
        name: NOTE_NAMES[index.rem_euclid(12) as usize],
        octave: (index.div_euclid(12) - 1) as i32,
        cents: ((midi - rounded) * 100.0).round() as i32,
        frequency,
        clarity: 0.0,
    })
}

/// YIN estimator with reusable scratch buffers, so a frame allocates nothing.
struct Yin {
    /// Windowed frame, zero-padded to the transform length.
    spectrum: Vec<Complex>,
    /// Prefix sums of the squared windowed frame, for `m[τ]`.
    prefix: Vec<f64>,
    /// `d[τ]`, replaced in place by the cumulative-mean-normalised form.
    difference: Vec<f64>,
    /// Samples of the current frame, converted from the decoder's `f32`.
    frame: Vec<f64>,
}

impl Yin {
    fn new() -> Self {
        let transform_len = (2 * WINDOW).next_power_of_two();
        Self {
            spectrum: vec![Complex::default(); transform_len],
            prefix: vec![0.0; WINDOW + 1],
            difference: vec![0.0; WINDOW + 1],
            frame: vec![0.0; WINDOW],
        }
    }

    /// Estimate `(frequency_hz, clarity)` of one frame, or `None` for silence/noise.
    fn estimate(&mut self, samples: &[f32], sample_rate: f64) -> Option<(f64, f64)> {
        debug_assert_eq!(samples.len(), WINDOW);
        let n = WINDOW;

        // Deliberately unwindowed: the difference function is defined on the raw signal,
        // and a Hann window shallows its dip exactly at the low frequencies where the
        // estimate is hardest. The transform path is the linear autocorrelation anyway
        // (the frame is zero-padded to twice its length), so no window is needed to
        // avoid circular wraparound either.
        for (slot, sample) in self.frame.iter_mut().zip(samples) {
            *slot = f64::from(*sample);
        }

        let energy: f64 = self.frame.iter().map(|x| x * x).sum();
        let rms = (energy / n as f64).sqrt();
        if rms < MIN_RMS {
            return None;
        }

        // Linear autocorrelation through the transform: padding to 2n turns the circular
        // result into the linear one for every lag below n.
        for (slot, value) in self.spectrum.iter_mut().zip(&self.frame) {
            *slot = Complex::new(*value, 0.0);
        }
        for slot in self.spectrum.iter_mut().skip(n) {
            *slot = Complex::default();
        }
        fft(&mut self.spectrum);
        for bin in self.spectrum.iter_mut() {
            *bin = Complex::new(bin.magnitude_squared(), 0.0);
        }
        ifft(&mut self.spectrum);

        // Prefix sums of squares: m[τ] is the energy of the two overlapping segments.
        self.prefix[0] = 0.0;
        for i in 0..n {
            self.prefix[i + 1] = self.prefix[i] + self.frame[i] * self.frame[i];
        }
        let total = self.prefix[n];

        let min_lag = ((sample_rate / MAX_HZ).floor() as usize).max(2);
        let max_lag = ((sample_rate / MIN_HZ).ceil() as usize).min(n - 1);
        if max_lag <= min_lag {
            return None;
        }

        // d[τ] = A(τ) + B(τ) − 2·r[τ], the exact expansion of Σ(x_i − x_{i+τ})² over the
        // overlapping parts of the frame, then normalised by the running mean over lags.
        let mut running = 0.0;
        for tau in 1..=max_lag {
            let m = self.prefix[n - tau] + (total - self.prefix[tau]);
            let difference = (m - 2.0 * self.spectrum[tau].re).max(0.0);
            running += difference;
            self.difference[tau] = difference * tau as f64 / running;
        }

        // First dip below the threshold wins; descending from it lands on the minimum
        // that a naive "smallest value" pick would have missed.
        let mut chosen = None;
        let mut tau = min_lag;
        while tau < max_lag {
            if self.difference[tau] < YIN_THRESHOLD {
                while tau < max_lag && self.difference[tau + 1] < self.difference[tau] {
                    tau += 1;
                }
                chosen = Some(tau);
                break;
            }
            tau += 1;
        }

        let (lag, clarity) = match chosen {
            Some(tau) => (tau, 1.0 - self.difference[tau]),
            None => {
                let (tau, value) = (min_lag..=max_lag)
                    .map(|t| (t, self.difference[t]))
                    .min_by(|a, b| a.1.total_cmp(&b.1))?;
                if value > ACCEPT_CLARITY {
                    return None;
                }
                (tau, 1.0 - value)
            }
        };

        // Sub-lag accuracy: the true minimum sits between samples, so interpolate.
        let refined = if lag > min_lag && lag < max_lag {
            let low = self.difference[lag - 1];
            let mid = self.difference[lag];
            let high = self.difference[lag + 1];
            let denominator = low - 2.0 * mid + high;
            if denominator.abs() > f64::EPSILON {
                lag as f64 + 0.5 * (low - high) / denominator
            } else {
                lag as f64
            }
        } else {
            lag as f64
        };

        Some((sample_rate / refined, clarity.clamp(0.0, 1.0)))
    }
}

/// Runs the estimator over the newest samples of the shared tap.
pub struct PitchTracker {
    yin: Yin,
    misses: u32,
    held: Option<Note>,
}

impl Default for PitchTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl PitchTracker {
    pub fn new() -> Self {
        Self {
            yin: Yin::new(),
            misses: 0,
            held: None,
        }
    }

    /// Estimate the pitch of the newest samples.
    ///
    /// Returns the estimate to display, or the previous one while detection is
    /// temporarily inconclusive (a held note decays away after [`MAX_MISSES`] frames).
    pub fn analyze(&mut self, mono: &[f32], sample_rate: u32) -> Option<Note> {
        if sample_rate == 0 || mono.len() < WINDOW {
            return self.hold();
        }

        let detected = self
            .yin
            .estimate(&mono[mono.len() - WINDOW..], f64::from(sample_rate))
            .and_then(|(frequency, clarity)| {
                note_from_frequency(frequency).map(|mut note| {
                    note.clarity = clarity;
                    note
                })
            });

        match detected {
            Some(note) => {
                self.misses = 0;
                self.held = Some(note.clone());
                Some(note)
            }
            None => self.hold(),
        }
    }

    /// Keep showing the last note for a few frames, then let it go.
    fn hold(&mut self) -> Option<Note> {
        self.held.as_ref()?;
        if self.misses < MAX_MISSES {
            self.misses += 1;
            return self.held.clone();
        }
        self.held = None;
        None
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use super::*;

    const RATE: u32 = 48000;

    fn sine(frequency: f64, len: usize) -> Vec<f32> {
        (0..len)
            .map(|i| {
                let t = i as f64 / f64::from(RATE);
                (2.0 * PI * frequency * t).sin() as f32
            })
            .collect()
    }

    fn estimate(samples: &[f32]) -> Option<(f64, f64)> {
        Yin::new().estimate(samples, f64::from(RATE))
    }

    #[test]
    fn concert_pitch_resolves_to_a4() {
        let note = note_from_frequency(440.0).expect("440 Hz is a valid frequency");
        assert_eq!(note.name, "A");
        assert_eq!(note.octave, 4);
        assert_eq!(note.cents, 0);
    }

    #[test]
    fn exact_notes_land_on_their_name() {
        assert_eq!(note_from_frequency(261.6256).unwrap().label(), "C4");
        assert_eq!(note_from_frequency(880.0).unwrap().label(), "A5");
        assert_eq!(note_from_frequency(27.5).unwrap().label(), "A0");
        assert_eq!(note_from_frequency(1975.533).unwrap().label(), "B6");
    }

    /// A slightly detuned note must report the deviation, not a different note.
    #[test]
    fn detuned_notes_report_cents() {
        let sharp = note_from_frequency(442.0).expect("valid");
        assert_eq!(sharp.label(), "A4");
        assert!(
            (5..=10).contains(&sharp.cents),
            "442 Hz is about +8 cents above A4, got {}",
            sharp.cents
        );

        let flat = note_from_frequency(438.0).expect("valid");
        assert_eq!(flat.label(), "A4");
        assert!(flat.cents < 0, "438 Hz must read flat, got {}", flat.cents);
    }

    #[test]
    fn unusable_frequencies_have_no_note() {
        assert!(note_from_frequency(0.0).is_none());
        assert!(note_from_frequency(-100.0).is_none());
        assert!(note_from_frequency(f64::NAN).is_none());
        assert!(note_from_frequency(f64::INFINITY).is_none());
    }

    /// The estimator's core claim: a generated tone comes back as its own frequency.
    #[test]
    fn generated_tones_come_back_at_their_frequency() {
        for &frequency in &[110.0, 220.0, 440.0, 880.0, 1760.0] {
            let (detected, clarity) = estimate(&sine(frequency, WINDOW)).unwrap_or_else(|| {
                panic!("{frequency} Hz was not detected at all");
            });
            let error = (detected - frequency).abs();
            assert!(
                error < frequency * 0.01,
                "{frequency} Hz came back as {detected} Hz (clarity {clarity})"
            );
            assert!(clarity > 0.8, "{frequency} Hz had clarity {clarity}");
        }
    }

    /// Harmonic-rich signals are where naive peak picking fails: the strongest partial is
    /// not always the fundamental, and YIN must still report the fundamental.
    #[test]
    fn harmonics_do_not_hide_the_fundamental() {
        let fundamental = 220.0;
        let samples: Vec<f32> = (0..WINDOW)
            .map(|i| {
                let t = i as f64 / f64::from(RATE);
                let phase = 2.0 * PI * fundamental * t;
                (0.5 * phase.sin() + 0.9 * (2.0 * phase).sin() + 0.7 * (3.0 * phase).sin()) as f32
            })
            .collect();

        let (detected, _) = estimate(&samples).expect("a harmonic tone has a pitch");
        assert!(
            (detected - fundamental).abs() < fundamental * 0.02,
            "expected {fundamental} Hz, got {detected} Hz"
        );
    }

    #[test]
    fn silence_and_noise_have_no_pitch() {
        assert!(estimate(&vec![0.0; WINDOW]).is_none());

        // Deterministic pseudo-noise: an LCG keeps the test reproducible.
        let mut state = 0x1234_5678u32;
        let noise: Vec<f32> = (0..WINDOW)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                ((state >> 8) as f32 / 8_388_608.0) - 1.0
            })
            .collect();
        assert!(
            estimate(&noise).is_none(),
            "white noise must not be reported as a note"
        );
    }

    #[test]
    fn a_short_buffer_yields_nothing_instead_of_panicking() {
        let mut tracker = PitchTracker::new();
        assert!(tracker.analyze(&[0.0; 16], RATE).is_none());
        assert!(tracker.analyze(&sine(440.0, WINDOW), 0).is_none());
    }

    /// A detected note must survive a few silent frames, then disappear.
    #[test]
    fn held_notes_expire_after_silence() {
        let mut tracker = PitchTracker::new();
        assert!(tracker.analyze(&sine(440.0, WINDOW), RATE).is_some());

        let silence = vec![0.0f32; WINDOW];
        for _ in 0..MAX_MISSES {
            assert!(
                tracker.analyze(&silence, RATE).is_some(),
                "a detected note should survive a few silent frames"
            );
        }
        assert!(
            tracker.analyze(&silence, RATE).is_none(),
            "a note must not be held forever"
        );
    }

    /// The FFT-based autocorrelation must equal the direct sum it replaces.
    #[test]
    fn fft_autocorrelation_matches_the_direct_sum() {
        let samples: Vec<f32> = sine(300.0, WINDOW);
        let mut yin = Yin::new();
        for (slot, sample) in yin.frame.iter_mut().zip(&samples) {
            *slot = f64::from(*sample);
        }

        // Recompute the transform path on a copy of the windowed frame.
        yin.spectrum
            .iter_mut()
            .for_each(|slot| *slot = Complex::default());
        for (slot, value) in yin.spectrum.iter_mut().zip(&yin.frame) {
            *slot = Complex::new(*value, 0.0);
        }
        fft(&mut yin.spectrum);
        for bin in yin.spectrum.iter_mut() {
            *bin = Complex::new(bin.magnitude_squared(), 0.0);
        }
        ifft(&mut yin.spectrum);

        for lag in [1usize, 40, 160, 800] {
            let direct: f64 = (0..WINDOW - lag)
                .map(|i| yin.frame[i] * yin.frame[i + lag])
                .sum();
            let transformed = yin.spectrum[lag].re;
            assert!(
                (direct - transformed).abs() < direct.abs() * 1e-6 + 1e-9,
                "lag {lag}: direct {direct}, fft {transformed}"
            );
        }
    }
}

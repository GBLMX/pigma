//! Frequency spectrum of what is playing.
//!
//! The decoder chain hands us `f32` samples before they reach the audio device, so a
//! small pass-through source ([`SpectrumTap`]) can mirror them into a ring buffer. The
//! UI then asks [`Spectrum`] for bar levels once per frame, which keeps the audio path
//! free of any analysis and the analysis off the audio thread.
//!
//! The FFT is a plain radix-2 implementation (no dependency) over a Hann-windowed frame,
//! because the bars only need to be visually right: bins are folded into logarithmically
//! spaced bands and smoothed with a decay, the same shape `cava`/`scope-tui` use.

use std::{
    collections::VecDeque,
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};

use rodio::{ChannelCount, Sample, SampleRate, Source};

use super::dsp::{Complex, fft, hann_into};

/// How many samples the analysis window holds; also the FFT size.
pub const WINDOW: usize = 1024;

/// Bands drawn, from the lowest to the highest frequency.
pub const BANDS: usize = 24;

/// Samples the tap accumulates before taking the lock: one lock per chunk keeps the
/// audio thread free of per-sample synchronisation.
const FLUSH_SAMPLES: usize = 512;

/// Samples the ring keeps, i.e. the largest window any consumer reads: the spectrum's
/// [`WINDOW`] and the pitch tracker's (larger) window.
pub const RING_SAMPLES: usize = 2048;

const MIN_HZ: f64 = 40.0;
const MAX_HZ: f64 = 16000.0;
/// How much of the previous level survives a frame, so bars fall back smoothly instead
/// of flickering with the music.
const DECAY: f32 = 0.55;
/// Bars sit this far below the loudest band, which roughly matches the ear's range.
const FLOOR_DB: f64 = -60.0;

/// Ring buffer of the most recent interleaved samples, shared by the audio thread and
/// the UI.
#[derive(Debug, Default)]
pub struct SpectrumBuffer {
    /// Samples plus the channel count they were decoded with (songs differ: mono files
    /// interleave differently from stereo ones).
    ring: Mutex<(VecDeque<f32>, usize, u32)>,
}

impl SpectrumBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Copy a decoded chunk into the ring, remembering its layout: songs differ in
    /// channel count and sample rate, and both feed the analysis.
    pub fn push(&self, samples: &[f32], channels: usize, sample_rate: u32) {
        let channels = channels.max(1);
        let Ok(mut ring) = self.ring.lock() else {
            return;
        };
        ring.1 = channels;
        if sample_rate > 0 {
            ring.2 = sample_rate;
        }
        let capacity = RING_SAMPLES * channels;
        for sample in samples {
            while ring.0.len() >= capacity {
                ring.0.pop_front();
            }
            ring.0.push_back(*sample);
        }
    }

    /// The most recent samples, mixed down to mono (empty when nothing was played yet).
    pub fn snapshot(&self, mono: &mut Vec<f32>) {
        mono.clear();
        let Ok(mut ring) = self.ring.lock() else {
            return;
        };
        let channels = ring.1.max(1);
        let complete = ring.0.len() / channels * channels;
        if complete == 0 {
            return;
        }

        // `make_contiguous` needs the mutable guard, and gives the frames back in order.
        for frame in ring.0.make_contiguous()[..complete].chunks(channels) {
            mono.push(frame.iter().sum::<f32>() / channels as f32);
        }
    }

    /// Sample rate of the most recent chunk, or 0 before anything played.
    pub fn sample_rate(&self) -> u32 {
        self.ring.lock().map(|ring| ring.2).unwrap_or(0)
    }
}

/// Process-wide sample ring.
///
/// One process plays one song, so the audio thread and the UI share a single buffer
/// instead of threading an `Arc` through the engine, controller and player signatures.
pub fn buffer() -> Arc<SpectrumBuffer> {
    static BUFFER: LazyLock<Arc<SpectrumBuffer>> =
        LazyLock::new(|| Arc::new(SpectrumBuffer::new()));
    Arc::clone(&BUFFER)
}

/// Pass-through audio source that mirrors samples into a [`SpectrumBuffer`].
pub struct SpectrumTap<S> {
    inner: S,
    buffer: Arc<SpectrumBuffer>,
    pending: Vec<f32>,
}

impl<S> SpectrumTap<S> {
    pub fn new(inner: S, buffer: Arc<SpectrumBuffer>) -> Self {
        Self {
            inner,
            buffer,
            pending: Vec::with_capacity(FLUSH_SAMPLES),
        }
    }
}

impl<S> Iterator for SpectrumTap<S>
where
    S: Source<Item = Sample>,
{
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        let sample = self.inner.next()?;
        self.pending.push(sample);
        if self.pending.len() >= FLUSH_SAMPLES {
            self.buffer.push(
                &self.pending,
                self.inner.channels().get() as usize,
                self.inner.sample_rate().get(),
            );
            self.pending.clear();
        }
        Some(sample)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<S> Source for SpectrumTap<S>
where
    S: Source<Item = Sample>,
{
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

/// Spectrum analyser: turns a mono sample window into smoothed bar levels.
pub struct Spectrum {
    sample_rate: u32,
    levels: Vec<f32>,
    /// Inclusive start/end FFT bin of every band, precomputed from the log scale.
    band_bins: Vec<(usize, usize)>,
    /// Windowed frame and its transform, kept between frames so analysing allocates
    /// nothing on the display path.
    windowed: Vec<f64>,
    scratch: Vec<Complex>,
}

impl Default for Spectrum {
    fn default() -> Self {
        Self::new()
    }
}

impl Spectrum {
    pub fn new() -> Self {
        // The band edges are rebuilt on the first analysis with the real sample rate.
        let sample_rate = 48000;
        Self {
            sample_rate,
            levels: vec![0.0; BANDS],
            band_bins: band_edges(sample_rate, WINDOW, BANDS),
            windowed: Vec::with_capacity(WINDOW),
            scratch: vec![Complex::default(); WINDOW],
        }
    }

    /// Bars for the current window, each in `0.0..=1.0`, loudest band at 1.0.
    pub fn bars(&self) -> &[f32] {
        &self.levels
    }

    /// Analyse one mono window (at most [`WINDOW`] samples) and refresh the bars.
    ///
    /// `sample_rate` is the rate the window was decoded with; the band edges follow it,
    /// so a 44.1 kHz track is not measured against 48 kHz boundaries.
    pub fn analyze(&mut self, mono: &[f32], sample_rate: u32) {
        if sample_rate > 0 && sample_rate != self.sample_rate {
            self.sample_rate = sample_rate;
            self.band_bins = band_edges(sample_rate, WINDOW, BANDS);
        }

        let take = mono.len().min(WINDOW);
        if take < 2 {
            self.decay();
            return;
        }
        // Use the tail of the window: it is the part closest to what is being heard.
        let window = &mono[mono.len() - take..];

        hann_into(window, &mut self.windowed);
        self.scratch.clear();
        self.scratch
            .extend(self.windowed.iter().map(|value| Complex::new(*value, 0.0)));
        fft(&mut self.scratch);

        let scale = 2.0 / take as f64;
        let mut loudest = f64::MIN;
        let mut bands = vec![0.0f64; BANDS];
        for (band, (start, end)) in self.band_bins.iter().enumerate() {
            let mut peak = 0.0f64;
            for bin in *start..=*end {
                // Bins above Nyquist only exist for the upper half of the output, which
                // the log scale never reaches, but clamp defensively.
                if bin < self.scratch.len() / 2 {
                    peak = peak.max(self.scratch[bin].magnitude() * scale);
                }
            }
            let db = if peak > 0.0 {
                20.0 * peak.log10()
            } else {
                FLOOR_DB
            };
            bands[band] = db;
            loudest = loudest.max(db);
        }

        // A 60 dB window ending at the loudest band: everything below it is drawn as an
        // empty bar. When even the loudest band sits at the floor, nothing is playing.
        let bottom = loudest + FLOOR_DB;
        for (level, db) in self.levels.iter_mut().zip(bands) {
            let normalized = if loudest <= FLOOR_DB {
                0.0
            } else {
                ((db - bottom) / -FLOOR_DB).clamp(0.0, 1.0) as f32
            };
            // Attack instantly, release slowly: bars jump to the beat and settle down.
            *level = if normalized > *level {
                normalized
            } else {
                (*level * DECAY).max(normalized)
            };
        }
    }

    /// Let the bars fall without new samples (paused, or silence).
    pub fn decay(&mut self) {
        for level in &mut self.levels {
            *level *= DECAY;
        }
    }
}

/// FFT bin range of every band, spaced logarithmically between the lowest and highest
/// audible frequency.
fn band_edges(sample_rate: u32, window: usize, bands: usize) -> Vec<(usize, usize)> {
    let bin_hz = f64::from(sample_rate) / window as f64;
    let nyquist_bin = window / 2 - 1;
    let max_hz = MAX_HZ.min(f64::from(sample_rate) / 2.0);

    (0..bands)
        .filter_map(|band| {
            let low = MIN_HZ * (max_hz / MIN_HZ).powf(band as f64 / bands as f64);
            let high = MIN_HZ * (max_hz / MIN_HZ).powf((band + 1) as f64 / bands as f64);
            let start = (low / bin_hz).floor() as usize;
            let end = ((high / bin_hz).ceil() as usize).saturating_sub(1);
            if start > nyquist_bin {
                return None;
            }
            Some((
                start.min(nyquist_bin),
                end.min(nyquist_bin).max(start.min(nyquist_bin)),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 440 Hz tone has to light up the band that contains 440 Hz.
    #[test]
    fn a_440_hz_tone_peaks_in_its_band() {
        let sample_rate = 48000u32;
        let mut mono = Vec::with_capacity(WINDOW);
        for i in 0..WINDOW {
            let t = i as f64 / f64::from(sample_rate);
            mono.push((2.0 * std::f64::consts::PI * 440.0 * t).sin() as f32);
        }

        let mut spectrum = Spectrum::new();
        spectrum.analyze(&mono, sample_rate);

        let peak_band = spectrum
            .bars()
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(index, _)| index)
            .expect("bands are never empty");

        let (start, _) = spectrum.band_bins[peak_band];
        let bin_hz = f64::from(sample_rate) / WINDOW as f64;
        let peak_hz = start as f64 * bin_hz;
        let next_hz = spectrum
            .band_bins
            .get(peak_band + 1)
            .map_or(f64::MAX, |(next, _)| *next as f64 * bin_hz);
        assert!(
            peak_hz <= 440.0 && 440.0 < next_hz,
            "peak landed on band {peak_band} ({peak_hz}..{next_hz} Hz)"
        );
    }

    #[test]
    fn silence_stays_at_the_floor() {
        let mut spectrum = Spectrum::new();
        spectrum.analyze(&vec![0.0; WINDOW], 48000);
        assert!(
            spectrum.bars().iter().all(|level| *level <= f32::EPSILON),
            "silence drew bars: {:?}",
            spectrum.bars()
        );
    }

    /// Bars must fall back over time rather than stay latched or snap to zero.
    #[test]
    fn bars_decay_after_the_tone_stops() {
        let mut mono = Vec::with_capacity(WINDOW);
        for i in 0..WINDOW {
            let t = i as f64 / 48000.0;
            mono.push((2.0 * std::f64::consts::PI * 440.0 * t).sin() as f32);
        }

        let mut spectrum = Spectrum::new();
        spectrum.analyze(&mono, 48000);
        let loud = spectrum.bars().iter().copied().fold(0.0f32, f32::max);
        assert!(loud > 0.5, "a full-scale tone should nearly fill a bar");

        spectrum.analyze(&vec![0.0; WINDOW], 48000);
        let quieter = spectrum.bars().iter().copied().fold(0.0f32, f32::max);
        assert!(quieter < loud, "bars must decay once the tone stops");
        assert!(quieter > 0.0, "decay must be gradual, not a snap to zero");
    }

    /// The tap must be transparent to playback and still fill the ring.
    #[test]
    fn tap_passes_samples_through_and_fills_the_ring() {
        let buffer = Arc::new(SpectrumBuffer::new());
        let source = rodio::buffer::SamplesBuffer::new(
            ChannelCount::new(2).expect("two channels"),
            SampleRate::new(48000).expect("non-zero rate"),
            vec![0.25f32; FLUSH_SAMPLES],
        );
        let tapped = SpectrumTap::new(source, Arc::clone(&buffer));

        let passed: Vec<f32> = tapped.collect();
        assert_eq!(passed.len(), FLUSH_SAMPLES, "the tap must not drop samples");
        assert!(passed.iter().all(|s| (*s - 0.25).abs() < f32::EPSILON));

        let mut mono = Vec::new();
        buffer.snapshot(&mut mono);
        assert_eq!(
            mono.len(),
            FLUSH_SAMPLES / 2,
            "the tap must fill the ring with whole frames"
        );
        assert!(
            mono.iter().all(|s| (*s - 0.25).abs() < f32::EPSILON),
            "channels must be averaged into mono"
        );
    }
}

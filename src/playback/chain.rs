//! The chain the samples pass through on their way to the device: sample-rate conversion, the
//! parametric EQ and loudness normalization.
//!
//! This is not [`super::dsp`], which is the *analysis* side — the spectrum bars and the pitch
//! estimator. These adapters change what is heard; those only look at it.
//!
//! Each stage is a `rodio::Source` adapter over the decoded samples, so the player's chain
//! reads as what it does, and a stage with nothing to do is not built at all: with the device
//! running at the file's rate and no EQ or normalization configured, the decoded samples reach
//! the sink untouched — the bit-perfect path.

use std::{num::NonZero, time::Duration};

use rodio::{ChannelCount, Sample, SampleRate, Source};
use rubato::{Fft, FixedSync, Resampler as _};

/// Frames pulled from the inner source per call.
///
/// rubato is built around chunks of a few hundred to a few thousand frames; this keeps the
/// added latency down around what the device buffers anyway, without running the FFT on
/// something so small that its overhead dominates.
const CHUNK: usize = 1024;

/// Sample-rate conversion, from the file's rate to the device's.
///
/// `rubato`'s synchronous FFT resampler: the ratio is fixed for the life of a song, which is
/// exactly the case here, and the filter it applies is what keeps a converted recording from
/// aliasing where the backend's own converter would not care.
pub(super) struct Resample<S> {
    inner: S,
    resampler: Fft<f32>,
    channels: usize,
    /// Interleaved input for one chunk: `CHUNK` frames, zero-padded at the end of the file.
    input: Vec<f32>,
    /// The same chunk, per channel — what rubato's adapter reads.
    planar_in: Vec<Vec<f32>>,
    planar_out: Vec<Vec<f32>>,
    /// Interleaved output of the last call, handed out one sample at a time.
    pending: Vec<f32>,
    cursor: usize,
    out_rate: u32,
    /// The inner source ran out and the tail has been flushed; nothing more will come.
    finished: bool,
    /// Set once the tail has been padded and resampled, so the flush happens exactly once.
    flushed: bool,
}

impl<S: Source<Item = Sample>> Resample<S> {
    /// Wrap `inner`, or hand it straight back when there is nothing to do — the device already
    /// runs at the file's rate, or it has no rate to convert to. Getting the source back is
    /// what keeps a matching file bit-perfect: no adapter is built at all.
    pub(super) fn new(inner: S, out_rate: u32) -> Result<Self, S> {
        let in_rate = inner.sample_rate().get();
        let channels = usize::from(inner.channels().get());
        if in_rate == out_rate || channels == 0 || out_rate == 0 {
            return Err(inner);
        }
        let Ok(resampler) = Fft::<f32>::new(
            in_rate as usize,
            out_rate as usize,
            CHUNK,
            channels,
            FixedSync::Input,
        ) else {
            return Err(inner);
        };
        let out_max = resampler.output_frames_max();
        Ok(Self {
            inner,
            resampler,
            channels,
            input: vec![0.0; CHUNK * channels],
            planar_in: vec![vec![0.0; CHUNK]; channels],
            planar_out: vec![vec![0.0; out_max]; channels],
            pending: Vec::with_capacity(out_max * channels),
            cursor: 0,
            out_rate,
            finished: false,
            flushed: false,
        })
    }

    /// Read one chunk from the inner source and resample it. Returns whether anything was
    /// produced; the tail of a file is padded with silence so the resampler's own delay drains
    /// rather than being cut off.
    fn refill(&mut self) {
        let frames = CHUNK;
        self.input.resize(frames * self.channels, 0.0);
        self.input.fill(0.0);

        let mut read = 0usize;
        while read < frames {
            match self.inner.next() {
                Some(sample) => self.input[read * self.channels] = sample,
                None => break,
            }
            // The rest of the frame's channels come straight after it.
            for channel in 1..self.channels {
                // `next` yields interleaved samples, so the frame is filled in order; a source
                // that ends mid-frame simply leaves zeros behind.
                if let Some(sample) = self.inner.next() {
                    self.input[read * self.channels + channel] = sample;
                }
            }
            read += 1;
        }

        if read == 0 {
            if self.flushed {
                self.finished = true;
                self.pending.clear();
                self.cursor = 0;
                return;
            }
            // One padded chunk flushes the resampler's delay; after that there is nothing left.
            self.flushed = true;
        } else if read < frames {
            self.flushed = true;
        }

        for channel in 0..self.channels {
            for frame in 0..frames {
                self.planar_in[channel][frame] = self.input[frame * self.channels + channel];
            }
        }

        let input = rubato::audioadapter_buffers::direct::SequentialSliceOfVecs::new(
            &self.planar_in,
            self.channels,
            frames,
        );
        let output = rubato::audioadapter_buffers::direct::SequentialSliceOfVecs::new_mut(
            &mut self.planar_out,
            self.channels,
            self.resampler.output_frames_max(),
        );
        let (Ok(input), Ok(mut output)) = (input, output) else {
            self.finished = true;
            return;
        };
        let produced = match self
            .resampler
            .process_into_buffer(&input, &mut output, None)
        {
            Ok((_consumed, produced)) => produced,
            Err(error) => {
                log::warn!("resampler: {error}");
                self.finished = true;
                self.pending.clear();
                self.cursor = 0;
                return;
            }
        };

        self.pending.clear();
        self.pending.reserve(produced * self.channels);
        for frame in 0..produced {
            for channel in 0..self.channels {
                self.pending.push(self.planar_out[channel][frame]);
            }
        }
        self.cursor = 0;
    }
}

impl<S: Source<Item = Sample>> Iterator for Resample<S> {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        loop {
            if self.cursor < self.pending.len() {
                let sample = self.pending[self.cursor];
                self.cursor += 1;
                return Some(sample);
            }
            if self.finished {
                return None;
            }
            self.refill();
        }
    }
}

impl<S: Source<Item = Sample>> Source for Resample<S> {
    /// `f32` samples carry no span of their own; the conversion is continuous.
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        NonZero::new(self.channels as u16).expect("a resampler is built with channels")
    }

    fn sample_rate(&self) -> SampleRate {
        NonZero::new(self.out_rate).expect("a resampler is built with a rate")
    }

    /// The duration is the file's: the conversion changes how many frames carry it, not how
    /// long the recording is.
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }

    fn try_seek(&mut self, position: Duration) -> Result<(), rodio::source::SeekError> {
        self.inner.try_seek(position)?;
        // The resampler carries filter state across chunks, so the seeked-to samples would
        // otherwise be filtered with the old position's tail.
        self.resampler.reset();
        self.pending.clear();
        self.cursor = 0;
        self.finished = false;
        self.flushed = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A source of `frames` frames of silence at a set rate, so the resampler can be exercised
    /// without a decoder or a device.
    #[derive(Debug)]
    struct Silence {
        frames: usize,
        channels: ChannelCount,
        rate: SampleRate,
        left: usize,
    }

    impl Silence {
        fn new(frames: usize, channels: u16, rate: u32) -> Self {
            Self {
                frames,
                channels: NonZero::new(channels).expect("channels"),
                rate: NonZero::new(rate).expect("rate"),
                left: frames * usize::from(channels),
            }
        }
    }

    impl Iterator for Silence {
        type Item = Sample;

        fn next(&mut self) -> Option<Sample> {
            if self.left == 0 {
                return None;
            }
            self.left -= 1;
            Some(0.0)
        }
    }

    impl Source for Silence {
        fn current_span_len(&self) -> Option<usize> {
            None
        }
        fn channels(&self) -> ChannelCount {
            self.channels
        }
        fn sample_rate(&self) -> SampleRate {
            self.rate
        }
        fn total_duration(&self) -> Option<Duration> {
            Some(Duration::from_secs_f64(
                self.frames as f64 / f64::from(self.rate.get()),
            ))
        }
    }

    /// The conversion reports the device's rate, keeps the channel count, and produces about
    /// as many frames as its ratio says — which is the whole point of doing it here.
    #[test]
    fn a_converted_stream_runs_at_the_devices_rate() {
        let source = Silence::new(44_100, 2, 44_100);
        let resampled = Resample::new(source, 48_000).expect("rates differ");

        assert_eq!(resampled.sample_rate().get(), 48_000);
        assert_eq!(resampled.channels().get(), 2);

        let produced = resampled.count();
        let expected = 48_000 * 2;
        let frames = produced / 2;
        assert!(
            frames.abs_diff(48_000) < 2_000,
            "expected about {expected} samples, got {produced}"
        );
    }

    /// Nothing to convert is nothing added: the same file the device already runs at is not
    /// resampled, which is what keeps it bit-perfect.
    #[test]
    fn a_matching_rate_is_left_alone() {
        let source = Silence::new(1_000, 2, 48_000);
        assert!(
            Resample::new(source, 48_000).is_err(),
            "a matching rate comes straight back, unwrapped"
        );
    }
}

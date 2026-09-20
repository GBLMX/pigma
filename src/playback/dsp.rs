//! DSP helpers shared by the spectrum bars and the pitch estimator: a dependency-free
//! radix-2 FFT (forward and inverse) plus the Hann window both of them use.
//!
//! Kept in one place so the two analysers cannot drift apart, and so the crate keeps
//! needing no FFT dependency: the transform is ~70 lines and the sizes involved (up to a
//! few thousand points, a few dozen times per second) are far below what a general
//! library would buy.

use std::f64::consts::PI;

/// One complex number.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct Complex {
    pub re: f64,
    pub im: f64,
}

impl Complex {
    pub(super) fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }

    fn mul(self, other: Self) -> Self {
        Self {
            re: self.re * other.re - self.im * other.im,
            im: self.re * other.im + self.im * other.re,
        }
    }

    fn add(self, other: Self) -> Self {
        Self {
            re: self.re + other.re,
            im: self.im + other.im,
        }
    }

    fn sub(self, other: Self) -> Self {
        Self {
            re: self.re - other.re,
            im: self.im - other.im,
        }
    }

    pub(super) fn magnitude(self) -> f64 {
        (self.re * self.re + self.im * self.im).sqrt()
    }

    /// `|z|²`, which is what the power spectrum needs and skips a square root.
    pub(super) fn magnitude_squared(self) -> f64 {
        self.re * self.re + self.im * self.im
    }
}

/// In-place radix-2 FFT. `data.len()` must be a power of two.
pub(super) fn fft(data: &mut [Complex]) {
    transform(data, false);
}

/// In-place inverse FFT, scaled so `ifft(fft(x)) == x`.
pub(super) fn ifft(data: &mut [Complex]) {
    transform(data, true);
    let scale = 1.0 / data.len() as f64;
    for value in data.iter_mut() {
        value.re *= scale;
        value.im *= scale;
    }
}

fn transform(data: &mut [Complex], inverse: bool) {
    let n = data.len();
    if n <= 1 {
        return;
    }

    // Bit-reversal permutation.
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            data.swap(i, j);
        }
    }

    let sign = if inverse { 1.0 } else { -1.0 };
    let mut len = 2;
    while len <= n {
        let angle = sign * 2.0 * PI / len as f64;
        let root = Complex::new(angle.cos(), angle.sin());
        for start in (0..n).step_by(len) {
            let mut twiddle = Complex::new(1.0, 0.0);
            for k in 0..len / 2 {
                let even = data[start + k];
                let odd = data[start + k + len / 2].mul(twiddle);
                data[start + k] = even.add(odd);
                data[start + k + len / 2] = even.sub(odd);
                twiddle = twiddle.mul(root);
            }
        }
        len <<= 1;
    }
}

/// Periodic (DFT-even) Hann window over `samples`, written into `out` as `f64`.
///
/// The periodic form is the right one for spectral work: the window tiles seamlessly,
/// which is what keeps neighbouring bands from leaking into each other. Writing into a
/// caller-owned buffer keeps the frame path allocation-free.
pub(super) fn hann_into(samples: &[f32], out: &mut Vec<f64>) {
    let len = samples.len() as f64;
    out.clear();
    out.extend(samples.iter().enumerate().map(|(i, sample)| {
        let factor = 0.5 * (1.0 - (2.0 * PI * i as f64 / len).cos());
        f64::from(*sample) * factor
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The transform must agree with the definition, otherwise every downstream number
    /// (bar heights, pitch in Hz) is a plausible-looking lie.
    #[test]
    fn fft_matches_a_naive_dft() {
        let n = 16;
        let signal: Vec<f64> = (0..n)
            .map(|i| (i as f64 * 0.7).sin() + 0.3 * (i as f64 * 2.1).cos())
            .collect();

        let mut data: Vec<Complex> = signal.iter().map(|v| Complex::new(*v, 0.0)).collect();
        fft(&mut data);

        for (k, bin) in data.iter().enumerate() {
            let mut re = 0.0;
            let mut im = 0.0;
            for (index, value) in signal.iter().enumerate() {
                let angle = -2.0 * PI * (k * index) as f64 / n as f64;
                re += value * angle.cos();
                im += value * angle.sin();
            }
            let expected = (re * re + im * im).sqrt();
            let actual = bin.magnitude();
            assert!(
                (expected - actual).abs() < 1e-9,
                "bin {k}: naive {expected}, fft {actual}"
            );
        }
    }

    #[test]
    fn inverse_fft_round_trips() {
        let original: Vec<Complex> = (0..32)
            .map(|i| Complex::new((i as f64 * 0.3).sin(), (i as f64 * 0.11).cos()))
            .collect();
        let mut data = original.clone();

        fft(&mut data);
        ifft(&mut data);

        for (got, want) in data.iter().zip(&original) {
            assert!(
                (got.re - want.re).abs() < 1e-12 && (got.im - want.im).abs() < 1e-12,
                "round trip changed {want:?} into {got:?}"
            );
        }
    }

    #[test]
    fn hann_window_tapers_both_ends() {
        let samples = vec![1.0f32; 64];
        let mut windowed = Vec::new();
        hann_into(&samples, &mut windowed);
        assert_eq!(windowed.len(), samples.len());
        assert!(windowed[0] < 1e-9, "first sample must fade in");
        assert!(windowed[63] < 0.01, "last sample must fade out");
        assert!(windowed[32] > 0.99, "centre must stay near unity");
    }
}

/// Benchmarks — not correctness tests. Run with
/// `cargo test --release --lib -- --ignored --nocapture dsp_bench`.
#[cfg(test)]
mod dsp_bench {
    use std::hint::black_box;

    use super::*;

    #[test]
    #[ignore = "benchmark"]
    fn fft_and_window_cost() {
        println!("DSP（每帧 30 次的预算内）:");
        for size in [512usize, 1024, 2048] {
            let mut data: Vec<Complex> = (0..size)
                .map(|i| Complex::new((i as f64 * 0.01).sin(), 0.0))
                .collect();
            crate::bench_util::time(&format!("fft {size} 点"), 2000, || {
                fft(black_box(&mut data));
            });
        }

        let samples: Vec<f32> = (0..2048).map(|i| (i as f32 * 0.02).sin()).collect();
        let mut windowed = Vec::with_capacity(2048);
        crate::bench_util::time("hann 2048 点", 20000, || {
            hann_into(black_box(&samples), black_box(&mut windowed));
        });
    }
}

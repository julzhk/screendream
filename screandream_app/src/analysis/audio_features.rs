use rustfft::{FftPlanner, num_complex::Complex};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub struct AudioFeatures {
    pub timestamp: Duration,
    pub smoothed_amplitude: f32,
    pub band_energies: [f32; 4],
    /// Log-spaced, auto-gained spectrum (≈30 Hz → 16 kHz), each bin in 0..1.
    pub spectrum: [f32; SPECTRUM_BINS],
    pub is_usable: bool,
}

/// Number of log-spaced bins in [`AudioFeatures::spectrum`].
pub const SPECTRUM_BINS: usize = 64;
const SPECTRUM_MIN_HZ: f32 = 30.0;
const SPECTRUM_MAX_HZ: f32 = 16_000.0;

impl AudioFeatures {
    pub fn silent(timestamp: Duration) -> Self {
        Self {
            timestamp,
            smoothed_amplitude: 0.0,
            band_energies: [0.0; 4],
            spectrum: [0.0; SPECTRUM_BINS],
            is_usable: false,
        }
    }
}

pub struct AudioFeatureAnalyzer {
    started_at: Instant,
    smoothed_amplitude: f32,
    silence_threshold: f32,
    sample_rate: f32,
    band_peaks: [f32; 4],
    smoothed_bands: [f32; 4],
    spectrum_peak: f32,
    smoothed_spectrum: [f32; SPECTRUM_BINS],
    planner: FftPlanner<f32>,
}

/// Upper edges (Hz) of the bass, low-mid, high-mid and treble bands.
const BAND_EDGES: [f32; 4] = [180.0, 900.0, 3_500.0, 16_000.0];

impl std::fmt::Debug for AudioFeatureAnalyzer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioFeatureAnalyzer")
            .field("smoothed_amplitude", &self.smoothed_amplitude)
            .field("sample_rate", &self.sample_rate)
            .field("band_peaks", &self.band_peaks)
            .finish_non_exhaustive()
    }
}

impl Default for AudioFeatureAnalyzer {
    fn default() -> Self {
        Self::new(0.01)
    }
}

impl AudioFeatureAnalyzer {
    pub fn new(silence_threshold: f32) -> Self {
        Self {
            started_at: Instant::now(),
            smoothed_amplitude: 0.0,
            silence_threshold,
            sample_rate: 48_000.0,
            band_peaks: [0.0; 4],
            smoothed_bands: [0.0; 4],
            spectrum_peak: 0.0,
            smoothed_spectrum: [0.0; SPECTRUM_BINS],
            planner: FftPlanner::new(),
        }
    }

    pub fn with_sample_rate(mut self, sample_rate: u32) -> Self {
        self.sample_rate = sample_rate.max(1) as f32;
        self
    }

    pub fn analyze(&mut self, samples: &[f32]) -> AudioFeatures {
        if samples.is_empty() {
            return AudioFeatures::silent(self.started_at.elapsed());
        }

        let amplitude = (samples.iter().map(|sample| sample * sample).sum::<f32>()
            / samples.len() as f32)
            .sqrt()
            .clamp(0.0, 1.0);
        self.smoothed_amplitude =
            (self.smoothed_amplitude * 0.75 + amplitude * 0.25).clamp(0.0, 1.0);

        // Hann-windowed FFT, grouped into log-spaced bands.
        let len = samples.len();
        let mut spectrum: Vec<Complex<f32>> = samples
            .iter()
            .enumerate()
            .map(|(i, &sample)| {
                let w = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / len.max(2) as f32).cos();
                Complex::new(sample * w, 0.0)
            })
            .collect();
        let fft_len = spectrum.len().next_power_of_two();
        spectrum.resize(fft_len, Complex::new(0.0, 0.0));
        self.planner
            .plan_fft_forward(fft_len)
            .process(&mut spectrum);

        let hz_per_bin = self.sample_rate / fft_len as f32;
        let mut sums = [0.0f32; 4];
        let mut counts = [0u32; 4];
        for (index, bin) in spectrum.iter().take(fft_len / 2).enumerate().skip(1) {
            let hz = index as f32 * hz_per_bin;
            if let Some(band) = BAND_EDGES.iter().position(|&edge| hz < edge) {
                sums[band] += bin.norm();
                counts[band] += 1;
            }
        }

        // Auto-gain: each band is scaled against its own slowly decaying peak so quiet and
        // loud tracks both use the full 0..1 range.
        let mut bands = [0.0; 4];
        for band in 0..4 {
            let raw = if counts[band] == 0 {
                0.0
            } else {
                sums[band] / counts[band] as f32 * 4.0 / fft_len as f32
            };
            if !raw.is_finite() {
                continue;
            }
            self.band_peaks[band] = (self.band_peaks[band] * 0.997).max(raw).max(1e-4);
            let level = (raw / self.band_peaks[band]).clamp(0.0, 1.0);
            let smoothing = if level > self.smoothed_bands[band] {
                0.55
            } else {
                0.12
            };
            self.smoothed_bands[band] += (level - self.smoothed_bands[band]) * smoothing;
            bands[band] = self.smoothed_bands[band].clamp(0.0, 1.0);
        }

        let spectrum = self.fine_spectrum(&spectrum[..fft_len / 2], hz_per_bin);

        AudioFeatures {
            timestamp: self.started_at.elapsed(),
            smoothed_amplitude: self.smoothed_amplitude,
            band_energies: bands,
            spectrum,
            is_usable: amplitude >= self.silence_threshold,
        }
    }

    /// Groups FFT bins into [`SPECTRUM_BINS`] log-spaced bins with a shared auto-gain, so the
    /// relative loudness between frequencies is preserved.
    fn fine_spectrum(&mut self, half: &[Complex<f32>], hz_per_bin: f32) -> [f32; SPECTRUM_BINS] {
        let ratio = SPECTRUM_MAX_HZ / SPECTRUM_MIN_HZ;
        let mut raw = [0.0f32; SPECTRUM_BINS];
        for (bin, value) in raw.iter_mut().enumerate() {
            let lo = SPECTRUM_MIN_HZ * ratio.powf(bin as f32 / SPECTRUM_BINS as f32);
            let hi = SPECTRUM_MIN_HZ * ratio.powf((bin + 1) as f32 / SPECTRUM_BINS as f32);
            let first = ((lo / hz_per_bin) as usize).clamp(1, half.len().saturating_sub(1));
            let last = ((hi / hz_per_bin).ceil() as usize).clamp(first + 1, half.len());
            let slice = &half[first.min(last)..last];
            let peak = slice.iter().map(|c| c.norm()).fold(0.0f32, f32::max);
            // Tilt up the highs so treble is visible next to the naturally louder bass.
            let tilt = 1.0 + 3.0 * bin as f32 / SPECTRUM_BINS as f32;
            *value = if peak.is_finite() {
                peak.sqrt() * tilt
            } else {
                0.0
            };
        }

        let frame_peak = raw.iter().copied().fold(0.0f32, f32::max);
        self.spectrum_peak = (self.spectrum_peak * 0.998).max(frame_peak).max(1e-4);
        let mut out = [0.0; SPECTRUM_BINS];
        for bin in 0..SPECTRUM_BINS {
            let level = (raw[bin] / self.spectrum_peak).clamp(0.0, 1.0);
            let current = &mut self.smoothed_spectrum[bin];
            let smoothing = if level > *current { 0.6 } else { 0.15 };
            *current += (level - *current) * smoothing;
            out[bin] = current.clamp(0.0, 1.0);
        }
        out
    }
}

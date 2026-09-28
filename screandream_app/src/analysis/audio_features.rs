use rustfft::{FftPlanner, num_complex::Complex};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub struct AudioFeatures {
    pub timestamp: Duration,
    pub smoothed_amplitude: f32,
    pub band_energies: [f32; 4],
    pub is_usable: bool,
}

impl AudioFeatures {
    pub fn silent(timestamp: Duration) -> Self {
        Self {
            timestamp,
            smoothed_amplitude: 0.0,
            band_energies: [0.0; 4],
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

        AudioFeatures {
            timestamp: self.started_at.elapsed(),
            smoothed_amplitude: self.smoothed_amplitude,
            band_energies: bands,
            is_usable: amplitude >= self.silence_threshold,
        }
    }
}

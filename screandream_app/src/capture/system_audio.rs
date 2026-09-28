use crate::analysis::audio_features::{AudioFeatureAnalyzer, AudioFeatures};
use crate::session::AudioAccessState;
use std::collections::VecDeque;

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("system-audio access is not granted")]
    AccessNotGranted,
    #[error("system-audio capture is unavailable")]
    Unavailable,
}

#[derive(Debug)]
pub struct SystemAudioCapture {
    active: bool,
    samples: VecDeque<f32>,
    capacity: usize,
}

impl SystemAudioCapture {
    pub fn new(capacity: usize) -> Self {
        Self {
            active: false,
            samples: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn start(&mut self, access: AudioAccessState) -> Result<(), CaptureError> {
        match access {
            AudioAccessState::Granted => {
                self.active = true;
                Ok(())
            }
            AudioAccessState::Unavailable => Err(CaptureError::Unavailable),
            _ => Err(CaptureError::AccessNotGranted),
        }
    }

    pub fn stop(&mut self) {
        self.active = false;
        self.samples.clear();
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn ingest_live_samples(&mut self, samples: &[f32]) {
        if !self.active {
            return;
        }
        // Keep only the newest `capacity` samples, even when one batch exceeds it.
        let batch = &samples[samples.len().saturating_sub(self.capacity)..];
        let overflow = (self.samples.len() + batch.len()).saturating_sub(self.capacity);
        self.samples.drain(..overflow);
        self.samples.extend(batch.iter().copied());
    }

    /// The newest buffered samples (at most `capacity`), oldest first.
    pub fn latest_samples(&self) -> impl Iterator<Item = f32> + '_ {
        self.samples.iter().copied()
    }

    pub fn analyze_latest(&mut self, analyzer: &mut AudioFeatureAnalyzer) -> AudioFeatures {
        let samples: Vec<f32> = self.samples.drain(..).collect();
        analyzer.analyze(&samples)
    }
}

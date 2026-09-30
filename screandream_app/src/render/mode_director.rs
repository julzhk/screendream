//! Decides when the visualizer should switch to another mode, and which one. Kept free of
//! GPU types so the rules can be unit-tested; `render::modes` holds the modes themselves.
//!
//! A switch happens when a new song appears to start (audio resumes after a gap of silence
//! following a decent stretch of playback), when the current mode has run for
//! `max_duration`, or when asked for explicitly.

use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
pub struct DirectorConfig {
    /// Switch after this long even if no song change is detected.
    pub max_duration: Duration,
    /// Silence at least this long, followed by audio, counts as a new song.
    pub song_gap: Duration,
    /// Ignore song changes until the current mode has shown for at least this long.
    pub min_duration: Duration,
}

impl Default for DirectorConfig {
    fn default() -> Self {
        Self {
            max_duration: Duration::from_secs(5 * 60),
            song_gap: Duration::from_millis(1_500),
            min_duration: Duration::from_secs(30),
        }
    }
}

#[derive(Debug)]
pub struct ModeDirector {
    config: DirectorConfig,
    mode_count: usize,
    current: usize,
    switched_at: Instant,
    silent_since: Option<Instant>,
    rng: u64,
}

impl ModeDirector {
    pub fn new(mode_count: usize, first: usize, now: Instant, seed: u64) -> Self {
        Self::with_config(DirectorConfig::default(), mode_count, first, now, seed)
    }

    pub fn with_config(
        config: DirectorConfig,
        mode_count: usize,
        first: usize,
        now: Instant,
        seed: u64,
    ) -> Self {
        Self {
            config,
            mode_count: mode_count.max(1),
            current: first.min(mode_count.saturating_sub(1)),
            switched_at: now,
            silent_since: None,
            // xorshift must never be seeded with zero.
            rng: seed | 1,
        }
    }

    pub fn current(&self) -> usize {
        self.current
    }

    /// Call once per frame. Returns the new mode index when it is time to switch.
    pub fn tick(&mut self, now: Instant, audio_present: bool) -> Option<usize> {
        let mut new_song = false;
        if audio_present {
            if let Some(silent_since) = self.silent_since.take() {
                new_song = now.duration_since(silent_since) >= self.config.song_gap
                    && silent_since.duration_since(self.switched_at) >= self.config.min_duration;
            }
        } else if self.silent_since.is_none() {
            self.silent_since = Some(now);
        }

        let expired = now.duration_since(self.switched_at) >= self.config.max_duration;
        (new_song || expired).then(|| self.next(now))
    }

    /// Switches to a random mode other than the current one and returns its index.
    pub fn next(&mut self, now: Instant) -> usize {
        self.switched_at = now;
        if self.mode_count > 1 {
            let offset = 1 + (self.random() % (self.mode_count as u64 - 1)) as usize;
            self.current = (self.current + offset) % self.mode_count;
        }
        self.current
    }

    fn random(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }
}

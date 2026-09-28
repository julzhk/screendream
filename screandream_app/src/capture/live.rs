//! Live system-audio source. Samples are held only in a short in-memory ring and are
//! discarded as soon as they are analyzed; nothing is written to disk or transmitted.

use crate::session::AudioAccessState;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const RING_CAPACITY: usize = 16_384;

#[derive(Clone, Default)]
struct Shared {
    samples: Arc<Mutex<VecDeque<f32>>>,
    revoked: Arc<AtomicBool>,
}

impl Shared {
    #[cfg_attr(
        not(all(target_os = "macos", feature = "macos-capture")),
        allow(dead_code)
    )]
    fn push(&self, samples: impl IntoIterator<Item = f32>) {
        if let Ok(mut ring) = self.samples.lock() {
            ring.extend(samples);
            let overflow = ring.len().saturating_sub(RING_CAPACITY);
            ring.drain(..overflow);
        }
    }
}

pub struct LiveAudio {
    shared: Shared,
    #[cfg(all(target_os = "macos", feature = "macos-capture"))]
    stream: Option<screencapturekit::prelude::SCStream>,
}

impl LiveAudio {
    /// Moves every buffered sample into `out`, clearing the ring.
    pub fn drain_into(&self, out: &mut Vec<f32>) {
        if let Ok(mut ring) = self.shared.samples.lock() {
            out.extend(ring.drain(..));
        }
    }

    /// True once the OS has stopped the stream (e.g. access was revoked).
    pub fn was_revoked(&self) -> bool {
        self.shared.revoked.load(Ordering::Relaxed)
    }

    pub fn sample_rate(&self) -> u32 {
        48_000
    }
}

#[cfg(all(target_os = "macos", feature = "macos-capture"))]
mod platform {
    use super::*;
    use screencapturekit::prelude::*;

    struct Delegate(Arc<AtomicBool>);

    impl SCStreamDelegateTrait for Delegate {
        fn did_stop_with_error(&self, error: SCError) {
            eprintln!("Screendream: audio capture stopped: {error}");
            self.0.store(true, Ordering::Relaxed);
        }
    }

    impl LiveAudio {
        /// Requests Screen & System Audio Recording access (macOS prompts on first use) and
        /// starts an audio-only ScreenCaptureKit stream.
        pub fn start() -> Result<Self, AudioAccessState> {
            let content = SCShareableContent::get().map_err(|error| {
                eprintln!("Screendream: shareable content unavailable: {error}");
                AudioAccessState::Denied
            })?;
            let display = content
                .displays()
                .into_iter()
                .next()
                .ok_or(AudioAccessState::Unavailable)?;
            let filter = SCContentFilter::create()
                .with_display(&display)
                .with_excluding_windows(&[])
                .build();
            let config = SCStreamConfiguration::new()
                .with_width(2)
                .with_height(2)
                .with_minimum_frame_interval(&CMTime::new(1, 1))
                .with_captures_audio(true)
                .with_excludes_current_process_audio(true)
                .with_sample_rate(48_000)
                .with_channel_count(2);

            let shared = Shared::default();
            let mut stream =
                SCStream::new_with_delegate(&filter, &config, Delegate(shared.revoked.clone()));
            let sink = shared.clone();
            stream.add_output_handler(
                move |sample: CMSampleBuffer, of_type: SCStreamOutputType| {
                    if of_type != SCStreamOutputType::Audio {
                        return;
                    }
                    let Some(list) = sample.audio_buffer_list() else {
                        return;
                    };
                    // ScreenCaptureKit delivers non-interleaved f32 PCM: one buffer per channel.
                    let channels: Vec<&[f32]> = list
                        .iter()
                        .map(|buffer| bytemuck::try_cast_slice(buffer.data()).unwrap_or(&[]))
                        .collect();
                    let Some(frames) = channels.iter().map(|c| c.len()).min() else {
                        return;
                    };
                    let scale = 1.0 / channels.len() as f32;
                    sink.push(
                        (0..frames).map(|i| channels.iter().map(|c| c[i]).sum::<f32>() * scale),
                    );
                },
                SCStreamOutputType::Audio,
            );
            // Video frames are unused, but a screen handler silences "output not found" logs.
            stream.add_output_handler(
                |_: CMSampleBuffer, _: SCStreamOutputType| {},
                SCStreamOutputType::Screen,
            );
            stream.start_capture().map_err(|error| {
                eprintln!("Screendream: could not start capture: {error}");
                AudioAccessState::Denied
            })?;
            Ok(Self {
                shared,
                stream: Some(stream),
            })
        }

        pub fn stop(&mut self) {
            if let Some(stream) = self.stream.take() {
                let _ = stream.stop_capture();
            }
            if let Ok(mut ring) = self.shared.samples.lock() {
                ring.clear();
            }
        }
    }
}

#[cfg(not(all(target_os = "macos", feature = "macos-capture")))]
impl LiveAudio {
    pub fn start() -> Result<Self, AudioAccessState> {
        Err(AudioAccessState::Unavailable)
    }

    pub fn stop(&mut self) {}
}

impl Drop for LiveAudio {
    fn drop(&mut self) {
        self.stop();
    }
}

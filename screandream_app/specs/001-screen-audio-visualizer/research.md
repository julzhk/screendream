# Research: Speaker-Reactive Screen Visualizer

## Rust system-audio capture and authorization

**Decision**: Use the Rust `screencapturekit` bindings to receive macOS system-audio sample buffers
after the listener grants Screen & System Audio Recording access. Target macOS 14+ and exclude the
visualizer's own process audio where the binding and platform support it.

**Rationale**: ScreenCaptureKit is the macOS-supported path for capturing current system audio without
requiring a virtual audio driver. The `screencapturekit` crate provides Rust bindings to that platform
capability, so the capture boundary stays native to the chosen language. Audio buffers are passed
directly to analysis and discarded after each feature update.

**Alternatives considered**:

- **CPAL loopback capture**: Rejected because it does not provide native capture of all macOS system
  output without requiring a virtual audio device.
- **A virtual audio driver**: Rejected because installation and routing add support and privacy
  complexity.
- **Microphone capture**: Rejected because it captures room sound rather than the computer's speaker
  output.

Sources: [screencapturekit crate](https://docs.rs/screencapturekit/latest/screencapturekit/),
[screencapturekit-rs](https://github.com/doom-fish/screencapturekit-rs), and
[Apple ScreenCaptureKit documentation](https://developer.apple.com/documentation/screencapturekit).

## Rendering, shaders, and scene assets

**Decision**: Use `winit` for macOS window/full-screen events and `wgpu` for rendering a single
bundled PLY mesh with WGSL shaders. Use the requested `ply` crate (v0.1.0) to parse the application
owned `default-scene.ply` asset at startup. Feed smoothed amplitude and a small set of frequency-band
values to the shader each frame to control geometry deformation, color, and glow.

**Rationale**: A PLY mesh provides a compact, versioned visual base that can be authored and reviewed
with the codebase. GPU shaders make the mesh visibly responsive to music without frame-by-frame media
assets or a playback subsystem. The `ply` crate is purpose-built for reading and writing Polygon File
Format streams and fulfills the requested crate choice. A shader-driven scene is lower risk than a
video clip because it has no clip licensing, decode timing, or static visual loop.

**Alternatives considered**:

- **Video clips**: Rejected for v1 because static clips do not provide sufficient live response and
  introduce asset licensing, large bundle size, and decoding complexity.
- **Raster animation frames**: Rejected because they have the same static-content and bundle-size
  drawbacks as video.
- **Procedural geometry only**: Rejected because the requested PLY crate and a simple bundled mesh
  provide a clearer, artist-authored visual baseline.
- **Amplitude-only shader input**: Rejected because frequency-band values add meaningful variation
  across music while retaining a small analysis surface.

Sources: [ply crate](https://crates.io/crates/ply),
[wgpu documentation](https://docs.rs/wgpu/latest/wgpu/), and
[winit documentation](https://docs.rs/winit/latest/winit/).

## Audio analysis and lifecycle

**Decision**: Use `rustfft` to derive normalized frequency-band energy and a smoothed amplitude from
short in-memory sample windows. Model the application as the following explicit session states:
`idle`, `requesting_access`, `starting`, `active`, `audio_unavailable`, `access_denied`,
`access_revoked`, `stopping`, and `ended`.

**Rationale**: FFT-derived bands give the shader distinct musical controls, while smoothed amplitude
communicates loudness immediately. Explicit states distinguish recoverable silence from authorization
loss, ensure capture is stopped at terminal boundaries, and give the UI precise recovery guidance.

**Alternatives considered**:

- **Persistent waveform history**: Rejected because it is unnecessary for rendering and violates the
  no-retention requirement.
- **One generic error state**: Rejected because it cannot tell the listener how to recover.
- **Closing on silence**: Rejected because temporary silence and output changes must recover without
  restarting the session.

Sources: [rustfft documentation](https://docs.rs/rustfft/latest/rustfft/) and
[Rust Audio analysis guidance](https://github.com/RustAudio/rustfft).

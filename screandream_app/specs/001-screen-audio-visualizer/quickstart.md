# Quickstart: Validate the Speaker-Reactive Screen Visualizer

## Prerequisites

- A Mac running macOS 14 or later.
- Rust stable installed with the Apple Silicon or Intel macOS target matching the computer.
- A local audio source playing through the Mac's current speakers.
- A user account that can grant Screen & System Audio Recording access.

## Build and launch

From the repository root, build and launch the app:

```sh
cargo run
```

The first launch must explain why live speaker-audio access is needed before directing the listener to
grant it. The default PLY mesh and WGSL shader assets must be bundled with the application and load
before capture can start.

## Automated validation

Run all static and unit checks:

```sh
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

Expected coverage:

- Session state transitions, including stop and revoked-access teardown.
- Normalization and smoothing boundaries for derived audio features.
- PLY mesh parsing and rejection of invalid or empty bundled mesh data.
- Shader asset load and validation errors.

## Manual end-to-end validation

1. Start music through the current speakers and choose **Start Visualizer**. Confirm full-screen
   animation starts in under 30 seconds and visible intensity changes follow louder and quieter
   passages within one second.
2. Confirm the bundled mesh visibly deforms or changes color/glow with both overall amplitude and
   different frequency ranges; no video clip or media playback is required.
3. Stop the visualizer. Confirm full-screen view closes and that no further capture indicator or
   audio-reactive movement remains.
4. Deny Screen & System Audio Recording access, retry starting, and confirm the app explains how to
   restore access without beginning capture.
5. Grant access, start with no speaker audio, and confirm the non-blocking “Audio not detected”
   status appears. Start music and confirm it clears without restarting.
6. Exit full-screen through a system control. Confirm the app offers **Continue Windowed** and
   **End Session**, and verify each behavior in
   [visualizer-ui.md](contracts/visualizer-ui.md).

## Privacy verification

During and after a session, inspect application support locations and outbound network activity.
There must be no raw audio files, waveform history, video recordings, or outbound audio transmission.
Refer to [data-model.md](data-model.md) for the permitted in-memory entities and bundled visual assets.

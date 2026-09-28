# Implementation Plan: Speaker-Reactive Screen Visualizer

**Branch**: `001-screen-audio-visualizer` | **Date**: 2026-09-24 | **Spec**:
[spec.md](spec.md)

**Input**: Feature specification from
`/specs/001-screen-audio-visualizer/spec.md`; planning direction: Rust, the `ply` crate, and visual
assets where appropriate.

## Summary

Deliver a Rust macOS desktop application that captures authorized live system audio, derives
short-lived intensity and frequency-band features, and drives a full-screen GPU visualizer. The
default scene will load a bundled PLY mesh using the requested `ply` crate and animate it with
custom GPU shaders. The design deliberately avoids video clips in v1 because they add large,
non-reactive licensed assets without improving the core live-music response.

## Technical Context

**Language/Version**: Rust 2024 edition, stable toolchain

**Primary Dependencies**: `screencapturekit` for macOS system-audio capture; `winit` for native
window and full-screen events; `wgpu` for GPU rendering and shaders; `rustfft` for frequency-band
analysis; `ply` for loading the bundled PLY scene mesh; `pollster` for application startup

**Storage**: N/A; source audio, analysis windows, and render control values remain in memory.
Bundled PLY geometry and WGSL shader source are read-only application assets.

**Testing**: `cargo test` for session and analysis logic; `cargo fmt --check` and `cargo clippy
-- -D warnings` as quality gates; manual macOS integration validation for permission, system-audio
capture, full-screen behavior, and visual response

**Target Platform**: macOS 14 or later

**Project Type**: native desktop application

**Performance Goals**: Sustain 60 frames per second on supported hardware; update visual response
within one second of a meaningful change in speaker-audio intensity

**Constraints**: Capture only current live system audio after user authorization; never store,
transmit, or record audio; stop capture when the session ends; exclude the application's own audio
where the platform supports it; bundle one license-cleared PLY mesh and authored shader source

**Scale/Scope**: One local listener, one current speaker output, one full-screen visualizer scene,
and one active session. Microphone input, saved media, video clips, visual-theme selection, sharing,
and persistent preferences are out of scope.

## Constitution Check

**Pre-design status: PASS**

- **User value first**: The plan delivers a live visual accompaniment. The PLY mesh and shaders serve
  that core response; unrelated asset libraries and video playback are excluded.
- **Clear, testable behavior**: Session states, audio analysis bounds, full-screen behavior, and UI
  recovery are independently testable; platform permission behavior is covered manually.
- **Quality at boundaries**: The design defines explicit handling for denied or revoked access,
  silence, speaker-output changes, asset-load failure, and session teardown.
- **Security and privacy**: Raw audio is never persisted or transmitted. Packaged assets are static,
  license-cleared, and validated before rendering.
- **Simplicity and maintainability**: A single Rust process uses supported macOS capture bindings,
  one PLY asset, and a small shader set; it introduces no server, database, or media pipeline.

## Project Structure

### Documentation (this feature)

```text
specs/001-screen-audio-visualizer/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── visualizer-ui.md
└── tasks.md              # Created by /speckit-tasks
```

### Source Code (repository root)

```text
Cargo.toml
src/
├── main.rs
├── app.rs
├── capture/
│   ├── authorization.rs
│   └── system_audio.rs
├── analysis/
│   └── audio_features.rs
├── session/
│   └── visualizer_session.rs
├── render/
│   ├── renderer.rs
│   ├── scene_mesh.rs
│   └── shaders/
│       ├── visualizer.wgsl
│       └── post_process.wgsl
└── ui/
    ├── start_view.rs
    ├── status_overlay.rs
    └── full_screen_exit.rs

assets/
└── default-scene.ply

tests/
├── audio_features.rs
├── session_states.rs
└── scene_mesh.rs
```

**Structure Decision**: Use a single Rust binary. Isolate platform capture, pure audio analysis,
session state, and GPU rendering to make lifecycle logic testable while keeping platform-specific
code at the capture boundary. Bundle one PLY mesh and authored WGSL shaders as application assets.

## Complexity Tracking

No constitution violations require justification.

## Constitution Check

**Post-design status: PASS**

The completed design limits visual assets to a validated local PLY mesh and authored shaders. It
retains no audio or derived history after a session ends, contains platform capture in one module,
and defines manual validation for the macOS authorization boundary.

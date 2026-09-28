# Tasks: Speaker-Reactive Screen Visualizer

**Input**: Design documents from `/specs/001-screen-audio-visualizer/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/visualizer-ui.md,
quickstart.md

**Tests**: Unit and integration tests are included because the implementation plan requires `cargo
test` coverage for session, analysis, asset-loading, and UI-flow behavior.

**Organization**: Tasks are grouped by user story so each increment can be implemented and verified
independently.

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Create the Rust application, build configuration, and static visual assets.

- [X] T001 Create the Rust 2024 binary package and declare `screencapturekit`, `winit`, `wgpu`, `rustfft`, `ply`, and `pollster` dependencies in `Cargo.toml`
- [X] T002 Create the module layout and crate entry points in `src/main.rs`, `src/app.rs`, `src/capture/mod.rs`, `src/analysis/mod.rs`, `src/session/mod.rs`, `src/render/mod.rs`, and `src/ui/mod.rs`
- [X] T003 [P] Add a license-cleared default scene mesh in `assets/default-scene.ply`
- [X] T004 [P] Configure formatting and warning-free linting for the Rust project in `rustfmt.toml` and `Cargo.toml`
- [X] T005 Configure the macOS application bundle identifier and Screen & System Audio Recording usage description in `Cargo.toml` and `resources/Info.plist`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Build the shared state, analysis, asset, and rendering infrastructure required by every
user story.

**⚠️ CRITICAL**: Complete this phase before beginning user-story work.

- [X] T006 Implement `VisualizerSession`, `AudioAccessState`, `FullScreenExitChoice`, and all documented state transitions in `src/session/visualizer_session.rs`
- [X] T007 [P] Add state-transition tests in `tests/session_states.rs`, including the rule that `access_denied` and `access_revoked` MUST have no active audio capture and `ended` MUST release capture, analysis, and rendering resources
- [X] T008 Implement ephemeral `AudioFeatures` calculation with normalized `smoothed_amplitude`, normalized `band_energies`, monotonic `timestamp`, and `is_usable` in `src/analysis/audio_features.rs`
- [X] T009 [P] Add audio-feature boundary tests in `tests/audio_features.rs`, asserting every normalized value remains within 0.0 to 1.0 and unusable input is reported without retaining samples
- [ ] T010 Implement PLY scene loading in `src/render/scene_mesh.rs`, enforcing that `mesh_path` resolves inside `assets/` and that `vertex_count` is greater than zero and within the renderer limit
- [X] T011 [P] Add PLY asset tests in `tests/scene_mesh.rs` for valid bundled mesh parsing and invalid, empty, and out-of-assets-path rejection
- [ ] T012 Implement GPU device initialization, shader validation, and asset-load error propagation in `src/render/renderer.rs`, `src/render/shaders/visualizer.wgsl`, and `src/render/shaders/post_process.wgsl`
- [ ] T013 Implement the application event loop and shared session-to-renderer message wiring in `src/app.rs`

**Checkpoint**: A window can initialize the renderer from the bundled mesh, and all pure state,
analysis, mesh, and shader tests pass without speaker-audio access.

---

## Phase 3: User Story 1 - Start a Music-Reactive Visualizer (Priority: P1) 🎯 MVP

**Goal**: Let a listener with granted access start a full-screen shader-driven PLY visualizer that
reacts to live speaker audio and can be stopped cleanly.

**Independent Test**: With Screen & System Audio Recording access granted and music playing through
the selected speakers, start the visualizer, observe full-screen visual changes within one second of
loudness changes, then stop it and confirm capture ends.

### Tests for User Story 1

- [X] T014 [P] [US1] Add start/stop integration tests with a granted-audio-capture test double in `tests/visualizer_start_stop.rs`
- [X] T015 [P] [US1] Add renderer tests proving amplitude and frequency-band inputs alter shader uniform data in `tests/renderer_audio_response.rs`

### Implementation for User Story 1

- [ ] T016 [US1] Implement authorized live system-audio stream setup and immediate teardown in `src/capture/system_audio.rs`
- [X] T017 [US1] Implement a bounded, non-persistent audio-sample handoff from `src/capture/system_audio.rs` to `src/analysis/audio_features.rs`
- [ ] T018 [US1] Implement per-frame mesh deformation, color, and glow controls from `AudioFeatures` in `src/render/renderer.rs` and `src/render/shaders/visualizer.wgsl`
- [ ] T019 [US1] Implement the start action, visible stop control, and full-screen entry in `src/ui/start_view.rs` and `src/app.rs`
- [ ] T020 [US1] Connect start, active rendering, and stop events through `VisualizerSession` in `src/app.rs` and `src/session/visualizer_session.rs`

**Checkpoint**: User Story 1 is independently demonstrable: authorized live music drives the
full-screen PLY scene, and stopping closes the view and ends capture.

---

## Phase 4: User Story 2 - Grant Audio Access (Priority: P2)

**Goal**: Explain, request, and recover system-audio authorization without starting capture until
access is granted.

**Independent Test**: Deny access, start the visualizer, confirm the recovery guidance appears and
no capture begins; restore access through System Settings, retry, and start the visualizer.

### Tests for User Story 2

- [X] T021 [P] [US2] Add authorization-state tests for `not_determined`, `denied`, `revoked`, and `unavailable` outcomes in `tests/audio_authorization.rs`
- [ ] T022 [P] [US2] Add UI-flow tests for permission explanation, settings recovery, and retry behavior in `tests/authorization_recovery.rs`

### Implementation for User Story 2

- [ ] T023 [US2] Implement authorization discovery, request, revoke detection, and System Settings recovery actions in `src/capture/authorization.rs`
- [ ] T024 [US2] Implement the permission explanation and access-recovery controls required by the UI contract in `src/ui/start_view.rs`
- [ ] T025 [US2] Enforce no-capture behavior for `denied`, `revoked`, and `unavailable` access states in `src/capture/system_audio.rs` and `src/session/visualizer_session.rs`
- [ ] T026 [US2] Wire authorization outcomes and retry transitions into the application event loop in `src/app.rs`

**Checkpoint**: User Story 2 is independently testable: the application clearly explains required
access, offers recovery, and never captures audio until authorization is granted.

---

## Phase 5: User Story 3 - See Audio Availability Status (Priority: P3)

**Goal**: Clearly distinguish missing speaker audio from authorization failure, keep the visualizer
open during temporary silence, and resume automatically when usable audio returns.

**Independent Test**: With access granted, start the visualizer with no active speaker audio, confirm
the non-blocking status, then begin playback and confirm the status clears and visual response resumes
without restarting.

### Tests for User Story 3

- [X] T027 [P] [US3] Add session tests for `active -> audio_unavailable -> active` recovery without a restart in `tests/audio_availability.rs`
- [X] T028 [P] [US3] Add status-overlay tests for unavailable, restored, and revoked-access messages in `tests/status_overlay.rs`

### Implementation for User Story 3

- [ ] T029 [US3] Implement usable-signal detection and silence/output-interruption transitions in `src/analysis/audio_features.rs`
- [ ] T030 [US3] Implement the non-blocking “Audio not detected” status and clear-on-recovery behavior in `src/ui/status_overlay.rs`
- [ ] T031 [US3] Keep rendering active during `audio_unavailable`, resume live feature updates on recovery, and stop capture on revocation in `src/app.rs` and `src/session/visualizer_session.rs`

**Checkpoint**: All user stories are independently functional: listeners understand unavailable
audio, the app resumes automatically when music returns, and revoked access stops capture.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Validate performance, privacy, user-facing full-screen behavior, and the documented
end-to-end flow.

- [ ] T032 [P] Add frame-time instrumentation and a 60-frames-per-second performance check in `src/render/renderer.rs` and `tests/renderer_performance.rs`
- [X] T033 [P] Add full-screen exit-choice tests for `continue_windowed` and `end_session` in `tests/full_screen_exit.rs`
- [ ] T034 Implement the full-screen exit prompt and explicit listener choices in `src/ui/full_screen_exit.rs` and `src/app.rs`
- [ ] T035 Audit capture, analysis, logging, and asset paths to verify no audio is retained, exported, transmitted, or recorded in `src/capture/system_audio.rs`, `src/analysis/audio_features.rs`, and `src/app.rs`
- [ ] T036 Run `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, and every manual scenario in `specs/001-screen-audio-visualizer/quickstart.md`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies.
- **Foundational (Phase 2)**: Depends on Setup; blocks all user stories.
- **User Story 1 (Phase 3)**: Depends on Foundational and delivers the MVP.
- **User Story 2 (Phase 4)**: Depends on Foundational and integrates its authorization transitions
  with US1's capture flow.
- **User Story 3 (Phase 5)**: Depends on Foundational and integrates its availability transitions
  with US1's active rendering flow.
- **Polish (Phase 6)**: Depends on the desired user-story phases.

### User Story Dependencies

```text
Setup -> Foundational -> US1 (MVP) -> US2 -> US3 -> Polish
                        \-> US2 (shared capture integration)
                        \-> US3 (shared rendering integration)
```

US2 and US3 can begin their tests and isolated modules after Foundational, but final integration
requires the capture and rendering flow delivered by US1.

### Parallel Opportunities

- T003 and T004 can run in parallel after T001.
- T007, T009, and T011 can run in parallel with their distinct foundational implementation files.
- T014 and T015 can run in parallel for US1.
- T021 and T022 can run in parallel for US2.
- T027 and T028 can run in parallel for US3.
- T032 and T033 can run in parallel during Polish.

## Parallel Examples

### User Story 1

```text
Task: "Add start/stop integration tests in tests/visualizer_start_stop.rs"
Task: "Add renderer audio-response tests in tests/renderer_audio_response.rs"
```

### User Story 2

```text
Task: "Add authorization-state tests in tests/audio_authorization.rs"
Task: "Add permission-recovery UI-flow tests in tests/authorization_recovery.rs"
```

### User Story 3

```text
Task: "Add availability-recovery tests in tests/audio_availability.rs"
Task: "Add status-overlay tests in tests/status_overlay.rs"
```

## Implementation Strategy

### MVP First

1. Complete Phases 1 and 2.
2. Complete Phase 3 only.
3. Verify the US1 independent test with granted macOS audio access and active speaker playback.
4. Demonstrate the full-screen, shader-driven PLY visualizer before adding recovery scenarios.

### Incremental Delivery

1. Deliver US1 as the live visualizer MVP.
2. Add US2 to make the permission boundary self-service and privacy-safe.
3. Add US3 to communicate and recover from silence or output interruptions.
4. Complete cross-cutting performance, privacy, and quickstart validation.

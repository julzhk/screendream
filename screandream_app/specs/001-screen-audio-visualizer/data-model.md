# Data Model: Speaker-Reactive Screen Visualizer

## VisualizerSession

Represents one listener-controlled visualizer lifetime. It is in memory only and is destroyed when the
session ends.

| Field | Description | Validation |
|---|---|---|
| `id` | Identifier used only to associate in-memory collaborators for one session | Generated when starting; never persisted |
| `state` | Current lifecycle state | Must be one of the defined states below |
| `started_at` | Time the session became active | Present only after successful start |
| `audio_access_state` | Authorization and availability condition | Must match the current capture authorization result |
| `full_screen_state` | Whether the display is full-screen or the listener chose windowed continuation | Changes only from listener or system full-screen actions |

### State transitions

```text
idle -> requesting_access -> starting -> active
requesting_access -> access_denied
starting -> access_denied | access_revoked | ended
active -> audio_unavailable | stopping | access_revoked
audio_unavailable -> active | stopping | access_revoked
access_denied -> requesting_access | ended
access_revoked -> requesting_access | ended
stopping -> ended
```

- `active` requires granted access and usable live audio.
- `audio_unavailable` requires granted access but no usable audio signal.
- `access_denied` and `access_revoked` MUST have no active audio capture.
- `ended` MUST release capture, analysis, and rendering resources.

## AudioAccessState

Represents what the listener can do next; it does not retain any audio.

| Value | Meaning | Listener-facing behavior |
|---|---|---|
| `not_determined` | Access has not been requested | Explain why speaker audio is needed before prompting |
| `granted` | Live speaker audio may be captured | Start or resume capture |
| `denied` | Listener or system denied access | Explain how to enable access in System Settings |
| `revoked` | Access was removed during a session | Stop capture and explain how to restore access |
| `unavailable` | The environment cannot provide system-audio capture | Explain that audio-reactive mode is unavailable |

## AudioFeatures

An ephemeral analysis snapshot derived from one short portion of live system audio. It is sent to the
renderer and overwritten by the next snapshot.

| Field | Description | Validation |
|---|---|---|
| `timestamp` | Time the sample analysis was produced | Monotonic within one session |
| `smoothed_amplitude` | Normalized recent overall intensity | Value is within 0.0 to 1.0 |
| `band_energies` | Normalized energy for a fixed small set of frequency ranges | Every value is within 0.0 to 1.0 |
| `is_usable` | Whether the input contains a usable speaker-audio signal | `false` transitions toward `audio_unavailable` |

## SceneAsset

Represents a bundled, read-only visual asset. It contains no listener or audio data.

| Field | Description | Validation |
|---|---|---|
| `mesh_path` | Application-relative path to the default PLY mesh | Must resolve within the bundled `assets/` directory |
| `vertex_count` | Count parsed from the PLY mesh | Must be greater than zero and within the renderer's documented limit |
| `shader_sources` | Authored WGSL shader assets used for the scene | Every required shader must load and pass renderer validation |

The app MUST fail to a clear, non-capturing error state if the bundled mesh or shader cannot be loaded.
It MUST NOT load mesh or shader assets from arbitrary user-provided paths in v1.

## FullScreenExitChoice

| Value | Effect |
|---|---|
| `continue_windowed` | Keep the existing session and capture active outside full-screen |
| `end_session` | Stop capture and transition the session to `ended` |

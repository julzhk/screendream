# UI Contract: Visualizer Session

This contract defines the observable application behavior. It is not a network interface.

## Start view

| Condition | Required controls and feedback |
|---|---|
| `idle` | A **Start Visualizer** action and a concise explanation that the animation responds to live speaker audio |
| `not_determined` | An explanation that system-audio access is needed only for live response and is not recorded, plus an action that starts the authorization flow |
| `denied` or `revoked` | A recoverable explanation with an action to open the relevant System Settings location and an action to retry after access is restored |
| `unavailable` | A clear statement that audio-reactive mode cannot run in the current environment; the start action is unavailable |

## Active visualizer

| Event | Required outcome |
|---|---|
| Start succeeds | Enter full-screen view, show the animation, and provide a visible stop control |
| Usable audio is received | Render animation changes from the latest `AudioFeatures` values |
| No usable audio is received | Keep the visualizer visible and show a non-blocking “Audio not detected” status |
| Usable audio returns | Remove the unavailable status and resume responsive animation without a restart |
| Stop requested | Stop capture, close full-screen view, and return to the start view |
| Authorization is revoked | Stop capture immediately and show the access-recovery state |

## Full-screen exit

When the operating system exits full-screen mode while the session is active, the app MUST present two
explicit choices: **Continue Windowed** and **End Session**. It MUST NOT silently continue capture
without a listener choice.

//! What the on-screen overlay shows for a given session state. Kept free of GPU types so
//! the rules can be unit-tested; `render::overlay` turns this into pixels.

use crate::capture::authorization;
use crate::session::{AudioAccessState, SessionState, VisualizerSession};
use crate::ui::{full_screen_exit, start_view, status_overlay};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tone {
    Info,
    Warning,
}

/// A small, non-blocking pill at the top of the screen.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Status {
    pub text: String,
    pub tone: Tone,
}

/// A centred panel with a heading, explanation and the keys that act on it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dialog {
    pub title: String,
    pub body: String,
    pub actions: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OverlayContent {
    pub status: Option<Status>,
    pub dialog: Option<Dialog>,
    /// Key hints along the bottom edge; includes the visible stop control while active.
    pub hint: Option<String>,
}

pub const ACTIVE_HINT: &str =
    "Space  Stop    ·    M  Next mode    ·    Esc  Leave full-screen    ·    Q  Quit";

pub fn content(
    session: &VisualizerSession,
    awaiting_exit_choice: bool,
    show_hint: bool,
) -> OverlayContent {
    if awaiting_exit_choice {
        return OverlayContent {
            dialog: Some(Dialog {
                title: full_screen_exit::PROMPT.to_string(),
                body: "Audio response is paused until you choose.".to_string(),
                actions: "Enter  Continue Windowed    ·    Q  End Session".to_string(),
            }),
            ..OverlayContent::default()
        };
    }

    match session.state {
        SessionState::Active | SessionState::AudioUnavailable => OverlayContent {
            status: status_overlay::message(session.state).map(|text| Status {
                text: format!("{text} — play something to bring the visualizer to life"),
                tone: Tone::Warning,
            }),
            dialog: None,
            hint: show_hint.then(|| ACTIVE_HINT.to_string()),
        },
        SessionState::AccessDenied | SessionState::AccessRevoked => {
            let access = session.audio_access_state;
            let unavailable = access == AudioAccessState::Unavailable;
            OverlayContent {
                dialog: Some(Dialog {
                    title: if unavailable {
                        "Audio-reactive mode can't run here".to_string()
                    } else {
                        "Audio access is required".to_string()
                    },
                    body: authorization::recovery_message(access).to_string(),
                    actions: if unavailable {
                        "Q  Quit".to_string()
                    } else {
                        "S  Open System Settings    ·    Space  Retry    ·    Q  Quit".to_string()
                    },
                }),
                ..OverlayContent::default()
            }
        }
        SessionState::RequestingAccess | SessionState::Starting => OverlayContent {
            status: Some(Status {
                text: start_view::status_for_access(AudioAccessState::NotDetermined).to_string(),
                tone: Tone::Info,
            }),
            ..OverlayContent::default()
        },
        SessionState::Idle | SessionState::Stopping | SessionState::Ended => OverlayContent {
            dialog: Some(Dialog {
                title: "Screendream".to_string(),
                body: "The animation responds to live audio playing through this Mac's \
                       speakers. Audio is never recorded."
                    .to_string(),
                actions: format!("Space  {}    ·    Q  Quit", start_view::start_label()),
            }),
            ..OverlayContent::default()
        },
    }
}

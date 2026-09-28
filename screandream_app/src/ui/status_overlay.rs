use crate::session::SessionState;

pub fn message(state: SessionState) -> Option<&'static str> {
    match state {
        SessionState::AudioUnavailable => Some("Audio not detected"),
        SessionState::AccessDenied | SessionState::AccessRevoked => {
            Some("Audio access is required. Open System Settings and retry.")
        }
        _ => None,
    }
}

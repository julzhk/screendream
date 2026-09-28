use crate::session::AudioAccessState;

pub fn recovery_message(state: AudioAccessState) -> &'static str {
    match state {
        AudioAccessState::NotDetermined => {
            "Screendream needs Screen & System Audio Recording access for live response only. Audio is not recorded."
        }
        AudioAccessState::Denied | AudioAccessState::Revoked => {
            "Allow Screendream in System Settings > Privacy & Security > Screen & System Audio Recording, then retry."
        }
        AudioAccessState::Unavailable => "System-audio capture is unavailable in this environment.",
        AudioAccessState::Granted => "Audio access is ready.",
    }
}

pub fn settings_url() -> &'static str {
    "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
}

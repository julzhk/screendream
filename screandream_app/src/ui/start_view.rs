use crate::capture::authorization::recovery_message;
use crate::session::AudioAccessState;

pub fn start_label() -> &'static str {
    "Start Visualizer"
}

pub fn status_for_access(access: AudioAccessState) -> &'static str {
    recovery_message(access)
}

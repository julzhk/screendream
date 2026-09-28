use crate::session::FullScreenExitChoice;

pub const PROMPT: &str = "Continue visualizer outside full-screen?";

pub fn choices() -> [FullScreenExitChoice; 2] {
    [
        FullScreenExitChoice::ContinueWindowed,
        FullScreenExitChoice::EndSession,
    ]
}

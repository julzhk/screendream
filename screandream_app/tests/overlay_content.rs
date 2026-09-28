use screendream::{
    session::{AudioAccessState, VisualizerSession},
    ui::overlay::{ACTIVE_HINT, Tone, content},
};

fn active_session() -> VisualizerSession {
    let mut session = VisualizerSession::default();
    session.set_access(AudioAccessState::Granted);
    session.activate();
    session
}

#[test]
fn active_session_shows_only_the_stop_hint() {
    let overlay = content(&active_session(), false, true);
    assert_eq!(overlay.status, None);
    assert_eq!(overlay.dialog, None);
    assert_eq!(overlay.hint.as_deref(), Some(ACTIVE_HINT));

    assert_eq!(content(&active_session(), false, false).hint, None);
}

#[test]
fn silence_shows_a_non_blocking_warning() {
    let mut session = active_session();
    session.set_audio_available(false);
    let overlay = content(&session, false, false);
    let status = overlay.status.expect("status pill");
    assert!(status.text.starts_with("Audio not detected"));
    assert_eq!(status.tone, Tone::Warning);
    assert_eq!(
        overlay.dialog, None,
        "silence must not block the visualizer"
    );
}

#[test]
fn denied_access_explains_recovery() {
    let mut session = VisualizerSession::default();
    session.set_access(AudioAccessState::Denied);
    let dialog = content(&session, false, true).dialog.expect("dialog");
    assert!(dialog.body.contains("System Settings"));
    assert!(dialog.actions.contains("Open System Settings"));
    assert!(dialog.actions.contains("Retry"));
}

#[test]
fn unavailable_capture_offers_no_retry() {
    let mut session = VisualizerSession::default();
    session.set_access(AudioAccessState::Unavailable);
    let dialog = content(&session, false, true).dialog.expect("dialog");
    assert!(!dialog.actions.contains("Retry"));
}

#[test]
fn leaving_full_screen_asks_for_an_explicit_choice() {
    let dialog = content(&active_session(), true, true)
        .dialog
        .expect("dialog");
    assert!(dialog.actions.contains("Continue Windowed"));
    assert!(dialog.actions.contains("End Session"));
}

#[test]
fn stopped_session_offers_start() {
    let mut session = active_session();
    session.stop();
    let dialog = content(&session, false, true).dialog.expect("dialog");
    assert!(dialog.actions.contains("Start Visualizer"));
}

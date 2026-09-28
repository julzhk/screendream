use screendream::session::{AudioAccessState, SessionState, VisualizerSession};

#[test]
fn revoked_access_ends_active_capture_state() {
    let mut session = VisualizerSession::default();
    session.set_access(AudioAccessState::Granted);
    session.activate();
    session.revoke_access();

    assert_eq!(session.state, SessionState::AccessRevoked);
    assert!(!session.full_screen);
}

#[test]
fn stopping_releases_the_session() {
    let mut session = VisualizerSession::default();
    session.set_access(AudioAccessState::Granted);
    session.activate();
    session.stop();

    assert_eq!(session.state, SessionState::Ended);
    assert!(!session.full_screen);
}

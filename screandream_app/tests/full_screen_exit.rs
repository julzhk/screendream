use screendream::session::{
    AudioAccessState, FullScreenExitChoice, SessionState, VisualizerSession,
};

#[test]
fn full_screen_exit_requires_an_explicit_choice() {
    let mut session = VisualizerSession::default();
    session.set_access(AudioAccessState::Granted);
    session.activate();
    session.exit_full_screen(FullScreenExitChoice::ContinueWindowed);
    assert_eq!(session.state, SessionState::Active);
    assert!(!session.full_screen);

    session.exit_full_screen(FullScreenExitChoice::EndSession);
    assert_eq!(session.state, SessionState::Ended);
}

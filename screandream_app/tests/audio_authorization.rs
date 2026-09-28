use screendream::{
    capture::authorization::recovery_message,
    session::{AudioAccessState, SessionState, VisualizerSession},
};

#[test]
fn denied_and_unavailable_access_do_not_activate_a_session() {
    for access in [AudioAccessState::Denied, AudioAccessState::Unavailable] {
        let mut session = VisualizerSession::default();
        session.set_access(access);
        session.activate();
        assert_eq!(session.state, SessionState::AccessDenied);
        assert!(!session.full_screen);
        assert!(!recovery_message(access).is_empty());
    }
}

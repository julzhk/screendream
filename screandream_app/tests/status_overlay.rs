use screendream::{session::SessionState, ui::status_overlay::message};

#[test]
fn availability_and_access_states_have_clear_messages() {
    assert_eq!(
        message(SessionState::AudioUnavailable),
        Some("Audio not detected")
    );
    assert!(message(SessionState::AccessRevoked).is_some());
    assert_eq!(message(SessionState::Active), None);
}

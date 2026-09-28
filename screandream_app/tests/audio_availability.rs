use screendream::session::{AudioAccessState, SessionState, VisualizerSession};

#[test]
fn silence_recovers_without_restarting() {
    let mut session = VisualizerSession::default();
    session.set_access(AudioAccessState::Granted);
    session.activate();
    session.set_audio_available(false);
    assert_eq!(session.state, SessionState::AudioUnavailable);

    session.set_audio_available(true);
    assert_eq!(session.state, SessionState::Active);
}

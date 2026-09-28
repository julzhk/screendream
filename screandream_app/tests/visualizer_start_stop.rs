use screendream::{
    capture::system_audio::SystemAudioCapture,
    session::{AudioAccessState, VisualizerSession},
};

#[test]
fn granted_access_starts_and_stopping_ends_capture() {
    let mut session = VisualizerSession::default();
    session.set_access(AudioAccessState::Granted);
    let mut capture = SystemAudioCapture::new(16);

    capture.start(session.audio_access_state).unwrap();
    session.activate();
    assert!(capture.is_active());

    capture.stop();
    session.stop();
    assert!(!capture.is_active());
}

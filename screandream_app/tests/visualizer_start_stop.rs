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

#[test]
fn capture_keeps_only_the_newest_samples_when_a_batch_exceeds_capacity() {
    let mut capture = SystemAudioCapture::new(4);
    capture.start(AudioAccessState::Granted).unwrap();

    capture.ingest_live_samples(&[1.0, 2.0]);
    capture.ingest_live_samples(&[3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
    assert_eq!(
        capture.latest_samples().collect::<Vec<_>>(),
        [5.0, 6.0, 7.0, 8.0]
    );

    capture.ingest_live_samples(&[9.0]);
    assert_eq!(
        capture.latest_samples().collect::<Vec<_>>(),
        [6.0, 7.0, 8.0, 9.0]
    );
}

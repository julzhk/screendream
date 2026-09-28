use screendream::analysis::audio_features::AudioFeatureAnalyzer;

#[test]
fn features_are_normalized_and_do_not_retain_empty_input() {
    let mut analyzer = AudioFeatureAnalyzer::default();
    let silent = analyzer.analyze(&[]);
    assert!(!silent.is_usable);
    assert_eq!(silent.smoothed_amplitude, 0.0);

    let features = analyzer.analyze(&[1.0, -1.0, 0.5, -0.5]);
    assert!(features.smoothed_amplitude.is_finite());
    assert!((0.0..=1.0).contains(&features.smoothed_amplitude));
    assert!(
        features
            .band_energies
            .iter()
            .all(|band| (0.0..=1.0).contains(band))
    );
}

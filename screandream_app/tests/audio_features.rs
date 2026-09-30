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
    assert!(
        features
            .spectrum
            .iter()
            .all(|bin| (0.0..=1.0).contains(bin))
    );
}

#[test]
fn spectrum_peaks_near_the_played_frequency() {
    let rate = 48_000.0;
    let tone: Vec<f32> = (0..2048)
        .map(|i| (std::f32::consts::TAU * 1_000.0 * i as f32 / rate).sin() * 0.5)
        .collect();
    let mut analyzer = AudioFeatureAnalyzer::default();
    let mut features = analyzer.analyze(&tone);
    for _ in 0..10 {
        features = analyzer.analyze(&tone);
    }
    let loudest = features
        .spectrum
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(bin, _)| bin)
        .unwrap();
    // 1 kHz sits ~56% of the way along the 30 Hz → 16 kHz log axis.
    assert!((30..=42).contains(&loudest), "loudest bin {loudest}");
}

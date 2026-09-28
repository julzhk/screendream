use screendream::{analysis::audio_features::AudioFeatures, render::renderer::Renderer};
use std::time::Duration;

#[test]
fn renderer_uniforms_follow_audio_features() {
    let mut renderer = Renderer::new().unwrap();
    renderer.update(&AudioFeatures {
        timestamp: Duration::ZERO,
        smoothed_amplitude: 0.8,
        band_energies: [0.1, 0.2, 0.6, 1.0],
        is_usable: true,
    });

    assert_eq!(renderer.uniform.amplitude, 0.8);
    assert_eq!(renderer.uniform.bands, [0.1, 0.2, 0.6, 1.0]);
    assert_eq!(renderer.uniform.glow, 0.8);
}

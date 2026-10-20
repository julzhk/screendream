use crate::analysis::audio_features::{AudioFeatures, SPECTRUM_BINS};

#[derive(Clone, Debug, PartialEq)]
pub struct VisualizerUniform {
    pub amplitude: f32,
    pub bands: [f32; 4],
    pub glow: f32,
    pub spectrum: [f32; SPECTRUM_BINS],
}

#[derive(Debug)]
pub struct Renderer {
    pub uniform: VisualizerUniform,
}

impl Renderer {
    pub fn validate_shaders() -> Result<(), &'static str> {
        for shader in [
            include_str!("shaders/visualizer.wgsl"),
            include_str!("shaders/post_process.wgsl"),
            include_str!("shaders/canyon.wgsl"),
            include_str!("shaders/windmill.wgsl"),
            include_str!("shaders/circuits.wgsl"),
            include_str!("shaders/bubbles.wgsl"),
            include_str!("shaders/automata.wgsl"),
            include_str!("shaders/neurons.wgsl"),
            include_str!("shaders/clockwork.wgsl"),
            include_str!("shaders/beacons.wgsl"),
        ] {
            if !shader.contains("@vertex") || !shader.contains("@fragment") {
                return Err("shader is missing a required entry point");
            }
        }
        Ok(())
    }

    pub fn new() -> Result<Self, &'static str> {
        Self::validate_shaders()?;
        Ok(Self {
            uniform: VisualizerUniform {
                amplitude: 0.0,
                bands: [0.0; 4],
                glow: 0.0,
                spectrum: [0.0; SPECTRUM_BINS],
            },
        })
    }

    pub fn update(&mut self, features: &AudioFeatures) {
        self.uniform.amplitude = features.smoothed_amplitude;
        self.uniform.bands = features.band_energies;
        self.uniform.spectrum = features.spectrum;
        self.uniform.glow =
            ((features.band_energies[2] + features.band_energies[3]) * 0.5).clamp(0.0, 1.0);
    }
}

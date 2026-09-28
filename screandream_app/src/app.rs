use crate::{
    analysis::audio_features::AudioFeatureAnalyzer,
    capture::system_audio::SystemAudioCapture,
    render::{renderer::Renderer, scene_mesh::SceneMesh},
    session::{AudioAccessState, VisualizerSession},
};

pub fn run() -> anyhow::Result<()> {
    let _mesh = SceneMesh::load("assets/default-scene.ply")?;
    let _renderer = Renderer::new().map_err(anyhow::Error::msg)?;
    let mut session = VisualizerSession::default();
    session.set_access(AudioAccessState::Granted);
    let mut capture = SystemAudioCapture::new(48_000);
    capture.start(session.audio_access_state)?;
    let mut analyzer = AudioFeatureAnalyzer::default();
    let _ = capture.analyze_latest(&mut analyzer);
    session.stop();
    Ok(())
}

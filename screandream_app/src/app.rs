use crate::{
    analysis::audio_features::{AudioFeatureAnalyzer, AudioFeatures},
    capture::{authorization, live::LiveAudio, system_audio::SystemAudioCapture},
    render::{gpu::GpuRenderer, overlay::OverlayFrame, renderer::Renderer, scene_mesh::SceneMesh},
    session::{AudioAccessState, FullScreenExitChoice, SessionState, VisualizerSession},
    ui::{full_screen_exit, overlay, start_view, status_overlay},
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Fullscreen, Window, WindowId},
};

const ANALYSIS_WINDOW: usize = 2048;
/// How long the signal must stay silent before "Audio not detected" is shown.
const SILENCE_GRACE: Duration = Duration::from_millis(750);
/// Key hints stay visible this long after the last input, then fade out.
const HINT_VISIBLE: Duration = Duration::from_secs(4);
const HINT_FADE: Duration = Duration::from_millis(600);
const CONTROLS_HELP: &str =
    "Space start/stop · F full-screen · Esc leave full-screen · S open Settings · Q quit";

pub fn run() -> anyhow::Result<()> {
    let mesh = SceneMesh::load("assets/default-scene.ply")?;
    let renderer = Renderer::new().map_err(anyhow::Error::msg)?;
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(mesh, renderer);
    event_loop.run_app(&mut app)?;
    Ok(())
}

struct App {
    mesh: SceneMesh,
    renderer: Renderer,
    session: VisualizerSession,
    capture: SystemAudioCapture,
    analyzer: AudioFeatureAnalyzer,
    live: Option<LiveAudio>,
    window: Option<Arc<Window>>,
    gpu: Option<GpuRenderer>,
    started_at: Instant,
    last_usable: Option<Instant>,
    awaiting_exit_choice: bool,
    scratch: Vec<f32>,
    window_samples: Vec<f32>,
    last_title: String,
    last_input: Instant,
}

impl App {
    fn new(mesh: SceneMesh, renderer: Renderer) -> Self {
        Self {
            mesh,
            renderer,
            session: VisualizerSession::default(),
            capture: SystemAudioCapture::new(ANALYSIS_WINDOW),
            analyzer: AudioFeatureAnalyzer::new(0.002),
            live: None,
            window: None,
            gpu: None,
            started_at: Instant::now(),
            last_usable: None,
            awaiting_exit_choice: false,
            scratch: Vec::new(),
            window_samples: Vec::with_capacity(ANALYSIS_WINDOW),
            last_title: String::new(),
            last_input: Instant::now(),
        }
    }

    fn start_visualizer(&mut self) {
        self.stop_capture();
        self.session = VisualizerSession::default();
        self.session.request_access();
        println!(
            "{}",
            start_view::status_for_access(AudioAccessState::NotDetermined)
        );

        let access = match LiveAudio::start() {
            Ok(live) => {
                self.analyzer =
                    AudioFeatureAnalyzer::new(0.002).with_sample_rate(live.sample_rate());
                self.live = Some(live);
                AudioAccessState::Granted
            }
            Err(access) => access,
        };
        self.session.set_access(access);
        if access != AudioAccessState::Granted {
            println!("{}", authorization::recovery_message(access));
            return;
        }
        if self.capture.start(access).is_err() {
            return;
        }
        self.session.activate();
        self.last_input = Instant::now();
        self.last_usable = None;
        self.awaiting_exit_choice = false;
        self.apply_full_screen(true);
        println!("Visualizer started. {CONTROLS_HELP}");
    }

    fn stop_capture(&mut self) {
        if let Some(mut live) = self.live.take() {
            live.stop();
        }
        self.capture.stop();
        self.window_samples.clear();
        self.renderer.update(&AudioFeatures::silent(Duration::ZERO));
    }

    fn stop_visualizer(&mut self) {
        self.stop_capture();
        self.session.stop();
        self.awaiting_exit_choice = false;
        self.apply_full_screen(false);
        println!("Visualizer stopped. Press Space to start again.");
    }

    fn apply_full_screen(&mut self, on: bool) {
        if let Some(window) = &self.window {
            window.set_fullscreen(on.then_some(Fullscreen::Borderless(None)));
        }
        self.session.full_screen = on;
    }

    fn is_running(&self) -> bool {
        matches!(
            self.session.state,
            SessionState::Active | SessionState::AudioUnavailable
        )
    }

    fn tick_audio(&mut self) {
        let Some(live) = &self.live else {
            return;
        };
        if live.was_revoked() {
            self.stop_capture();
            self.session.revoke_access();
            self.apply_full_screen(false);
            println!(
                "{}",
                authorization::recovery_message(AudioAccessState::Revoked)
            );
            return;
        }

        self.scratch.clear();
        live.drain_into(&mut self.scratch);
        self.capture.ingest_live_samples(&self.scratch);
        // The capture ring keeps only the newest ANALYSIS_WINDOW samples, so every frame
        // analyzes a full, overlapping FFT block.
        self.window_samples.clear();
        self.window_samples.extend(self.capture.latest_samples());

        let features = if self.awaiting_exit_choice {
            AudioFeatures::silent(Duration::ZERO)
        } else {
            self.analyzer.analyze(&self.window_samples)
        };
        let now = Instant::now();
        if features.is_usable {
            self.last_usable = Some(now);
        }
        let usable = self
            .last_usable
            .is_some_and(|at| now.duration_since(at) < SILENCE_GRACE);
        let was = self.session.state;
        self.session.set_audio_available(usable);
        if was != self.session.state {
            match status_overlay::message(self.session.state) {
                Some(message) => println!("{message}"),
                None => println!("Audio detected — visualizer is reacting."),
            }
        }
        self.renderer.update(&features);
    }

    fn update_title(&mut self) {
        let status = if self.awaiting_exit_choice {
            format!(
                "{} Enter = Continue Windowed · Q = End Session",
                full_screen_exit::PROMPT
            )
        } else if let Some(message) = status_overlay::message(self.session.state) {
            message.to_string()
        } else {
            match self.session.state {
                SessionState::Active => "Reacting to live audio".to_string(),
                SessionState::Idle | SessionState::Ended => {
                    format!("Press Space to {}", start_view::start_label())
                }
                _ => authorization::recovery_message(self.session.audio_access_state).to_string(),
            }
        };
        let title = format!("Screendream — {status}");
        if title != self.last_title {
            if let Some(window) = &self.window {
                window.set_title(&title);
            }
            self.last_title = title;
        }
    }

    fn handle_key(&mut self, event_loop: &ActiveEventLoop, key: &Key) {
        self.last_input = Instant::now();
        match key {
            Key::Named(NamedKey::Enter) if self.awaiting_exit_choice => {
                self.awaiting_exit_choice = false;
                self.session
                    .exit_full_screen(FullScreenExitChoice::ContinueWindowed);
                println!("Continuing windowed.");
            }
            Key::Named(NamedKey::Escape) if self.session.full_screen => {
                self.apply_full_screen(false);
                if self.is_running() {
                    self.prompt_exit_choice();
                }
            }
            Key::Named(NamedKey::Space) => {
                if self.is_running() {
                    self.stop_visualizer();
                } else {
                    self.start_visualizer();
                }
            }
            Key::Character(c) => match c.to_lowercase().as_str() {
                "q" if self.awaiting_exit_choice => self.stop_visualizer(),
                "q" => event_loop.exit(),
                "f" => {
                    let on = !self.session.full_screen;
                    self.awaiting_exit_choice = false;
                    self.apply_full_screen(on);
                }
                "s" => {
                    let _ = std::process::Command::new("open")
                        .arg(authorization::settings_url())
                        .spawn();
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn prompt_exit_choice(&mut self) {
        self.awaiting_exit_choice = true;
        println!(
            "{} Press Enter to Continue Windowed or Q to End Session.",
            full_screen_exit::PROMPT
        );
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Screendream")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                eprintln!("Screendream could not open a window: {error}");
                event_loop.exit();
                return;
            }
        };
        match GpuRenderer::new(window.clone(), &self.mesh) {
            Ok(gpu) => self.gpu = Some(gpu),
            Err(error) => {
                eprintln!("Screendream could not initialise the GPU: {error}");
                event_loop.exit();
                return;
            }
        }
        self.window = Some(window);
        self.start_visualizer();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.stop_visualizer();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(size.width, size.height);
                }
                // The OS (green button / Ctrl-Cmd-F) left full-screen: ask before continuing.
                let os_full_screen = self
                    .window
                    .as_ref()
                    .is_some_and(|window| window.fullscreen().is_some());
                if self.session.full_screen && !os_full_screen && self.is_running() {
                    self.session.full_screen = false;
                    self.prompt_exit_choice();
                }
            }
            WindowEvent::CursorMoved { .. } => self.last_input = Instant::now(),
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key,
                        state: ElementState::Pressed,
                        repeat: false,
                        ..
                    },
                ..
            } => self.handle_key(event_loop, &logical_key),
            WindowEvent::RedrawRequested => {
                self.tick_audio();
                self.update_title();
                let active =
                    self.session.state == SessionState::Active && !self.awaiting_exit_choice;
                let time = self.started_at.elapsed().as_secs_f32();
                let idle = self.last_input.elapsed().saturating_sub(HINT_VISIBLE);
                let hint_alpha = 1.0 - (idle.as_secs_f32() / HINT_FADE.as_secs_f32()).min(1.0);
                let content =
                    overlay::content(&self.session, self.awaiting_exit_choice, hint_alpha > 0.0);
                let scale = self
                    .window
                    .as_ref()
                    .map_or(1.0, |window| window.scale_factor() as f32);
                if let Some(gpu) = &mut self.gpu {
                    let frame = OverlayFrame {
                        content: &content,
                        scale,
                        hint_alpha,
                    };
                    gpu.render(&self.renderer.uniform, time, active, &frame);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.stop_capture();
    }
}

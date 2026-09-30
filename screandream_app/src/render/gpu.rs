use crate::render::{
    modes::{self, DEPTH_FORMAT, FrameInput, ModeContext, VisualizerMode},
    overlay::{OverlayFrame, OverlayRenderer},
    renderer::VisualizerUniform,
    scene_mesh::SceneMesh,
};
use std::sync::Arc;
use winit::window::Window;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuControls {
    time: f32,
    aspect: f32,
    amplitude: f32,
    glow: f32,
    bands: [f32; 4],
    active: f32,
    _pad: [f32; 3],
}

pub struct GpuRenderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth: wgpu::TextureView,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    modes: Vec<Box<dyn VisualizerMode>>,
    current: usize,
    overlay: OverlayRenderer,
}

impl GpuRenderer {
    pub fn new(window: Arc<Window>, scene: &SceneMesh) -> anyhow::Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(window)?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .ok_or_else(|| anyhow::anyhow!("no compatible GPU adapter"))?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None))?;

        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or_else(|| anyhow::anyhow!("surface is not supported by the adapter"))?;
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);
        let depth = create_depth(&device, &config);
        let overlay = OverlayRenderer::new(&device, &queue, config.format);

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("controls"),
            size: std::mem::size_of::<GpuControls>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("controls"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("controls"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let modes = modes::build_all(
            &ModeContext {
                device: &device,
                format: config.format,
                controls_layout: &layout,
            },
            scene,
        );

        Ok(Self {
            surface,
            device,
            queue,
            config,
            depth,
            uniform_buffer,
            bind_group,
            modes,
            current: 0,
            overlay,
        })
    }

    pub fn mode_count(&self) -> usize {
        self.modes.len()
    }

    pub fn mode_index(&self, name: &str) -> Option<usize> {
        self.modes.iter().position(|mode| mode.name() == name)
    }

    pub fn mode_name(&self) -> &'static str {
        self.modes[self.current].name()
    }

    pub fn set_mode(&mut self, index: usize) {
        if index < self.modes.len() {
            self.current = index;
            self.modes[index].activate();
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.depth = create_depth(&self.device, &self.config);
    }

    pub fn render(
        &mut self,
        uniform: &VisualizerUniform,
        time: f32,
        active: bool,
        overlay: &OverlayFrame,
    ) {
        let controls = GpuControls {
            time,
            aspect: self.config.width as f32 / self.config.height.max(1) as f32,
            amplitude: uniform.amplitude,
            glow: uniform.glow,
            bands: uniform.bands,
            active: if active { 1.0 } else { 0.0 },
            _pad: [0.0; 3],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&controls));
        let mode = &mut self.modes[self.current];
        mode.prepare(
            &self.queue,
            &FrameInput {
                uniform,
                time,
                active,
            },
        );

        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            Err(error) => {
                eprintln!("Screendream: dropped frame: {error}");
                return;
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("visualizer"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.bind_group, &[]);
            self.modes[self.current].draw(&mut pass);
        }
        self.overlay.prepare(
            &self.device,
            &self.queue,
            overlay,
            self.config.width,
            self.config.height,
        );
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("overlay"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.overlay.render(&mut pass);
        }
        self.queue.submit([encoder.finish()]);
        frame.present();
        self.overlay.trim();
    }
}

fn create_depth(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width: config.width,
                height: config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

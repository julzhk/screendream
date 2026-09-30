//! Planetary canyon fly-through. The camera flies along a winding canyon whose walls are
//! raised by the music: distance from the canyon floor maps to frequency (treble on the floor,
//! bass on the outer peaks) and distance ahead of the camera maps to time, so each moment of
//! sound ripples out towards the horizon. A ringed planet, aurora, shooting stars and glowing
//! contour lines ride along; flight speed follows the bass and the palette keeps drifting.

use super::{FrameInput, ModeContext, PipelineSpec, VisualizerMode, pipeline, shader_with_common};
use crate::analysis::audio_features::SPECTRUM_BINS;
use wgpu::util::DeviceExt;

/// Terrain grid resolution; must match `COLS` in canyon.wgsl.
const GRID_COLS: u32 = 201;
const GRID_ROWS: u32 = 300;
/// Rows of spectrum history kept, and how often a new row is recorded.
const HISTORY_ROWS: u32 = 256;
const HISTORY_RATE: f32 = 60.0;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CanyonParams {
    travel: f32,
    /// Texture v coordinate of the newest history row.
    head: f32,
    hue: f32,
    kick: f32,
}

pub struct CanyonMode {
    sky: wgpu::RenderPipeline,
    terrain: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    params_buffer: wgpu::Buffer,
    history: wgpu::Texture,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,

    params: CanyonParams,
    head_row: u32,
    row_debt: f32,
    last_time: Option<f32>,
    speed: f32,
    last_bass: f32,
}

impl CanyonMode {
    pub const NAME: &'static str = "Canyon";

    pub fn new(ctx: &ModeContext) -> Self {
        let device = ctx.device;
        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("canyon params"),
            size: std::mem::size_of::<CanyonParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let history = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("canyon spectrum history"),
            size: wgpu::Extent3d {
                width: SPECTRUM_BINS as u32,
                height: HISTORY_ROWS,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("canyon history"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("canyon"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let history_view = history.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("canyon"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&history_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("canyon"),
            bind_group_layouts: &[ctx.controls_layout, &layout],
            push_constant_ranges: &[],
        });
        let module = shader_with_common(ctx, "canyon.wgsl", include_str!("../shaders/canyon.wgsl"));
        let sky = pipeline(
            ctx,
            PipelineSpec {
                label: "canyon sky",
                layout: &pipeline_layout,
                shader: &module,
                vs: "vs_sky",
                fs: "fs_sky",
                buffers: &[],
                depth_write: false,
            },
        );
        let terrain = pipeline(
            ctx,
            PipelineSpec {
                label: "canyon terrain",
                layout: &pipeline_layout,
                shader: &module,
                vs: "vs_terrain",
                fs: "fs_terrain",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 8,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2],
                }],
                depth_write: true,
            },
        );

        let (vertices, indices) = grid(GRID_COLS, GRID_ROWS);
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("canyon grid vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("canyon grid indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            sky,
            terrain,
            bind_group,
            params_buffer,
            history,
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
            params: CanyonParams {
                travel: 0.0,
                head: 0.0,
                hue: 0.0,
                kick: 0.0,
            },
            head_row: 0,
            row_debt: 0.0,
            last_time: None,
            speed: 12.0,
            last_bass: 0.0,
        }
    }

    fn push_history_row(&mut self, queue: &wgpu::Queue, spectrum: &[f32; SPECTRUM_BINS]) {
        self.head_row = (self.head_row + 1) % HISTORY_ROWS;
        let row: [u8; SPECTRUM_BINS] =
            std::array::from_fn(|bin| (spectrum[bin].clamp(0.0, 1.0) * 255.0).round() as u8);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.history,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: self.head_row,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &row,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(SPECTRUM_BINS as u32),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: SPECTRUM_BINS as u32,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
    }
}

impl VisualizerMode for CanyonMode {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn activate(&mut self) {
        // Start each visit from the canyon mouth so travel (and shader precision) stays small.
        self.params.travel = 0.0;
        self.last_time = None;
    }

    fn prepare(&mut self, queue: &wgpu::Queue, frame: &FrameInput) {
        let dt = self
            .last_time
            .map_or(0.0, |last| (frame.time - last).clamp(0.0, 0.1));
        self.last_time = Some(frame.time);

        let audio = frame.uniform;
        let bass = audio.bands[0];
        let silent = [0.0; SPECTRUM_BINS];
        let spectrum = if frame.active {
            &audio.spectrum
        } else {
            &silent
        };

        // Record spectrum rows at a fixed rate so ripples travel at the same speed at any fps.
        self.row_debt = (self.row_debt + dt * HISTORY_RATE).min(4.0);
        while self.row_debt >= 1.0 {
            self.row_debt -= 1.0;
            self.push_history_row(queue, spectrum);
        }

        // Kicks: a sharp rise in bass gives a jolt that decays quickly.
        let rise = (bass - self.last_bass).max(0.0);
        self.last_bass = bass;
        self.params.kick = (self.params.kick * (1.0 - dt * 4.0).max(0.0) + rise * 3.0).min(1.5);

        let target_speed = if frame.active {
            10.0 + 38.0 * bass.powf(1.5) + 12.0 * audio.amplitude + 10.0 * self.params.kick
        } else {
            5.0
        };
        self.speed += (target_speed - self.speed) * (dt * 1.5).min(1.0);
        self.params.travel += self.speed * dt;
        self.params.hue = (self.params.hue
            + dt * (0.015 + 0.05 * audio.bands[1] + 0.25 * self.params.kick))
            .fract();
        self.params.head = (self.head_row as f32 + 0.5) / HISTORY_ROWS as f32;

        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&self.params));
    }

    fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_bind_group(1, &self.bind_group, &[]);
        pass.set_pipeline(&self.sky);
        pass.draw(0..3, 0..1);
        pass.set_pipeline(&self.terrain);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..1);
    }
}

/// A `cols` × `rows` grid of (column, row) vertices; the shader places them in the world.
fn grid(cols: u32, rows: u32) -> (Vec<[f32; 2]>, Vec<u32>) {
    let vertices = (0..rows)
        .flat_map(|row| (0..cols).map(move |col| [col as f32, row as f32]))
        .collect();
    let mut indices = Vec::with_capacity(((cols - 1) * (rows - 1) * 6) as usize);
    for row in 0..rows - 1 {
        for col in 0..cols - 1 {
            let a = row * cols + col;
            let b = a + cols;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    (vertices, indices)
}

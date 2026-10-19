//! A drift through deep space past dozens of cellular automata, each growing on its own
//! tilted, bent plane. The automata come from `render::automata_sim`; this mode uploads their
//! cells into one texture layer each and draws the planes additively, so overlapping sheets
//! mix their colours. Screen warps bend both the backdrop (per pixel) and the planes (per
//! vertex, after projection).

use super::{
    ADDITIVE, FrameInput, ModeContext, PipelineSpec, VisualizerMode, pipeline, pipeline_with_blend,
    shader_with_common,
};
use crate::render::automata_sim::{AutomataSim, Drive, GRID, Kind, MAX_AUTOMATA, MAX_WARPS};

/// Quads per plane side; matches `TESS` in automata.wgsl.
const TESS: u32 = 40;
/// Horizontal field of view in the shader's units: focal length = 1 / tan(fov / 2).
const FOCAL: f32 = 1.45;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct FieldParams {
    /// Camera position and focal length, forward and beat pulse, right and energy, up and
    /// treble.
    camera: [[f32; 4]; 4],
    /// Two vec4s per warp: (x, y, radius, strength × envelope), (kind, dir x, dir y, age).
    warps: [[f32; 4]; MAX_WARPS * 2],
    /// Eight vec4s per automaton; see automata.wgsl.
    planes: [[f32; 4]; MAX_AUTOMATA * 8],
}

pub struct AutomataMode {
    space: wgpu::RenderPipeline,
    planes: wgpu::RenderPipeline,
    warps: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    params_buffer: wgpu::Buffer,
    cells: wgpu::Texture,
    params: Box<FieldParams>,
    texels: Vec<u8>,
    sim: AutomataSim,
    last_time: Option<f32>,
    ripple_clock: f32,
}

impl AutomataMode {
    pub const NAME: &'static str = "Automata";

    pub fn new(ctx: &ModeContext) -> Self {
        let device = ctx.device;
        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("automata params"),
            size: std::mem::size_of::<FieldParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let cells = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("automata cells"),
            size: wgpu::Extent3d {
                width: GRID as u32,
                height: GRID as u32,
                depth_or_array_layers: MAX_AUTOMATA as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("automata cells"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("automata"),
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
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let cells_view = cells.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("automata"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&cells_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("automata"),
            bind_group_layouts: &[ctx.controls_layout, &layout],
            push_constant_ranges: &[],
        });
        let module = shader_with_common(
            ctx,
            "automata.wgsl",
            include_str!("../shaders/automata.wgsl"),
        );
        let spec = |label, vs, fs| PipelineSpec {
            label,
            layout: &pipeline_layout,
            shader: &module,
            vs,
            fs,
            buffers: &[],
            depth_write: false,
        };
        let space = pipeline(ctx, spec("automata space", "vs_space", "fs_space"));
        let planes = pipeline_with_blend(
            ctx,
            spec("automata planes", "vs_plane", "fs_plane"),
            ADDITIVE,
        );
        let warps = pipeline_with_blend(
            ctx,
            spec("automata warps", "vs_space", "fs_warps"),
            ADDITIVE,
        );

        Self {
            space,
            planes,
            warps,
            bind_group,
            params_buffer,
            cells,
            params: Box::new(bytemuck::Zeroable::zeroed()),
            texels: vec![0; GRID * GRID * 4],
            sim: new_sim(),
            last_time: None,
            ripple_clock: 0.0,
        }
    }

    fn upload_cells(&mut self, queue: &wgpu::Queue) {
        for (layer, automaton) in self.sim.automata_mut().iter_mut().enumerate() {
            if !automaton.take_dirty() {
                continue;
            }
            let dim = automaton.dim as u32;
            let bytes = &mut self.texels[..(dim * dim * 4) as usize];
            automaton.write_texels(bytes);
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.cells,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(dim * 4),
                    rows_per_image: Some(dim),
                },
                wgpu::Extent3d {
                    width: dim,
                    height: dim,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
}

impl VisualizerMode for AutomataMode {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn activate(&mut self) {
        self.sim = new_sim();
        self.last_time = None;
    }

    fn prepare(&mut self, queue: &wgpu::Queue, frame: &FrameInput) {
        let dt = self
            .last_time
            .map_or(0.0, |last| (frame.time - last).clamp(0.0, 0.1));
        self.last_time = Some(frame.time);

        let audio = frame.uniform;
        let drive = if frame.active {
            Drive {
                bass: audio.bands[0],
                energy: (audio.bands.iter().sum::<f32>() / 4.0).max(audio.amplitude),
                treble: audio.bands[3],
            }
        } else {
            Drive::default()
        };
        self.sim.step(dt, frame.aspect, drive);
        let pulse = self.sim.beat_pulse();
        self.ripple_clock += dt * (1.2 + 4.0 * pulse);

        let cam = self.sim.camera();
        // A slight zoom punch on each beat.
        let focal = FOCAL * (1.0 + 0.05 * pulse);
        self.params.camera = [
            [cam.pos[0], cam.pos[1], cam.pos[2], focal],
            [cam.forward[0], cam.forward[1], cam.forward[2], pulse],
            [cam.right[0], cam.right[1], cam.right[2], drive.energy],
            [cam.up[0], cam.up[1], cam.up[2], drive.treble],
        ];

        let warps = self.sim.warps();
        for (i, slot) in self
            .params
            .warps
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .enumerate()
        {
            let Some(w) = warps.get(i) else {
                slot.fill([0.0; 4]);
                continue;
            };
            let speed = (w.vel[0] * w.vel[0] + w.vel[1] * w.vel[1]).sqrt().max(1e-6);
            let strength = w.strength * w.envelope() * (1.0 + 0.6 * pulse);
            slot[0] = [w.pos[0], w.pos[1], w.radius, strength];
            slot[1] = [
                w.kind as u8 as f32,
                w.vel[0] / speed,
                w.vel[1] / speed,
                w.age,
            ];
        }

        for (a, slot) in self
            .sim
            .automata()
            .iter()
            .zip(self.params.planes.as_chunks_mut::<8>().0)
        {
            let rule = a.rule();
            let h = a.half_size;
            let [c0, c1, c2] = a.palette;
            slot[0] = [a.center[0], a.center[1], a.center[2], a.fade];
            slot[1] = [
                a.axis_u[0] * h,
                a.axis_u[1] * h,
                a.axis_u[2] * h,
                a.dim as f32,
            ];
            slot[2] = [a.axis_v[0] * h, a.axis_v[1] * h, a.axis_v[2] * h, a.radius];
            slot[3] = a.warp;
            slot[4] = [c0[0], c0[1], c0[2], self.ripple_clock + a.seed * 10.0];
            slot[5] = [c1[0], c1[1], c1[2], a.front_pulse];
            slot[6] = [c2[0], c2[1], c2[2], a.shape as f32];
            let glow = if rule.class == 4 { 1.0 } else { 0.6 };
            let elementary = matches!(rule.kind, Kind::Elementary { .. });
            slot[7] = [a.seed, glow, 0.0, elementary as u8 as f32];
        }
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&*self.params));
        self.upload_cells(queue);
    }

    fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_bind_group(1, &self.bind_group, &[]);
        pass.set_pipeline(&self.space);
        pass.draw(0..3, 0..1);
        pass.set_pipeline(&self.planes);
        pass.draw(0..TESS * TESS * 6, 0..self.sim.automata().len() as u32);
        pass.set_pipeline(&self.warps);
        pass.draw(0..3, 0..1);
    }
}

fn new_sim() -> AutomataSim {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |elapsed| elapsed.as_nanos() as u64);
    AutomataSim::new(seed)
}

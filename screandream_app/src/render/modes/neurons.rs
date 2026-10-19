//! Flying through a neural culture under a fluorescence microscope. The network comes from
//! `render::neuron_sim`; this mode turns it into two instance streams each frame: capsule
//! segments for the tendril ropes, and procedural billboards for cell bodies, growth cones,
//! impulses, debris, organelles and bacteria. Tendrils are max-blended so their overlapping
//! joints don't double in brightness; everything else glows additively, like fluorescence.

use super::{
    ADDITIVE, FrameInput, MULTIPLY, ModeContext, PipelineSpec, VisualizerMode, pipeline,
    pipeline_with_blend, shader_with_common,
};
use crate::render::neuron_sim::{
    BANDS, CHAIN, Drive, MAX_DEBRIS, MAX_MICROBES, MAX_NODES, MAX_PULSES, MAX_TENDRILS, NeuronSim,
    Phase,
};

const FOCAL: f32 = 1.5;
/// Depth-of-field strength: blur radius in screen units per unit of relative defocus.
const APERTURE: f32 = 0.035;
/// Seconds each stain set is shown before crossfading to the next.
const STAIN_PERIOD: f32 = 45.0;
const MAX_SEGMENTS: usize = MAX_TENDRILS * (CHAIN - 1);
const MAX_SPRITES: usize = MAX_NODES + MAX_TENDRILS + MAX_PULSES + MAX_DEBRIS + MAX_MICROBES;
/// Impulse colours per band: bass magenta, low-mid amber, high-mid lime, treble cyan.
const BAND_COLORS: [[f32; 3]; BANDS] = [
    [1.0, 0.25, 0.5],
    [1.0, 0.65, 0.2],
    [0.55, 1.0, 0.3],
    [0.45, 0.9, 1.0],
];
/// Sprite kinds, matching the `KIND_*` constants in neurons.wgsl. Microbes follow from 4.
const KIND_SOMA: f32 = 0.0;
const KIND_FLOWER: f32 = 1.0;
const KIND_PULSE: f32 = 2.0;
const KIND_DEBRIS: f32 = 3.0;
const KIND_FIRST_MICROBE: f32 = 4.0;

/// Keeps whichever is brighter: overlapping rope segments merge seamlessly.
const MAX_BLEND: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Max,
    },
    alpha: wgpu::BlendComponent::OVER,
};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ScopeParams {
    /// Camera position and focal length, forward and focus distance, right and aperture,
    /// up and stain phase.
    camera: [[f32; 4]; 4],
    /// Per-band onset flash.
    beats: [f32; 4],
}

/// One rope segment; see `TendrilIn` in neurons.wgsl.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Segment {
    a: [f32; 4],
    b: [f32; 4],
    g: [f32; 4],
    c: [f32; 4],
}

/// One billboard; see `SpriteIn` in neurons.wgsl.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Sprite {
    a: [f32; 4],
    b: [f32; 4],
    c: [f32; 4],
    d: [f32; 4],
}

const INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 4] =
    wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4];

pub struct NeuronsMode {
    fluid: wgpu::RenderPipeline,
    tendrils: wgpu::RenderPipeline,
    sprites: wgpu::RenderPipeline,
    optics: wgpu::RenderPipeline,
    eyepiece: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    params_buffer: wgpu::Buffer,
    segment_buffer: wgpu::Buffer,
    sprite_buffer: wgpu::Buffer,
    segments: Vec<Segment>,
    sprites_data: Vec<Sprite>,
    sim: NeuronSim,
    last_time: Option<f32>,
}

impl NeuronsMode {
    pub const NAME: &'static str = "Neurons";

    pub fn new(ctx: &ModeContext) -> Self {
        let device = ctx.device;
        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("neurons params"),
            size: std::mem::size_of::<ScopeParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let instance_buffer = |label, size| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let segment_buffer = instance_buffer(
            "neuron segments",
            MAX_SEGMENTS * std::mem::size_of::<Segment>(),
        );
        let sprite_buffer = instance_buffer(
            "neuron sprites",
            MAX_SPRITES * std::mem::size_of::<Sprite>(),
        );
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("neurons"),
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
            label: Some("neurons"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buffer.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("neurons"),
            bind_group_layouts: &[ctx.controls_layout, &layout],
            push_constant_ranges: &[],
        });
        let module =
            shader_with_common(ctx, "neurons.wgsl", include_str!("../shaders/neurons.wgsl"));
        // Segments and sprites are both four vec4s per instance, so they share a layout.
        let instances = [wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Sprite>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &INSTANCE_ATTRIBUTES,
        }];
        let spec = |label, vs, fs, buffers| PipelineSpec {
            label,
            layout: &pipeline_layout,
            shader: &module,
            vs,
            fs,
            buffers,
            depth_write: false,
        };
        let fluid = pipeline(ctx, spec("neurons fluid", "vs_screen", "fs_fluid", &[]));
        let tendrils = pipeline_with_blend(
            ctx,
            spec("neurons tendrils", "vs_tendril", "fs_tendril", &instances),
            MAX_BLEND,
        );
        let sprites = pipeline_with_blend(
            ctx,
            spec("neurons sprites", "vs_sprite", "fs_sprite", &instances),
            ADDITIVE,
        );
        let optics = pipeline_with_blend(
            ctx,
            spec("neurons optics", "vs_screen", "fs_optics", &[]),
            ADDITIVE,
        );
        let eyepiece = pipeline_with_blend(
            ctx,
            spec("neurons eyepiece", "vs_screen", "fs_eyepiece", &[]),
            MULTIPLY,
        );

        Self {
            fluid,
            tendrils,
            sprites,
            optics,
            eyepiece,
            bind_group,
            params_buffer,
            segment_buffer,
            sprite_buffer,
            segments: Vec::with_capacity(MAX_SEGMENTS),
            sprites_data: Vec::with_capacity(MAX_SPRITES),
            sim: new_sim(),
            last_time: None,
        }
    }

    fn build_segments(&mut self) {
        self.segments.clear();
        let sim = &self.sim;
        let nodes = sim.nodes();
        for (t, tendril) in sim.tendrils().iter().enumerate() {
            let glow = sim.point_glow(t);
            let fade = match tendril.phase {
                Phase::Connected => nodes[tendril.from].fade.min(nodes[tendril.to].fade),
                Phase::Growing => nodes[tendril.from].fade,
                // Dims as it withdraws.
                Phase::Retracting => nodes[tendril.from].fade * (tendril.rest * 2.0).min(1.0),
            };
            let mut arc = [0.0f32; CHAIN];
            for i in 1..CHAIN {
                let d: f32 = (0..3)
                    .map(|k| (tendril.points[i][k] - tendril.points[i - 1][k]).powi(2))
                    .sum();
                arc[i] = arc[i - 1] + d.sqrt();
            }
            let point = |i: usize| {
                let s = i as f32 / (CHAIN - 1) as f32;
                let taper = match tendril.phase {
                    Phase::Connected => 1.0 - 0.3 * (s * std::f32::consts::PI).sin(),
                    Phase::Growing => 1.0 - 0.45 * s,
                    Phase::Retracting => 1.0 - 0.4 * s,
                };
                let stain = match tendril.phase {
                    Phase::Connected => {
                        tendril.stain_from + (tendril.stain_to - tendril.stain_from) * s
                    }
                    _ => tendril.stain_from,
                };
                (tendril.thickness * taper, stain)
            };
            for i in 0..CHAIN - 1 {
                let (r0, st0) = point(i);
                let (r1, st1) = point(i + 1);
                let (p0, p1) = (tendril.points[i], tendril.points[i + 1]);
                self.segments.push(Segment {
                    a: [p0[0], p0[1], p0[2], r0],
                    b: [p1[0], p1[1], p1[2], r1],
                    g: [glow[i], glow[i + 1], arc[i], arc[i + 1]],
                    c: [st0, st1, fade, tendril.seed],
                });
            }
        }
    }

    fn build_sprites(&mut self, time: f32) {
        self.sprites_data.clear();
        let sim = &self.sim;
        let out = &mut self.sprites_data;
        let up = [0.0, 0.0, 1.0];
        for n in sim.nodes() {
            let flash = BAND_COLORS[n.glow_band];
            out.push(Sprite {
                a: [n.pos[0], n.pos[1], n.pos[2], n.radius],
                b: [up[0], up[1], up[2], KIND_SOMA],
                c: [n.glow, n.phase, n.seed, n.stain],
                d: [flash[0], flash[1], flash[2], n.fade],
            });
        }
        for (t, tendril) in sim.tendrils().iter().enumerate() {
            let Some((pos, axis, open)) = sim.flower(t) else {
                continue;
            };
            let fade = sim.nodes()[tendril.from].fade;
            out.push(Sprite {
                a: [
                    pos[0],
                    pos[1],
                    pos[2],
                    tendril.thickness * 3.2 * (0.7 + 0.5 * open),
                ],
                b: [axis[0], axis[1], axis[2], KIND_FLOWER],
                c: [
                    open,
                    time * 1.5 + tendril.seed * 10.0,
                    tendril.flash,
                    tendril.stain_from,
                ],
                d: [1.0, 1.0, 1.0, fade],
            });
        }
        for p in sim.pulses() {
            let pos = sim.pulse_position(p);
            let size = [0.11, 0.09, 0.075, 0.06][p.band] * (0.6 + 0.6 * p.energy);
            let c = BAND_COLORS[p.band];
            out.push(Sprite {
                a: [pos[0], pos[1], pos[2], size],
                b: [up[0], up[1], up[2], KIND_PULSE],
                c: [p.energy, p.band as f32, 0.0, 0.0],
                d: [c[0], c[1], c[2], 1.0],
            });
        }
        for d in sim.debris() {
            out.push(Sprite {
                a: [d.pos[0], d.pos[1], d.pos[2], d.size],
                b: [up[0], up[1], up[2], KIND_DEBRIS],
                c: [1.0 - d.age / d.life, 0.0, 0.0, d.stain],
                d: [1.0, 1.0, 1.0, 1.0],
            });
        }
        for m in sim.microbes() {
            out.push(Sprite {
                a: [m.pos[0], m.pos[1], m.pos[2], m.size],
                b: [
                    m.axis[0],
                    m.axis[1],
                    m.axis[2],
                    KIND_FIRST_MICROBE + m.kind as u8 as f32,
                ],
                c: [m.phase, m.seed, 0.0, 0.0],
                d: [m.tint[0], m.tint[1], m.tint[2], 0.55],
            });
        }
    }
}

impl VisualizerMode for NeuronsMode {
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
                bands: audio.bands,
                energy: (audio.bands.iter().sum::<f32>() / 4.0).max(audio.amplitude),
            }
        } else {
            Drive::default()
        };
        self.sim.step(dt, drive);

        let cam = self.sim.camera();
        let beats = self.sim.beat_flash();
        let stain_phase = (frame.time / STAIN_PERIOD).rem_euclid(3.0);
        let params = ScopeParams {
            camera: [
                [cam.pos[0], cam.pos[1], cam.pos[2], FOCAL],
                [
                    cam.forward[0],
                    cam.forward[1],
                    cam.forward[2],
                    self.sim.focus(),
                ],
                [cam.right[0], cam.right[1], cam.right[2], APERTURE],
                [cam.up[0], cam.up[1], cam.up[2], stain_phase],
            ],
            beats,
        };
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&params));

        self.build_segments();
        self.build_sprites(frame.time);
        queue.write_buffer(
            &self.segment_buffer,
            0,
            bytemuck::cast_slice(&self.segments),
        );
        queue.write_buffer(
            &self.sprite_buffer,
            0,
            bytemuck::cast_slice(&self.sprites_data),
        );
    }

    fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_bind_group(1, &self.bind_group, &[]);
        pass.set_pipeline(&self.fluid);
        pass.draw(0..3, 0..1);
        if !self.segments.is_empty() {
            pass.set_pipeline(&self.tendrils);
            pass.set_vertex_buffer(0, self.segment_buffer.slice(..));
            pass.draw(0..6, 0..self.segments.len() as u32);
        }
        if !self.sprites_data.is_empty() {
            pass.set_pipeline(&self.sprites);
            pass.set_vertex_buffer(0, self.sprite_buffer.slice(..));
            pass.draw(0..6, 0..self.sprites_data.len() as u32);
        }
        pass.set_pipeline(&self.optics);
        pass.draw(0..3, 0..1);
        pass.set_pipeline(&self.eyepiece);
        pass.draw(0..3, 0..1);
    }
}

fn new_sim() -> NeuronSim {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |elapsed| elapsed.as_nanos() as u64);
    NeuronSim::new(seed)
}

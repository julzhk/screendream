//! A screen-filling clockwork mechanism of brass, gold, steel and copper under wandering
//! lamps. The gear trains come from `render::gear_sim`; this mode draws them back to front,
//! each gear preceded by its soft shadow on the layer beneath, so sinking gears slip under
//! the mechanism and arriving ones cast a widening shadow as they drop in.

use super::{
    ADDITIVE, FrameInput, MULTIPLY, ModeContext, PipelineSpec, VisualizerMode, pipeline,
    pipeline_with_blend, pipeline_with_depth, shader_with_common,
};
use crate::render::gear_sim::{
    DUST_MOTES, Drive, FOCAL, GearSim, LAYER_Z, LIGHTS, MAX_GEARS, MAX_SPARKS, PLATE_Z,
};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct WorksParams {
    /// Camera x, y, roll, focal length.
    cam: [f32; 4],
    light_pos: [[f32; 4]; LIGHTS],
    light_col: [[f32; 4]; LIGHTS],
    /// Beat pulse, plate depth.
    misc: [f32; 4],
}

/// One gear; see `GearIn` in clockwork.wgsl.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GearInstance {
    a: [f32; 4],
    b: [f32; 4],
    c: [f32; 4],
    d: [f32; 4],
}

/// One spark or dust mote; see `SparkIn` in clockwork.wgsl.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SparkInstance {
    a: [f32; 4],
    b: [f32; 4],
    c: [f32; 4],
}

const GEAR_ATTRIBUTES: [wgpu::VertexAttribute; 4] =
    wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4];
const SPARK_ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4];

pub struct ClockworkMode {
    plate: wgpu::RenderPipeline,
    jewels: wgpu::RenderPipeline,
    shadows: wgpu::RenderPipeline,
    gears: wgpu::RenderPipeline,
    sparks: wgpu::RenderPipeline,
    glow: wgpu::RenderPipeline,
    vignette: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    params_buffer: wgpu::Buffer,
    gear_buffer: wgpu::Buffer,
    spark_buffer: wgpu::Buffer,
    gear_data: Vec<GearInstance>,
    spark_data: Vec<SparkInstance>,
    sim: GearSim,
    last_time: Option<f32>,
}

impl ClockworkMode {
    pub const NAME: &'static str = "Clockwork";

    pub fn new(ctx: &ModeContext) -> Self {
        let device = ctx.device;
        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("clockwork params"),
            size: std::mem::size_of::<WorksParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let instance_buffer = |label, size: usize| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let gear_buffer = instance_buffer(
            "clockwork gears",
            MAX_GEARS * std::mem::size_of::<GearInstance>(),
        );
        let spark_buffer = instance_buffer(
            "clockwork sparks",
            (MAX_SPARKS + DUST_MOTES) * std::mem::size_of::<SparkInstance>(),
        );
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("clockwork"),
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
            label: Some("clockwork"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buffer.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("clockwork"),
            bind_group_layouts: &[ctx.controls_layout, &layout],
            push_constant_ranges: &[],
        });
        let module = shader_with_common(
            ctx,
            "clockwork.wgsl",
            include_str!("../shaders/clockwork.wgsl"),
        );
        let gear_layout = [wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GearInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &GEAR_ATTRIBUTES,
        }];
        let spark_layout = [wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SparkInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &SPARK_ATTRIBUTES,
        }];
        let spec = |label, vs, fs, buffers, depth_write| PipelineSpec {
            label,
            layout: &pipeline_layout,
            shader: &module,
            vs,
            fs,
            buffers,
            depth_write,
        };
        let plate = pipeline(
            ctx,
            spec("clockwork plate", "vs_screen", "fs_plate", &[], true),
        );
        let jewels = pipeline(
            ctx,
            spec(
                "clockwork jewels",
                "vs_jewel",
                "fs_jewel",
                &gear_layout,
                true,
            ),
        );
        // Shadows darken only what lies behind them; sparks hide behind gears.
        let shadows = pipeline_with_depth(
            ctx,
            spec(
                "clockwork shadows",
                "vs_shadow",
                "fs_shadow",
                &gear_layout,
                false,
            ),
            MULTIPLY,
            wgpu::CompareFunction::Less,
        );
        let gears = pipeline(
            ctx,
            spec("clockwork gears", "vs_gear", "fs_gear", &gear_layout, true),
        );
        let sparks = pipeline_with_depth(
            ctx,
            spec(
                "clockwork sparks",
                "vs_spark",
                "fs_spark",
                &spark_layout,
                false,
            ),
            ADDITIVE,
            wgpu::CompareFunction::Less,
        );
        let glow = pipeline_with_blend(
            ctx,
            spec("clockwork glow", "vs_screen", "fs_glow", &[], false),
            ADDITIVE,
        );
        let vignette = pipeline_with_blend(
            ctx,
            spec("clockwork vignette", "vs_screen", "fs_vignette", &[], false),
            MULTIPLY,
        );

        Self {
            plate,
            jewels,
            shadows,
            gears,
            sparks,
            glow,
            vignette,
            bind_group,
            params_buffer,
            gear_buffer,
            spark_buffer,
            gear_data: Vec::with_capacity(MAX_GEARS),
            spark_data: Vec::with_capacity(MAX_SPARKS + DUST_MOTES),
            sim: new_sim(),
            last_time: None,
        }
    }

    fn build_instances(&mut self) {
        // Back to front, so each shadow falls on what is already drawn beneath it.
        let mut gears: Vec<_> = self.sim.gears().map(|(_, g)| *g).collect();
        gears.sort_by(|a, b| b.pos[2].total_cmp(&a.pos[2]));
        self.gear_data.clear();
        for g in &gears {
            let receiver = LAYER_Z
                .iter()
                .copied()
                .chain([PLATE_Z])
                .filter(|&z| z > g.pos[2] + 0.04)
                .fold(PLATE_Z, f32::min);
            self.gear_data.push(GearInstance {
                a: [g.pos[0], g.pos[1], g.pos[2], g.outer_radius()],
                b: [g.angle, g.teeth as f32, g.radius, g.kind as u8 as f32],
                c: [g.material as u8 as f32, g.seed, g.spokes as f32, g.spin],
                d: [g.fade, receiver, 0.0, 0.0],
            });
        }

        self.spark_data.clear();
        for s in self.sim.sparks() {
            self.spark_data.push(SparkInstance {
                a: [s.pos[0], s.pos[1], s.pos[2], 0.006],
                b: [s.vel[0], s.vel[1], s.vel[2], 0.0],
                c: [1.0 - s.age / s.life, 0.0, 0.0, 0.0],
            });
        }
        for m in self.sim.motes() {
            self.spark_data.push(SparkInstance {
                a: [m.pos[0], m.pos[1], m.pos[2], 0.004],
                b: [0.0, 0.0, 0.0, 1.0],
                c: [m.age / m.life, 0.0, 0.0, 0.0],
            });
        }
    }
}

impl VisualizerMode for ClockworkMode {
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
        self.sim.step(dt, drive);

        // A slow drift across the mechanism shows off the parallax between layers.
        let t = frame.time;
        let shake = self.sim.shake();
        let params = WorksParams {
            cam: [
                0.18 * (t * 0.05).sin() + shake[0],
                0.1 * (t * 0.07 + 1.0).sin() + shake[1],
                0.03 * (t * 0.04).sin(),
                FOCAL,
            ],
            light_pos: self
                .sim
                .lights()
                .map(|l| [l.pos[0], l.pos[1], l.pos[2], 0.0]),
            light_col: self
                .sim
                .lights()
                .map(|l| [l.color[0], l.color[1], l.color[2], 0.0]),
            misc: [self.sim.beat_pulse(), PLATE_Z, 0.0, 0.0],
        };
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&params));

        self.build_instances();
        queue.write_buffer(&self.gear_buffer, 0, bytemuck::cast_slice(&self.gear_data));
        queue.write_buffer(
            &self.spark_buffer,
            0,
            bytemuck::cast_slice(&self.spark_data),
        );
    }

    fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_bind_group(1, &self.bind_group, &[]);
        pass.set_pipeline(&self.plate);
        pass.draw(0..3, 0..1);
        let gears = self.gear_data.len() as u32;
        if gears > 0 {
            pass.set_vertex_buffer(0, self.gear_buffer.slice(..));
            pass.set_pipeline(&self.jewels);
            pass.draw(0..6, 0..gears);
            for i in 0..gears {
                pass.set_pipeline(&self.shadows);
                pass.draw(0..6, i..i + 1);
                pass.set_pipeline(&self.gears);
                pass.draw(0..6, i..i + 1);
            }
        }
        if !self.spark_data.is_empty() {
            pass.set_pipeline(&self.sparks);
            pass.set_vertex_buffer(0, self.spark_buffer.slice(..));
            pass.draw(0..6, 0..self.spark_data.len() as u32);
        }
        pass.set_pipeline(&self.glow);
        pass.draw(0..3, 0..1);
        pass.set_pipeline(&self.vignette);
        pass.draw(0..3, 0..1);
    }
}

fn new_sim() -> GearSim {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |elapsed| elapsed.as_nanos() as u64);
    GearSim::new(seed)
}

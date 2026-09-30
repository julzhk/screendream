//! Fractal circuits. Eight pens, one per frequency band, etch glowing space-filling curves
//! (see `render::fractal_curves`) onto a dark steel plate. Traces start white-hot, cool to
//! their band colour, then to embers, and fade away. The plate bends in 3D under the pen tips
//! and folds more and more as the screen fills; the camera tilts to show it off. Pen tips
//! carry targeting reticles and light the metal around them; a scan sweep, vignette and
//! scanlines finish the industrial monitor look.

use super::{
    ADDITIVE, FrameInput, MULTIPLY, ModeContext, PipelineSpec, VisualizerMode, pipeline,
    pipeline_with_blend, shader_with_common,
};
use crate::analysis::audio_features::SPECTRUM_BINS;
use crate::render::fractal_curves::{CurveDrawer, SlotWrite};
use wgpu::util::DeviceExt;

const PENS: usize = 8;
/// Ring-buffer capacity for trace segments; must outlast `SEGMENT_LIFETIME` at full speed.
const RING: u32 = 16_384;
/// Plate mesh resolution; must match `SURF_COLS`/`SURF_ROWS` in circuits.wgsl.
const SURF_COLS: u32 = 241;
const SURF_ROWS: u32 = 161;
const CAMERA_DISTANCE: f32 = 1.6;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CircuitParams {
    cam_pos: [f32; 3],
    coverage: f32,
    cam_right: [f32; 3],
    kick: f32,
    cam_up: [f32; 3],
    sweep: f32,
    cam_fwd: [f32; 3],
    /// Slowly smoothed bass, for the plate ripple.
    bass: f32,
    /// Per pen: tip xy, slowly smoothed level (plate bulge), fast level (glow).
    heads: [[f32; 4]; PENS],
}

/// One trace segment as the shader reads it (instance data).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuSegment {
    ends: [f32; 4],
    /// birth time, pen, width, via flag.
    meta: [f32; 4],
}

const EMPTY_SEGMENT: GpuSegment = GpuSegment {
    ends: [0.0; 4],
    meta: [-1e9, 0.0, 0.0, 0.0],
};

pub struct CircuitsMode {
    surface: wgpu::RenderPipeline,
    traces: wgpu::RenderPipeline,
    heads: wgpu::RenderPipeline,
    overlay: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    params_buffer: wgpu::Buffer,
    segments: wgpu::Buffer,
    surface_vertices: wgpu::Buffer,
    surface_indices: wgpu::Buffer,
    surface_index_count: u32,

    params: CircuitParams,
    drawer: CurveDrawer,
    clock: f32,
    last_time: Option<f32>,
    last_bass: f32,
    needs_clear: bool,
}

impl CircuitsMode {
    pub const NAME: &'static str = "Circuits";

    pub fn new(ctx: &ModeContext) -> Self {
        let device = ctx.device;
        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("circuit params"),
            size: std::mem::size_of::<CircuitParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("circuits"),
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
            label: Some("circuits"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buffer.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("circuits"),
            bind_group_layouts: &[ctx.controls_layout, &layout],
            push_constant_ranges: &[],
        });
        let module = shader_with_common(
            ctx,
            "circuits.wgsl",
            include_str!("../shaders/circuits.wgsl"),
        );

        let surface_attributes = wgpu::vertex_attr_array![0 => Float32x2];
        let surface_buffers = [wgpu::VertexBufferLayout {
            array_stride: 8,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &surface_attributes,
        }];
        let segment_attributes = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4];
        let segment_buffers = [wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuSegment>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &segment_attributes,
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
        let surface = pipeline(
            ctx,
            spec(
                "circuit plate",
                "vs_surface",
                "fs_surface",
                &surface_buffers,
                true,
            ),
        );
        let traces = pipeline_with_blend(
            ctx,
            spec(
                "circuit traces",
                "vs_trace",
                "fs_trace",
                &segment_buffers,
                false,
            ),
            ADDITIVE,
        );
        let heads = pipeline_with_blend(
            ctx,
            spec("circuit heads", "vs_head", "fs_head", &[], false),
            ADDITIVE,
        );
        let overlay = pipeline_with_blend(
            ctx,
            spec("circuit overlay", "vs_overlay", "fs_overlay", &[], false),
            MULTIPLY,
        );

        let segments = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("circuit segments"),
            contents: bytemuck::cast_slice(&vec![EMPTY_SEGMENT; RING as usize]),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let (vertices, indices) = grid(SURF_COLS, SURF_ROWS);
        let surface_vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("circuit plate vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let surface_indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("circuit plate indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            surface,
            traces,
            heads,
            overlay,
            bind_group,
            params_buffer,
            segments,
            surface_vertices,
            surface_indices,
            surface_index_count: indices.len() as u32,
            params: bytemuck::Zeroable::zeroed(),
            drawer: new_drawer(),
            clock: 0.0,
            last_time: None,
            last_bass: 0.0,
            needs_clear: false,
        }
    }

    fn write_segments(&self, queue: &wgpu::Queue, writes: &[SlotWrite]) {
        for write in writes {
            let s = write.segment;
            let gpu = GpuSegment {
                ends: [s.a[0], s.a[1], s.b[0], s.b[1]],
                meta: [
                    s.birth,
                    s.pen as f32,
                    s.width,
                    if s.via { 1.0 } else { 0.0 },
                ],
            };
            queue.write_buffer(
                &self.segments,
                write.slot as u64 * std::mem::size_of::<GpuSegment>() as u64,
                bytemuck::bytes_of(&gpu),
            );
        }
    }

    fn update_camera(&mut self, dt: f32) {
        self.clock += dt;
        let t = self.clock;
        // The fuller the screen, the further the view tilts to show the plate folding.
        let tilt = 0.1 + 0.5 * self.params.coverage;
        let yaw = tilt * 0.8 * (t * 0.07 + 1.0).sin();
        let pitch = tilt * (t * 0.11).sin();
        let pos = [
            CAMERA_DISTANCE * yaw.sin() * pitch.cos(),
            CAMERA_DISTANCE * pitch.sin(),
            CAMERA_DISTANCE * yaw.cos() * pitch.cos(),
        ];
        let fwd = normalize([-pos[0], -pos[1], -pos[2]]);
        let flat_right = normalize(cross(fwd, [0.0, 1.0, 0.0]));
        let flat_up = cross(flat_right, fwd);
        let roll = 0.04 * (t * 0.05).sin() * (1.0 + self.params.coverage);
        let (sr, cr) = roll.sin_cos();
        let right = [
            flat_right[0] * cr + flat_up[0] * sr,
            flat_right[1] * cr + flat_up[1] * sr,
            flat_right[2] * cr + flat_up[2] * sr,
        ];
        self.params.cam_pos = pos;
        self.params.cam_fwd = fwd;
        self.params.cam_right = right;
        self.params.cam_up = cross(right, fwd);
    }
}

impl VisualizerMode for CircuitsMode {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn activate(&mut self) {
        // Start from a clean plate each visit.
        self.drawer = new_drawer();
        self.needs_clear = true;
        self.last_time = None;
    }

    fn prepare(&mut self, queue: &wgpu::Queue, frame: &FrameInput) {
        if self.needs_clear {
            let empty = vec![EMPTY_SEGMENT; RING as usize];
            queue.write_buffer(&self.segments, 0, bytemuck::cast_slice(&empty));
            self.needs_clear = false;
        }
        let dt = self
            .last_time
            .map_or(0.0, |last| (frame.time - last).clamp(0.0, 0.1));
        self.last_time = Some(frame.time);

        let audio = frame.uniform;
        let bins_per_pen = SPECTRUM_BINS / PENS;
        let levels: Vec<f32> = (0..PENS)
            .map(|pen| {
                if !frame.active {
                    return 0.0;
                }
                let bins = &audio.spectrum[pen * bins_per_pen..(pen + 1) * bins_per_pen];
                bins.iter().sum::<f32>() / bins_per_pen as f32
            })
            .collect();

        let writes = self.drawer.advance(frame.time, dt, &levels);
        self.write_segments(queue, &writes);

        let bass = if frame.active { audio.bands[0] } else { 0.0 };
        let rise = (bass - self.last_bass).max(0.0);
        self.last_bass = bass;
        self.params.kick = (self.params.kick * (1.0 - dt * 4.0).max(0.0) + rise * 3.0).min(1.5);
        // Plate motion uses slow smoothing so the warp glides rather than jitters.
        let glide = (dt * 1.5).min(1.0);
        self.params.bass += (bass - self.params.bass) * glide;
        for (head, (tip, level)) in self.params.heads.iter_mut().zip(self.drawer.heads()) {
            head[0] = tip[0];
            head[1] = tip[1];
            head[2] += (level - head[2]) * glide;
            head[3] = level;
        }
        self.params.coverage = self.drawer.coverage();
        self.params.sweep = (frame.time * 0.12).fract();
        self.update_camera(dt);
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&self.params));
    }

    fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_bind_group(1, &self.bind_group, &[]);
        pass.set_pipeline(&self.surface);
        pass.set_vertex_buffer(0, self.surface_vertices.slice(..));
        pass.set_index_buffer(self.surface_indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.surface_index_count, 0, 0..1);

        pass.set_pipeline(&self.traces);
        pass.set_vertex_buffer(0, self.segments.slice(..));
        pass.draw(0..6, 0..RING);

        pass.set_pipeline(&self.heads);
        pass.draw(0..6, 0..PENS as u32);

        pass.set_pipeline(&self.overlay);
        pass.draw(0..3, 0..1);
    }
}

fn new_drawer() -> CurveDrawer {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |elapsed| elapsed.as_nanos() as u64);
    CurveDrawer::new(PENS, RING, seed)
}

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

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(a: [f32; 3]) -> [f32; 3] {
    let len = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt().max(1e-6);
    [a[0] / len, a[1] / len, a[2] / len]
}

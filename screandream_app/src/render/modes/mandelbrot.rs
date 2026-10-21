//! Flights into the Mandelbrot set and Julia sets, laid out as a landscape with lightning
//! arcing between its peaks. The dives, dissolves and bolts come from `render::mandel_sim`;
//! this mode draws the sky, then one terrain grid per dive (both while one dissolves into the
//! next), whose heights and colours the shader computes from the fractal, then the bolts.

use super::{
    ADDITIVE, FrameInput, ModeContext, PipelineSpec, VisualizerMode, pipeline, pipeline_with_depth,
    shader_with_common,
};
use crate::analysis::audio_features::SPECTRUM_BINS;
use crate::render::mandel_sim::{
    Dive, Drive, FOCAL, GRID_CELLS, MAX_LIGHTS, MAX_SEGMENTS, MandelSim, Set,
};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SceneParams {
    /// Camera position and focal length, forward and dissolve, right and flash, up and energy.
    camera: [[f32; 4]; 4],
    ring: [f32; 4],
    /// Three vec4s per dive; see `Dive` in mandelbrot.wgsl.
    dives: [[f32; 4]; 6],
    lights: [[f32; 4]; MAX_LIGHTS * 2],
    spectrum: [[f32; 4]; SPECTRUM_BINS / 4],
}

/// One bolt segment; see `vs_bolt` in mandelbrot.wgsl.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SegmentInstance {
    a: [f32; 4],
    b: [f32; 4],
    colour: [f32; 4],
}

const SEGMENT_ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4];

pub struct MandelbrotMode {
    sky: wgpu::RenderPipeline,
    terrain: wgpu::RenderPipeline,
    bolts: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    params_buffer: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    segment_buffer: wgpu::Buffer,
    segment_count: u32,
    dive_count: u32,
    params: SceneParams,
    sim: MandelSim,
    last_time: Option<f32>,
}

impl MandelbrotMode {
    pub const NAME: &'static str = "Mandelbrot";

    pub fn new(ctx: &ModeContext) -> Self {
        let device = ctx.device;
        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mandelbrot params"),
            size: std::mem::size_of::<SceneParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let segment_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mandelbrot bolts"),
            size: (MAX_SEGMENTS * std::mem::size_of::<SegmentInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mandelbrot"),
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
            label: Some("mandelbrot"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buffer.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mandelbrot"),
            bind_group_layouts: &[ctx.controls_layout, &layout],
            push_constant_ranges: &[],
        });
        let module = shader_with_common(
            ctx,
            "mandelbrot.wgsl",
            include_str!("../shaders/mandelbrot.wgsl"),
        );
        let grid_buffers = [wgpu::VertexBufferLayout {
            array_stride: 8,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x2],
        }];
        let segment_buffers = [wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SegmentInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &SEGMENT_ATTRIBUTES,
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
        let sky = pipeline(
            ctx,
            spec("mandelbrot sky", "vs_screen", "fs_sky", &[], false),
        );
        let terrain = pipeline(
            ctx,
            spec(
                "mandelbrot terrain",
                "vs_terrain",
                "fs_terrain",
                &grid_buffers,
                true,
            ),
        );
        // Bolts glow over what is behind them, hidden by the peaks in front.
        let bolts = pipeline_with_depth(
            ctx,
            spec(
                "mandelbrot bolts",
                "vs_bolt",
                "fs_bolt",
                &segment_buffers,
                false,
            ),
            ADDITIVE,
            wgpu::CompareFunction::Less,
        );

        let (vertices, indices) = grid(GRID_CELLS + 1);
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mandelbrot grid vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mandelbrot grid indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            sky,
            terrain,
            bolts,
            bind_group,
            params_buffer,
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
            segment_buffer,
            segment_count: 0,
            dive_count: 1,
            params: bytemuck::Zeroable::zeroed(),
            sim: new_sim(),
            last_time: None,
        }
    }
}

impl VisualizerMode for MandelbrotMode {
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
                spectrum: audio.spectrum,
            }
        } else {
            Drive::default()
        };
        self.sim.step(dt, &drive);

        let cam = self.sim.camera();
        let (current, next) = self.sim.dives();
        let p = &mut self.params;
        p.camera = [
            [cam.pos[0], cam.pos[1], cam.pos[2], FOCAL],
            [
                cam.forward[0],
                cam.forward[1],
                cam.forward[2],
                next.map_or(0.0, |(_, t)| t),
            ],
            [cam.right[0], cam.right[1], cam.right[2], self.sim.flash()],
            [cam.up[0], cam.up[1], cam.up[2], drive.energy],
        ];
        let [radius, strength] = self.sim.ring();
        p.ring = [radius, strength, 0.0, 0.0];
        let (first, second) = p.dives.split_at_mut(3);
        write_dive(first, current);
        write_dive(second, next.map_or(current, |(d, _)| d));
        self.dive_count = if next.is_some() { 2 } else { 1 };

        p.lights = [[0.0; 4]; MAX_LIGHTS * 2];
        for (light, slot) in self
            .sim
            .lights()
            .iter()
            .zip(p.lights.as_chunks_mut::<2>().0.iter_mut())
        {
            slot[0] = [light.pos[0], light.pos[1], light.pos[2], 0.0];
            slot[1] = [
                light.colour[0],
                light.colour[1],
                light.colour[2],
                light.intensity,
            ];
        }
        for (slot, bins) in p.spectrum.iter_mut().zip(drive.spectrum.as_chunks::<4>().0) {
            *slot = *bins;
        }
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&self.params));

        let segments: Vec<SegmentInstance> = self
            .sim
            .segments()
            .iter()
            .map(|s| SegmentInstance {
                a: [s.a[0], s.a[1], s.a[2], s.width],
                b: [s.b[0], s.b[1], s.b[2], s.intensity],
                colour: [s.colour[0], s.colour[1], s.colour[2], 0.0],
            })
            .collect();
        self.segment_count = segments.len() as u32;
        queue.write_buffer(&self.segment_buffer, 0, bytemuck::cast_slice(&segments));
    }

    fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_bind_group(1, &self.bind_group, &[]);
        pass.set_pipeline(&self.sky);
        pass.draw(0..3, 0..1);
        pass.set_pipeline(&self.terrain);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..self.dive_count);
        if self.segment_count > 0 {
            pass.set_pipeline(&self.bolts);
            pass.set_vertex_buffer(0, self.segment_buffer.slice(..));
            pass.draw(0..6, 0..self.segment_count);
        }
    }
}

fn write_dive(slot: &mut [[f32; 4]], dive: &Dive) {
    let (julia, kind) = match dive.set_now() {
        Set::Mandelbrot => ([0.0, 0.0], 0.0),
        Set::Julia(c) => ([c[0] as f32, c[1] as f32], 1.0),
    };
    slot[0] = [
        dive.centre[0] as f32,
        dive.centre[1] as f32,
        dive.span as f32,
        dive.rot,
    ];
    slot[1] = [julia[0], julia[1], kind, dive.iterations() as f32];
    let palette = dive.palette;
    slot[2] = [dive.phase, palette.hue, palette.range, palette.saturation];
}

/// A `side` × `side` grid of (column, row) vertices; the shader places them in the world.
fn grid(side: u32) -> (Vec<[f32; 2]>, Vec<u32>) {
    let vertices = (0..side)
        .flat_map(|row| (0..side).map(move |col| [col as f32, row as f32]))
        .collect();
    let mut indices = Vec::with_capacity(((side - 1) * (side - 1) * 6) as usize);
    for row in 0..side - 1 {
        for col in 0..side - 1 {
            let a = row * side + col;
            let b = a + side;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    (vertices, indices)
}

fn new_sim() -> MandelSim {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |elapsed| elapsed.as_nanos() as u64);
    MandelSim::new(seed)
}

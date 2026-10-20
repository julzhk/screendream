//! Platonic solids adrift in black fog, throwing bold beams of light from their faces. The
//! shapes and beams come from `render::beacon_sim`; this mode builds the five solids' meshes
//! once, then each frame draws the fog and floor, the solid shapes, the glass ones, the beams
//! (which integrate the fog's scattering per pixel) and the dust caught in them.

use super::{
    ADDITIVE, FrameInput, MULTIPLY, ModeContext, PipelineSpec, VisualizerMode, pipeline,
    pipeline_with_blend, pipeline_with_depth, shader_with_common,
};
use crate::render::beacon_sim::{
    BeaconSim, Drive, FOCAL, MAX_BEAMS, MAX_SHAPES, MOTES, SOLIDS, Shape, faces, matrix_q,
};
use std::ops::Range;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SceneParams {
    /// Camera position and focal length, forward and beam count, right and bass flash, up and
    /// energy.
    camera: [[f32; 4]; 4],
    /// Three vec4s per beam; see `Scene` in beacons.wgsl.
    beams: [[f32; 4]; MAX_BEAMS * 3],
}

/// A corner of a face's triangle fan; see `ShapeIn` in beacons.wgsl.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MeshVertex {
    pos: [f32; 3],
    normal: [f32; 3],
    /// 1 at the face centre, 0 on its rim; face index.
    extra: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ShapeInstance {
    m: [[f32; 4]; 3],
    centre: [f32; 4],
    colors: [[f32; 4]; 3],
}

const MESH_ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2];
const SHAPE_ATTRIBUTES: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![
    3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4,
    7 => Float32x4, 8 => Float32x4, 9 => Float32x4
];
const MOTE_ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x4];

pub struct BeaconsMode {
    fog: wgpu::RenderPipeline,
    shapes: wgpu::RenderPipeline,
    crystals: wgpu::RenderPipeline,
    beams: wgpu::RenderPipeline,
    motes: wgpu::RenderPipeline,
    grade: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    params_buffer: wgpu::Buffer,
    mesh_buffer: wgpu::Buffer,
    shape_buffer: wgpu::Buffer,
    mote_buffer: wgpu::Buffer,
    /// Vertex range of each solid in the mesh buffer, in `SOLIDS` order.
    ranges: Vec<Range<u32>>,
    params: Box<SceneParams>,
    /// Per drawn shape: its solid's vertex range; solid shapes come before crystals.
    draws: Vec<Range<u32>>,
    first_crystal: usize,
    beam_count: u32,
    sim: BeaconSim,
    last_time: Option<f32>,
}

impl BeaconsMode {
    pub const NAME: &'static str = "Beacons";

    pub fn new(ctx: &ModeContext) -> Self {
        let device = ctx.device;
        let (mesh, ranges) = build_mesh();
        let mesh_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("beacon solids"),
            size: std::mem::size_of_val(mesh.as_slice()) as u64,
            usage: wgpu::BufferUsages::VERTEX,
            mapped_at_creation: true,
        });
        mesh_buffer
            .slice(..)
            .get_mapped_range_mut()
            .copy_from_slice(bytemuck::cast_slice(&mesh));
        mesh_buffer.unmap();
        let buffer = |label, size: usize, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size as u64,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let params_buffer = buffer(
            "beacon params",
            std::mem::size_of::<SceneParams>(),
            wgpu::BufferUsages::UNIFORM,
        );
        let shape_buffer = buffer(
            "beacon shapes",
            MAX_SHAPES * std::mem::size_of::<ShapeInstance>(),
            wgpu::BufferUsages::VERTEX,
        );
        let mote_buffer = buffer(
            "beacon motes",
            MOTES * std::mem::size_of::<[f32; 4]>(),
            wgpu::BufferUsages::VERTEX,
        );
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("beacons"),
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
            label: Some("beacons"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buffer.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("beacons"),
            bind_group_layouts: &[ctx.controls_layout, &layout],
            push_constant_ranges: &[],
        });
        let module =
            shader_with_common(ctx, "beacons.wgsl", include_str!("../shaders/beacons.wgsl"));
        let shape_buffers = [
            wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<MeshVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &MESH_ATTRIBUTES,
            },
            wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<ShapeInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &SHAPE_ATTRIBUTES,
            },
        ];
        let mote_buffers = [wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<[f32; 4]>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &MOTE_ATTRIBUTES,
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
        let fog = pipeline(ctx, spec("beacon fog", "vs_screen", "fs_fog", &[], false));
        let shapes = pipeline(
            ctx,
            spec(
                "beacon shapes",
                "vs_shape",
                "fs_shape",
                &shape_buffers,
                true,
            ),
        );
        // Glass, beams and dust glow over what is behind them, hidden by what is in front.
        let crystals = pipeline_with_depth(
            ctx,
            spec(
                "beacon crystals",
                "vs_shape",
                "fs_crystal",
                &shape_buffers,
                false,
            ),
            ADDITIVE,
            wgpu::CompareFunction::Less,
        );
        let beams = pipeline_with_depth(
            ctx,
            spec("beacon beams", "vs_beam", "fs_beam", &[], false),
            ADDITIVE,
            wgpu::CompareFunction::Less,
        );
        let motes = pipeline_with_depth(
            ctx,
            spec("beacon motes", "vs_mote", "fs_mote", &mote_buffers, false),
            ADDITIVE,
            wgpu::CompareFunction::Less,
        );
        let grade = pipeline_with_blend(
            ctx,
            spec("beacon grade", "vs_screen", "fs_grade", &[], false),
            MULTIPLY,
        );

        Self {
            fog,
            shapes,
            crystals,
            beams,
            motes,
            grade,
            bind_group,
            params_buffer,
            mesh_buffer,
            shape_buffer,
            mote_buffer,
            ranges,
            params: Box::new(bytemuck::Zeroable::zeroed()),
            draws: Vec::with_capacity(MAX_SHAPES),
            first_crystal: 0,
            beam_count: 0,
            sim: new_sim(),
            last_time: None,
        }
    }
}

impl VisualizerMode for BeaconsMode {
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
        let beams = self.sim.beams();
        self.beam_count = beams.len() as u32;
        self.params.camera = [
            [cam.pos[0], cam.pos[1], cam.pos[2], FOCAL],
            [
                cam.forward[0],
                cam.forward[1],
                cam.forward[2],
                beams.len() as f32,
            ],
            [
                cam.right[0],
                cam.right[1],
                cam.right[2],
                self.sim.beat_flash(),
            ],
            [cam.up[0], cam.up[1], cam.up[2], drive.energy],
        ];
        for (b, slot) in beams
            .iter()
            .zip(self.params.beams.as_chunks_mut::<3>().0.iter_mut())
        {
            slot[0] = [b.origin[0], b.origin[1], b.origin[2], b.spread];
            slot[1] = [b.dir[0], b.dir[1], b.dir[2], b.aperture];
            slot[2] = [b.color[0], b.color[1], b.color[2], b.intensity];
        }
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&*self.params));

        // Solid shapes first, then crystals, which glow over everything behind them.
        let mut shapes: Vec<&Shape> = self.sim.shapes().iter().collect();
        shapes.sort_by_key(|s| s.crystal);
        self.first_crystal = shapes
            .iter()
            .position(|s| s.crystal)
            .unwrap_or(shapes.len());
        self.draws.clear();
        let instances: Vec<ShapeInstance> = shapes
            .iter()
            .map(|s| {
                self.draws.push(self.ranges[s.solid as usize].clone());
                let [c0, c1, c2] = matrix_q(s.rot).map(|c| c.map(|v| v * s.scale));
                let [p0, p1, p2] = s.palette;
                ShapeInstance {
                    m: [
                        [c0[0], c0[1], c0[2], s.crystal as u8 as f32],
                        [c1[0], c1[1], c1[2], s.emitters as f32],
                        [c2[0], c2[1], c2[2], s.fade],
                    ],
                    centre: [s.pos[0], s.pos[1], s.pos[2], s.intensity],
                    colors: [p0, p1, p2].map(|c| [c[0], c[1], c[2], 0.0]),
                }
            })
            .collect();
        queue.write_buffer(&self.shape_buffer, 0, bytemuck::cast_slice(&instances));
        let motes: Vec<[f32; 4]> = self
            .sim
            .motes()
            .iter()
            .map(|m| [m[0], m[1], m[2], 0.0])
            .collect();
        queue.write_buffer(&self.mote_buffer, 0, bytemuck::cast_slice(&motes));
    }

    fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_bind_group(1, &self.bind_group, &[]);
        pass.set_pipeline(&self.fog);
        pass.draw(0..3, 0..1);
        pass.set_vertex_buffer(0, self.mesh_buffer.slice(..));
        pass.set_vertex_buffer(1, self.shape_buffer.slice(..));
        for (i, range) in self.draws.iter().enumerate() {
            let pipeline = if i < self.first_crystal {
                &self.shapes
            } else {
                &self.crystals
            };
            pass.set_pipeline(pipeline);
            pass.draw(range.clone(), i as u32..i as u32 + 1);
        }
        if self.beam_count > 0 {
            pass.set_pipeline(&self.beams);
            pass.draw(0..6, 0..self.beam_count);
        }
        pass.set_pipeline(&self.motes);
        pass.set_vertex_buffer(0, self.mote_buffer.slice(..));
        pass.draw(0..6, 0..MOTES as u32);
        pass.set_pipeline(&self.grade);
        pass.draw(0..3, 0..1);
    }
}

/// Triangle fans for every face of the five solids, and each solid's vertex range.
fn build_mesh() -> (Vec<MeshVertex>, Vec<Range<u32>>) {
    let mut vertices = Vec::new();
    let mut ranges = Vec::new();
    for solid in SOLIDS {
        let start = vertices.len() as u32;
        for (k, face) in faces(solid).iter().enumerate() {
            let n = face.corners.len();
            for i in 0..n {
                let (a, b) = (face.corners[i], face.corners[(i + 1) % n]);
                for (pos, edge) in [(face.center, 1.0), (a, 0.0), (b, 0.0)] {
                    vertices.push(MeshVertex {
                        pos,
                        normal: face.normal,
                        extra: [edge, k as f32],
                    });
                }
            }
        }
        ranges.push(start..vertices.len() as u32);
    }
    (vertices, ranges)
}

fn new_sim() -> BeaconSim {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |elapsed| elapsed.as_nanos() as u64);
    BeaconSim::new(seed)
}

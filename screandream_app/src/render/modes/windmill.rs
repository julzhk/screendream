//! Summer windmill fields. The camera glides, swoops and banks over rolling farmland dotted
//! with windmills. Each windmill listens to one frequency bin: its sails spin faster and
//! grow longer as that frequency gets louder (bass mills are big and stately, treble mills
//! small and quick). Drifting clouds cast shadows, wind waves roll through the crops, pollen
//! glints in the sunlight and hot-air balloons and birds share the sky.

use super::{FrameInput, ModeContext, PipelineSpec, VisualizerMode, pipeline, shader_with_common};
use crate::analysis::audio_features::SPECTRUM_BINS;
use std::f32::consts::TAU;
use wgpu::util::DeviceExt;

/// Ground grid resolution; must match `GROUND_COLS` in windmill.wgsl.
const GROUND_COLS: u32 = 321;
/// Windmill instances: a 32 × 32 block of cells around the camera (`MILL_GRID` in the shader).
const MILL_INSTANCES: u32 = 32 * 32;
const POLLEN: u32 = 500;

/// Minimum camera altitude; keeps the flight above the tallest hills and towers.
const MIN_ALTITUDE: f32 = 26.0;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MeadowParams {
    cam_pos: [f32; 3],
    /// 0 = bright midday, 1 = golden late afternoon.
    warmth: f32,
    cam_right: [f32; 3],
    kick: f32,
    cam_up: [f32; 3],
    /// Accumulated wind phase; runs faster with the mids.
    wind: f32,
    cam_fwd: [f32; 3],
    _pad: f32,
    /// Sail angle for each spectrum bin.
    angles: [[f32; 4]; SPECTRUM_BINS / 4],
    /// Smoothed level for each spectrum bin.
    levels: [[f32; 4]; SPECTRUM_BINS / 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MillVertex {
    pos: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
    part: f32,
}

/// Parts of the windmill mesh, as numbered in the shader.
#[derive(Clone, Copy)]
enum Part {
    Tower = 0,
    Cap = 1,
    Cloth = 2,
    Spar = 3,
    Hub = 4,
}

/// Height of the sail hub above the tower base, in mesh units (`HUB` in the shader).
const HUB: [f32; 3] = [0.0, 10.2, 1.9];

struct Flight {
    clock: f32,
    pos: [f32; 3],
    heading: f32,
    speed: f32,
    pitch: f32,
    roll: f32,
    /// Smoothed vertical speed, used to tilt the nose into climbs and dives.
    climb: f32,
}

impl Flight {
    fn new() -> Self {
        Self {
            clock: 0.0,
            pos: [0.0, 46.0, 0.0],
            heading: 0.0,
            speed: 12.0,
            pitch: -0.2,
            roll: 0.0,
            climb: 0.0,
        }
    }
}

pub struct WindmillMode {
    sky: wgpu::RenderPipeline,
    ground: wgpu::RenderPipeline,
    mills: wgpu::RenderPipeline,
    pollen: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    params_buffer: wgpu::Buffer,
    ground_vertices: wgpu::Buffer,
    ground_indices: wgpu::Buffer,
    ground_index_count: u32,
    mill_vertices: wgpu::Buffer,
    mill_indices: wgpu::Buffer,
    mill_index_count: u32,

    params: MeadowParams,
    flight: Flight,
    last_time: Option<f32>,
    last_bass: f32,
}

impl WindmillMode {
    pub const NAME: &'static str = "Windmills";

    pub fn new(ctx: &ModeContext) -> Self {
        let device = ctx.device;
        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("meadow params"),
            size: std::mem::size_of::<MeadowParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("meadow"),
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
            label: Some("meadow"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buffer.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("meadow"),
            bind_group_layouts: &[ctx.controls_layout, &layout],
            push_constant_ranges: &[],
        });

        let module = shader_with_common(
            ctx,
            "windmill.wgsl",
            include_str!("../shaders/windmill.wgsl"),
        );
        let ground_attributes = wgpu::vertex_attr_array![0 => Float32x2];
        let ground_layout = wgpu::VertexBufferLayout {
            array_stride: 8,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ground_attributes,
        };
        let mill_attributes =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32];
        let mill_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<MillVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &mill_attributes,
        };
        let (ground_buffers, mill_buffers) = ([ground_layout], [mill_layout]);
        let spec = |label, vs, fs, buffers, depth_write| PipelineSpec {
            label,
            layout: &pipeline_layout,
            shader: &module,
            vs,
            fs,
            buffers,
            depth_write,
        };
        let sky = pipeline(ctx, spec("meadow sky", "vs_sky", "fs_sky", &[], false));
        let ground = pipeline(
            ctx,
            spec(
                "meadow ground",
                "vs_ground",
                "fs_ground",
                &ground_buffers,
                true,
            ),
        );
        let mills = pipeline(
            ctx,
            spec("windmills", "vs_mill", "fs_mill", &mill_buffers, true),
        );
        let pollen = pipeline(ctx, spec("pollen", "vs_pollen", "fs_pollen", &[], true));

        let (grid_vertices, grid_indices) = grid(GROUND_COLS);
        let (mill_vertices, mill_indices) = windmill_mesh();
        let buffer = |label, contents: &[u8], usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage,
            })
        };

        Self {
            sky,
            ground,
            mills,
            pollen,
            bind_group,
            params_buffer,
            ground_vertices: buffer(
                "meadow ground vertices",
                bytemuck::cast_slice(&grid_vertices),
                wgpu::BufferUsages::VERTEX,
            ),
            ground_indices: buffer(
                "meadow ground indices",
                bytemuck::cast_slice(&grid_indices),
                wgpu::BufferUsages::INDEX,
            ),
            ground_index_count: grid_indices.len() as u32,
            mill_vertices: buffer(
                "windmill vertices",
                bytemuck::cast_slice(&mill_vertices),
                wgpu::BufferUsages::VERTEX,
            ),
            mill_indices: buffer(
                "windmill indices",
                bytemuck::cast_slice(&mill_indices),
                wgpu::BufferUsages::INDEX,
            ),
            mill_index_count: mill_indices.len() as u32,
            params: bytemuck::Zeroable::zeroed(),
            flight: Flight::new(),
            last_time: None,
            last_bass: 0.0,
        }
    }

    fn fly(&mut self, dt: f32, bass: f32, amplitude: f32, active: bool) {
        let kick = self.params.kick;
        let f = &mut self.flight;
        f.clock += dt;
        let t = f.clock;

        // Lazy S-turns that occasionally tighten into a sweeping bank.
        let turn = 0.2 * (t * 0.07).sin() + 0.13 * (t * 0.19 + 1.0).sin() + 0.08 * (t * 0.41).sin();
        f.heading += turn * dt;

        let target_speed = if active {
            11.0 + 30.0 * bass.powf(1.5) + 10.0 * amplitude + 14.0 * kick
        } else {
            6.0
        };
        f.speed += (target_speed - f.speed) * (dt * 1.2).min(1.0);
        f.pos[0] += f.heading.sin() * f.speed * dt;
        f.pos[2] += f.heading.cos() * f.speed * dt;

        // Swoop: slow, shallow climbs and dives. Altitude and the resulting nose tilt are
        // both low-passed so the motion stays glassy smooth.
        let target_altitude =
            (46.0 + 5.0 * (t * 0.09).sin() + 2.0 * (t * 0.23 + 2.0).sin()).max(MIN_ALTITUDE);
        let previous = f.pos[1];
        f.pos[1] += (target_altitude - previous) * (dt * 0.6).min(1.0);
        if dt > 0.0 {
            let climb = (f.pos[1] - previous) / dt;
            f.climb += (climb - f.climb) * (dt * 1.0).min(1.0);
        }
        let target_pitch =
            -0.2 - (f.pos[1] - MIN_ALTITUDE) * 0.004 + (f.climb / f.speed.max(1.0)).atan() * 0.4;
        f.pitch += (target_pitch - f.pitch) * (dt * 0.8).min(1.0);
        f.roll += ((-turn * 1.1).clamp(-0.4, 0.4) - f.roll) * (dt * 2.0).min(1.0);

        let (sh, ch) = f.heading.sin_cos();
        let (sp, cp) = f.pitch.sin_cos();
        let fwd = [sh * cp, sp, ch * cp];
        let flat_right = normalize([fwd[2], 0.0, -fwd[0]]);
        let flat_up = cross(fwd, flat_right);
        let (sr, cr) = f.roll.sin_cos();
        let right = normalize(add(scale(flat_right, cr), scale(flat_up, sr)));

        self.params.cam_pos = f.pos;
        self.params.cam_fwd = fwd;
        self.params.cam_right = right;
        self.params.cam_up = cross(fwd, right);
    }
}

impl VisualizerMode for WindmillMode {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn activate(&mut self) {
        // Keep coordinates small for shader precision, and restart the flight.
        self.flight = Flight::new();
        self.last_time = None;
    }

    fn prepare(&mut self, queue: &wgpu::Queue, frame: &FrameInput) {
        let dt = self
            .last_time
            .map_or(0.0, |last| (frame.time - last).clamp(0.0, 0.1));
        self.last_time = Some(frame.time);

        let audio = frame.uniform;
        let bass = if frame.active { audio.bands[0] } else { 0.0 };
        let rise = (bass - self.last_bass).max(0.0);
        self.last_bass = bass;
        self.params.kick = (self.params.kick * (1.0 - dt * 4.0).max(0.0) + rise * 3.0).min(1.5);

        for bin in 0..SPECTRUM_BINS {
            let target = if frame.active {
                audio.spectrum[bin]
            } else {
                0.0
            };
            let level = &mut self.params.levels[bin / 4][bin % 4];
            *level += (target - *level) * (dt * 6.0).min(1.0);
            let angle = &mut self.params.angles[bin / 4][bin % 4];
            *angle = (*angle + dt * (0.35 + 7.5 * *level)) % TAU;
        }

        let mids = if frame.active { audio.bands[1] } else { 0.0 };
        self.params.wind += dt * (0.6 + 2.5 * mids);
        // A slow drift from bright midday to golden hour and back, every few minutes.
        self.params.warmth = 0.5 - 0.5 * (frame.time * 0.025).cos();

        self.fly(dt, bass, audio.amplitude, frame.active);
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&self.params));
    }

    fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_bind_group(1, &self.bind_group, &[]);
        pass.set_pipeline(&self.sky);
        pass.draw(0..3, 0..1);

        pass.set_pipeline(&self.ground);
        pass.set_vertex_buffer(0, self.ground_vertices.slice(..));
        pass.set_index_buffer(self.ground_indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.ground_index_count, 0, 0..1);

        pass.set_pipeline(&self.mills);
        pass.set_vertex_buffer(0, self.mill_vertices.slice(..));
        pass.set_index_buffer(self.mill_indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.mill_index_count, 0, 0..MILL_INSTANCES);

        pass.set_pipeline(&self.pollen);
        pass.draw(0..6, 0..POLLEN);
    }
}

/// A square `cols` × `cols` grid of (column, row) vertices centred by the shader.
fn grid(cols: u32) -> (Vec<[f32; 2]>, Vec<u32>) {
    let vertices = (0..cols)
        .flat_map(|row| (0..cols).map(move |col| [col as f32, row as f32]))
        .collect();
    let mut indices = Vec::with_capacity(((cols - 1) * (cols - 1) * 6) as usize);
    for row in 0..cols - 1 {
        for col in 0..cols - 1 {
            let a = row * cols + col;
            let b = a + cols;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    (vertices, indices)
}

#[derive(Default)]
struct MeshBuilder {
    vertices: Vec<MillVertex>,
    indices: Vec<u32>,
}

impl MeshBuilder {
    /// Adds a flat quad (corners in order around the face) with the given uv corners.
    fn quad(&mut self, corners: [[f32; 3]; 4], uvs: [[f32; 2]; 4], part: Part) {
        let normal = normalize(cross(
            sub(corners[1], corners[0]),
            sub(corners[3], corners[0]),
        ));
        let base = self.vertices.len() as u32;
        for (pos, uv) in corners.into_iter().zip(uvs) {
            self.vertices.push(MillVertex {
                pos,
                normal,
                uv,
                part: part as u32 as f32,
            });
        }
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// A tapered many-sided column (tower or cap) from `y0` to `y1`.
    fn frustum(&mut self, sides: u32, y0: f32, r0: f32, y1: f32, r1: f32, part: Part) {
        for side in 0..sides {
            let a0 = side as f32 / sides as f32 * TAU;
            let a1 = (side + 1) as f32 / sides as f32 * TAU;
            let at = |a: f32, r: f32, y: f32| [a.sin() * r, y, a.cos() * r];
            let u0 = side as f32 / sides as f32;
            let u1 = (side + 1) as f32 / sides as f32;
            self.quad(
                [
                    at(a0, r0, y0),
                    at(a1, r0, y0),
                    at(a1, r1, y1),
                    at(a0, r1, y1),
                ],
                [[u0, 0.0], [u1, 0.0], [u1, 1.0], [u0, 1.0]],
                part,
            );
        }
    }

    /// An axis-aligned box given by its centre and half-extents.
    fn cuboid(&mut self, centre: [f32; 3], half: [f32; 3], part: Part) {
        let [cx, cy, cz] = centre;
        let [hx, hy, hz] = half;
        let p = |x: f32, y: f32, z: f32| [cx + x * hx, cy + y * hy, cz + z * hz];
        let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        for face in [
            [
                p(-1., -1., 1.),
                p(1., -1., 1.),
                p(1., 1., 1.),
                p(-1., 1., 1.),
            ],
            [
                p(1., -1., -1.),
                p(-1., -1., -1.),
                p(-1., 1., -1.),
                p(1., 1., -1.),
            ],
            [
                p(1., -1., 1.),
                p(1., -1., -1.),
                p(1., 1., -1.),
                p(1., 1., 1.),
            ],
            [
                p(-1., -1., -1.),
                p(-1., -1., 1.),
                p(-1., 1., 1.),
                p(-1., 1., -1.),
            ],
            [
                p(-1., 1., 1.),
                p(1., 1., 1.),
                p(1., 1., -1.),
                p(-1., 1., -1.),
            ],
            [
                p(-1., -1., -1.),
                p(1., -1., -1.),
                p(1., -1., 1.),
                p(-1., -1., 1.),
            ],
        ] {
            self.quad(face, uv, part);
        }
    }
}

/// A Dutch smock mill: tapered octagonal tower, domed cap, and four lattice sails. Sail
/// vertices (cloth, spars, hub) are relative to the hub so the shader can spin and scale them.
fn windmill_mesh() -> (Vec<MillVertex>, Vec<u32>) {
    let mut m = MeshBuilder::default();
    m.frustum(8, 0.0, 2.3, 8.6, 1.35, Part::Tower);
    // Gallery ring around the tower, then the cap.
    m.frustum(8, 5.2, 2.3, 5.45, 2.3, Part::Cap);
    m.frustum(8, 8.6, 1.6, 10.0, 1.7, Part::Cap);
    m.frustum(8, 10.0, 1.7, 11.4, 0.6, Part::Cap);
    m.frustum(8, 11.4, 0.6, 11.7, 0.0, Part::Cap);
    // Windshaft from the cap to the hub.
    m.cuboid(
        [0.0, HUB[1], HUB[2] * 0.5],
        [0.22, 0.22, HUB[2] * 0.5],
        Part::Cap,
    );

    m.cuboid([0.0, 0.0, 0.0], [0.45, 0.45, 0.3], Part::Hub);
    for blade in 0..4 {
        let a = blade as f32 * TAU / 4.0;
        let radial = [a.cos(), a.sin(), 0.0];
        let across = [-a.sin(), a.cos(), 0.0];
        let at =
            |r: f32, s: f32, z: f32| add(add(scale(radial, r), scale(across, s)), [0.0, 0.0, z]);
        // The stock: a long spar from the hub to the sail tip.
        let (r0, r1, w, d) = (0.3, 8.6, 0.13, 0.13);
        m.quad(
            [at(r0, -w, d), at(r1, -w, d), at(r1, w, d), at(r0, w, d)],
            unit_uv(),
            Part::Spar,
        );
        m.quad(
            [at(r0, w, -d), at(r1, w, -d), at(r1, -w, -d), at(r0, -w, -d)],
            unit_uv(),
            Part::Spar,
        );
        m.quad(
            [at(r0, w, d), at(r1, w, d), at(r1, w, -d), at(r0, w, -d)],
            unit_uv(),
            Part::Spar,
        );
        m.quad(
            [at(r0, -w, -d), at(r1, -w, -d), at(r1, -w, d), at(r0, -w, d)],
            unit_uv(),
            Part::Spar,
        );
        // The cloth on one side of the stock, slightly twisted along its length.
        m.quad(
            [
                at(1.7, 0.18, 0.05),
                at(8.3, 0.18, 0.05),
                at(8.3, 1.75, 0.3),
                at(1.7, 1.75, 0.15),
            ],
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            Part::Cloth,
        );
    }
    (m.vertices, m.indices)
}

fn unit_uv() -> [[f32; 2]; 4] {
    [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
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
    scale(a, 1.0 / len)
}

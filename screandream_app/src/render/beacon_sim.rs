//! Platonic solids drifting through black fog, throwing beams of coloured light from their
//! faces. Kept free of GPU types so the rules can be unit-tested; `render::modes::beacons`
//! draws the result.
//!
//! Each shape follows its own slow, curving path: it slides in from beyond the edge of the
//! view (or out of the depths of the fog), wanders through and leaves on the far side, to be
//! replaced by another. Every shape listens to one frequency band: its spin quickens with that
//! band and the music's energy, and now and then an onset throws its spin into reverse. Beam
//! brightness changes gradually: it follows slow envelopes of the band and the energy (swelling
//! over about a second, fading over two), onsets add a gentle swell rather than a flash, and
//! each face's beam breathes in its own slow rhythm, quicker when the music is energetic.
//!
//! Solid shapes light up some of their faces edge to edge, each pouring out a beam as wide as
//! the face that spreads at its own angle, from a tight spot to a broad flood; a few shapes
//! fire fine lasers instead. Crystal shapes are glass with a light at their heart: each beam
//! leaves from the centre and is split by dispersion into a fan of spectral colours.

use crate::render::sim_math::{Rng, add, cross, dot, length, normalize, scale, sub};
use std::f32::consts::TAU;

pub const MAX_SHAPES: usize = 35;
/// How many of the shapes are glass.
pub const CRYSTALS: usize = 6;
pub const MAX_BEAMS: usize = 240;
pub const MOTES: usize = 500;
pub const BANDS: usize = 4;
/// The most faces any solid has (the icosahedron).
pub const MAX_FACES: usize = 20;
/// Focal length: the view spans y ∈ [-1, 1] at unit distance times this.
pub const FOCAL: f32 = 1.7;
/// Beyond this the fog swallows everything.
pub const FOG_FAR: f32 = 30.0;
/// Height of the wet floor far below.
pub const FLOOR_Y: f32 = -3.4;
pub const BEAM_LENGTH: f32 = 20.0;
/// Bold beam colours.
pub const COLOURS: [[f32; 3]; 8] = [
    [1.0, 0.08, 0.55], // magenta
    [0.05, 0.85, 1.0], // cyan
    [1.0, 0.55, 0.02], // amber
    [0.45, 1.0, 0.08], // lime
    [1.0, 0.07, 0.04], // red
    [0.12, 0.28, 1.0], // electric blue
    [0.6, 0.15, 1.0],  // violet
    [1.0, 0.9, 0.7],   // warm white
];
/// The spectrum a crystal splits its light into, red to violet.
pub const SPECTRUM: [[f32; 3]; 5] = [
    [1.0, 0.05, 0.02],
    [1.0, 0.75, 0.0],
    [0.15, 1.0, 0.1],
    [0.0, 0.6, 1.0],
    [0.5, 0.05, 1.0],
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Solid {
    Tetrahedron = 0,
    Cube = 1,
    Octahedron = 2,
    Dodecahedron = 3,
    Icosahedron = 4,
}

pub const SOLIDS: [Solid; 5] = [
    Solid::Tetrahedron,
    Solid::Cube,
    Solid::Octahedron,
    Solid::Dodecahedron,
    Solid::Icosahedron,
];

/// One face of a solid of circumradius 1: its centre, outward normal and corners in order.
#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    pub center: [f32; 3],
    pub normal: [f32; 3],
    pub corners: Vec<[f32; 3]>,
}

/// The faces of `solid`, scaled to circumradius 1. Each solid's face normals point at the
/// vertices of its dual, so faces are found as the vertices furthest along those directions.
pub fn faces(solid: Solid) -> Vec<Face> {
    let phi = (1.0 + 5.0f32.sqrt()) / 2.0;
    let ip = 1.0 / phi;
    let tetra = vec![
        [1.0, 1.0, 1.0],
        [1.0, -1.0, -1.0],
        [-1.0, 1.0, -1.0],
        [-1.0, -1.0, 1.0],
    ];
    let mut cube = Vec::new();
    for x in [-1.0, 1.0] {
        for y in [-1.0, 1.0] {
            for z in [-1.0, 1.0] {
                cube.push([x, y, z]);
            }
        }
    }
    let octa = vec![
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
    ];
    // An icosahedron and dodecahedron in dual orientations: each one's face normals point
    // at the other's vertices.
    let mut icosa_dual = Vec::new();
    for a in [-1.0, 1.0] {
        for b in [-phi, phi] {
            icosa_dual.push([0.0, b, a]);
            icosa_dual.push([b, a, 0.0]);
            icosa_dual.push([a, 0.0, b]);
        }
    }
    let mut dodeca = cube.clone();
    for a in [-ip, ip] {
        for b in [-phi, phi] {
            dodeca.push([0.0, a, b]);
            dodeca.push([a, b, 0.0]);
            dodeca.push([b, 0.0, a]);
        }
    }
    let (vertices, normals): (Vec<[f32; 3]>, Vec<[f32; 3]>) = match solid {
        Solid::Tetrahedron => (
            tetra.clone(),
            tetra.iter().map(|v| scale(*v, -1.0)).collect(),
        ),
        Solid::Cube => (cube.clone(), octa.clone()),
        Solid::Octahedron => (octa.clone(), cube.clone()),
        Solid::Dodecahedron => (dodeca.clone(), icosa_dual.clone()),
        Solid::Icosahedron => (icosa_dual.clone(), dodeca.clone()),
    };
    let vertices: Vec<[f32; 3]> = vertices.into_iter().map(normalize).collect();
    normals
        .into_iter()
        .map(|n| {
            let n = normalize(n);
            let best = vertices.iter().map(|v| dot(*v, n)).fold(f32::MIN, f32::max);
            let mut corners: Vec<[f32; 3]> = vertices
                .iter()
                .copied()
                .filter(|v| dot(*v, n) > best - 1e-3)
                .collect();
            let center = scale(
                corners.iter().fold([0.0; 3], |a, v| add(a, *v)),
                1.0 / corners.len() as f32,
            );
            // Order the corners anticlockwise seen from outside.
            let u = normalize(sub(corners[0], center));
            let w = cross(n, u);
            corners.sort_by(|a, b| {
                let angle = |p: &[f32; 3]| {
                    let d = sub(*p, center);
                    dot(d, w).atan2(dot(d, u))
                };
                angle(a).total_cmp(&angle(b))
            });
            Face {
                center,
                normal: n,
                corners,
            }
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub solid: Solid,
    pub crystal: bool,
    pub pos: [f32; 3],
    /// Orientation quaternion (x, y, z, w).
    pub rot: [f32; 4],
    pub scale: f32,
    pub palette: [[f32; 3]; 3],
    /// Bit `i` set: face `i` throws a beam.
    pub emitters: u32,
    /// Each face's beam half-angle, from tight spots to broad floods.
    pub half_angles: [f32; MAX_FACES],
    /// Fires thin lasers instead of beams as wide as its faces.
    pub laser: bool,
    /// Which band it listens to.
    pub band: usize,
    /// Current spin: unit axis and signed speed (radians per second).
    spin_axis: [f32; 3],
    pub spin: f32,
    spin_dir: f32,
    path: [[f32; 3]; 4],
    pub age: f32,
    life: f32,
    pub fade: f32,
    /// Beam brightness, swelling with the band and flashing on its onsets.
    pub intensity: f32,
    flash: f32,
    pub seed: f32,
}

/// One cone of light.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Beam {
    pub origin: [f32; 3],
    pub dir: [f32; 3],
    pub color: [f32; 3],
    /// Tangent of the cone's half-angle.
    pub spread: f32,
    /// Radius where the beam starts: the size of its face, or a point for lasers and crystals.
    pub aperture: f32,
    pub intensity: f32,
    pub length: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub pos: [f32; 3],
    pub forward: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Drive {
    pub bands: [f32; BANDS],
    pub energy: f32,
}

pub struct BeaconSim {
    shapes: Vec<Shape>,
    beams: Vec<Beam>,
    motes: Vec<[f32; 3]>,
    faces: [Vec<Face>; 5],
    rng: Rng,
    time: f32,
    camera: Camera,
    levels: [f32; BANDS],
    /// Slow envelopes of each band and of the energy, which beam brightness follows.
    swell: [f32; BANDS],
    swell_energy: f32,
    /// Phase of the faces' breathing, advancing faster when the music is energetic.
    breath: f32,
    last_level: [f32; BANDS],
    kick: [f32; BANDS],
    last_kick: [f32; BANDS],
    cooldown: [f32; BANDS],
    beat_flash: f32,
    reversals: u32,
}

impl BeaconSim {
    pub fn new(seed: u64) -> Self {
        let mut sim = Self {
            shapes: Vec::with_capacity(MAX_SHAPES),
            beams: Vec::with_capacity(MAX_BEAMS),
            motes: Vec::with_capacity(MOTES),
            faces: SOLIDS.map(faces),
            rng: Rng::new(seed),
            time: 0.0,
            camera: Camera {
                pos: [0.0; 3],
                forward: [0.0, 0.0, -1.0],
                right: [1.0, 0.0, 0.0],
                up: [0.0, 1.0, 0.0],
            },
            levels: [0.0; BANDS],
            last_level: [0.0; BANDS],
            kick: [0.0; BANDS],
            last_kick: [0.0; BANDS],
            cooldown: [0.0; BANDS],
            beat_flash: 0.0,
            swell: [0.0; BANDS],
            swell_energy: 0.0,
            breath: 0.0,
            reversals: 0,
        };
        for i in 0..MAX_SHAPES {
            // A few crystals among them, and all spread along their paths already.
            let mut shape = sim.new_shape(i < CRYSTALS);
            shape.age = shape.life * sim.rng.uniform(0.1, 0.85);
            sim.shapes.push(shape);
        }
        for _ in 0..MOTES {
            let mote = sim.new_mote();
            sim.motes.push(mote);
        }
        sim.update_shapes(0.0, 0.0, &[None; BANDS]);
        sim.build_beams();
        sim
    }

    pub fn shapes(&self) -> &[Shape] {
        &self.shapes
    }

    pub fn beams(&self) -> &[Beam] {
        &self.beams
    }

    pub fn motes(&self) -> &[[f32; 3]] {
        &self.motes
    }

    pub fn camera(&self) -> Camera {
        self.camera
    }

    pub fn faces(&self, solid: Solid) -> &[Face] {
        &self.faces[solid as usize]
    }

    /// 1 on a bass onset, decaying.
    pub fn beat_flash(&self) -> f32 {
        self.beat_flash
    }

    pub fn reversals(&self) -> u32 {
        self.reversals
    }

    pub fn step(&mut self, dt: f32, drive: Drive) {
        self.time += dt;
        let energy = drive.energy.clamp(0.0, 1.0);
        for (level, band) in self.levels.iter_mut().zip(drive.bands) {
            *level += (band.clamp(0.0, 1.0) - *level) * (dt * 6.0).min(1.0);
        }
        for (swell, band) in self.swell.iter_mut().zip(drive.bands) {
            *swell = follow_slowly(*swell, band.clamp(0.0, 1.0), dt);
        }
        self.swell_energy = follow_slowly(self.swell_energy, energy, dt);
        self.breath += dt * (0.35 + 0.9 * self.swell_energy);
        let onsets = self.detect_onsets(dt, &drive.bands);
        self.beat_flash = (self.beat_flash - dt * 3.0).max(0.0);
        if let Some(strength) = onsets[0] {
            self.beat_flash = strength.min(1.0);
        }
        self.move_camera();
        self.update_shapes(dt, energy, &onsets);
        self.build_beams();
        self.drift_motes(dt);
    }

    fn detect_onsets(&mut self, dt: f32, levels: &[f32; BANDS]) -> [Option<f32>; BANDS] {
        const COOLDOWN: [f32; BANDS] = [0.25, 0.2, 0.15, 0.12];
        std::array::from_fn(|b| {
            let level = levels[b].clamp(0.0, 1.0);
            let rise = (level - self.last_level[b]).max(0.0);
            self.last_level[b] = level;
            self.kick[b] = (self.kick[b] * (1.0 - dt * 4.0).max(0.0) + rise * 3.0).min(1.5);
            self.cooldown[b] -= dt;
            let hit = self.kick[b] - self.last_kick[b] > 0.08
                && self.kick[b] > 0.25
                && self.cooldown[b] <= 0.0;
            self.last_kick[b] = self.kick[b];
            hit.then(|| {
                self.cooldown[b] = COOLDOWN[b];
                self.kick[b].min(1.5)
            })
        })
    }

    /// The camera barely moves: a slow sway, as if adrift in the fog.
    fn move_camera(&mut self) {
        let t = self.time;
        let pos = [
            0.4 * (t * 0.05).sin(),
            0.25 * (t * 0.07 + 1.0).sin(),
            0.5 * (t * 0.03).sin(),
        ];
        let target = [1.2 * (t * 0.04).sin(), 0.6 * (t * 0.055).sin(), -12.0];
        let forward = normalize(sub(target, pos));
        let roll = 0.06 * (t * 0.045).sin();
        let flat_right = normalize(cross(forward, [0.0, 1.0, 0.0]));
        let flat_up = cross(flat_right, forward);
        let (sin, cos) = roll.sin_cos();
        self.camera = Camera {
            pos,
            forward,
            right: add(scale(flat_right, cos), scale(flat_up, sin)),
            up: sub(scale(flat_up, cos), scale(flat_right, sin)),
        };
    }

    fn new_shape(&mut self, crystal: bool) -> Shape {
        let rng = &mut self.rng;
        let solid = SOLIDS[rng.pick(SOLIDS.len())];
        let face_count = self.faces[solid as usize].len();
        let laser = !crystal && rng.next_f32() < 0.35;
        // Which faces shine: about half of them, fewer on the many-faced solids.
        let want = match face_count {
            4 => 3,
            6 => 4,
            8 => 4,
            12 => 6,
            _ => 8,
        }
        .min(if crystal { 3 } else { 99 });
        let mut emitters = 0u32;
        while (emitters.count_ones() as usize) < want {
            emitters |= 1 << rng.pick(face_count);
        }
        let mut palette = [[0.0; 3]; 3];
        for c in &mut palette {
            *c = COLOURS[rng.pick(COLOURS.len())];
        }
        let size = rng.uniform(0.7, 1.6);
        let path = new_path(rng, size);
        Shape {
            solid,
            crystal,
            pos: path[0],
            rot: normalize4([
                rng.uniform(-1.0, 1.0),
                rng.uniform(-1.0, 1.0),
                rng.uniform(-1.0, 1.0),
                rng.uniform(-1.0, 1.0),
            ]),
            scale: size,
            palette,
            emitters,
            half_angles: std::array::from_fn(|_| beam_angle(rng, laser, crystal)),
            laser,
            band: rng.pick(BANDS),
            spin_axis: random_unit(rng),
            spin: 0.0,
            spin_dir: if rng.next_f32() < 0.5 { 1.0 } else { -1.0 },
            path,
            age: 0.0,
            life: rng.uniform(22.0, 40.0),
            fade: 0.0,
            intensity: 0.5,
            flash: 0.0,
            seed: rng.next_f32(),
        }
    }

    fn update_shapes(&mut self, dt: f32, energy: f32, onsets: &[Option<f32>; BANDS]) {
        for i in 0..self.shapes.len() {
            let finished = {
                let s = &self.shapes[i];
                s.age >= s.life
            };
            if finished {
                let crystal = self.shapes[i].crystal;
                self.shapes[i] = self.new_shape(crystal);
            }
            let level = self.levels;
            let (swell, swell_energy) = (self.swell, self.swell_energy);
            let rng = &mut self.rng;
            let s = &mut self.shapes[i];
            // Glide faster when the music is loud.
            s.age += dt * (0.7 + 0.6 * energy);
            let u = (s.age / s.life).clamp(0.0, 1.0);
            let t = self.time + s.seed * 50.0;
            let wobble = [
                0.35 * (t * 0.31).sin(),
                0.25 * (t * 0.23 + 1.0).sin(),
                0.4 * (t * 0.17 + 2.0).sin(),
            ];
            s.pos = add(bezier(&s.path, u), wobble);
            s.fade = smooth(u / 0.08).min(smooth((1.0 - u) / 0.08));

            // Spin with the band and the music's energy; onsets swell the beams and sometimes
            // reverse the spin.
            let band_level = level[s.band];
            if let Some(strength) = onsets[s.band] {
                s.flash = strength.min(1.2);
                if rng.next_f32() < 0.35 {
                    s.spin_dir = -s.spin_dir;
                    self.reversals += 1;
                }
                // A nudge to the axis keeps the tumbling unpredictable.
                s.spin_axis = normalize(add(s.spin_axis, scale(random_unit(rng), 0.4)));
            }
            s.flash = (s.flash - dt * 0.8).max(0.0);
            let target = s.spin_dir * (0.15 + 1.6 * band_level + 0.8 * energy) / s.scale.sqrt();
            s.spin += (target - s.spin) * (dt * 2.0).min(1.0);
            s.rot = integrate(s.rot, scale(s.spin_axis, s.spin), dt);
            // Brightness eases towards where the music is heading, never jumping.
            let glow = 0.3 + 1.2 * swell[s.band] + 0.4 * swell_energy + 0.5 * s.flash;
            s.intensity += (glow - s.intensity) * (dt * 1.5).min(1.0);
        }
    }

    fn build_beams(&mut self) {
        self.beams.clear();
        // Nearest first: if the budget runs out, it's the beams deepest in the fog that go.
        let cam = self.camera.pos;
        let mut order: Vec<usize> = (0..self.shapes.len()).collect();
        order.sort_by(|&a, &b| {
            length(sub(self.shapes[a].pos, cam)).total_cmp(&length(sub(self.shapes[b].pos, cam)))
        });
        for s in order.iter().map(|&i| &self.shapes[i]) {
            if s.fade <= 0.0 {
                continue;
            }
            let faces = &self.faces[s.solid as usize];
            for (k, face) in faces.iter().enumerate() {
                if s.emitters & (1 << k) == 0 {
                    continue;
                }
                let dir = rotate_q(s.rot, face.normal);
                let spread = s.half_angles[k].tan();
                let breathing = 0.7 + 0.3 * (self.breath + k as f32 * 1.7 + s.seed * 10.0).sin();
                let intensity = s.intensity * breathing * s.fade * if s.laser { 2.5 } else { 1.0 };
                // Crystals shine from their heart; everything else from the whole face.
                let (origin, aperture) = if s.crystal {
                    (s.pos, 0.02)
                } else {
                    let origin = add(s.pos, scale(rotate_q(s.rot, face.center), s.scale));
                    let aperture = if s.laser {
                        0.02
                    } else {
                        face_radius(face) * s.scale
                    };
                    (origin, aperture)
                };
                if s.crystal {
                    // Dispersion: the light fans out into its spectrum.
                    let across = normalize(cross(dir, rotate_q(s.rot, [0.3, 1.0, 0.2])));
                    for (j, color) in SPECTRUM.iter().enumerate() {
                        if self.beams.len() == MAX_BEAMS {
                            return;
                        }
                        let bend = (j as f32 - 2.0) * 0.045;
                        self.beams.push(Beam {
                            origin,
                            dir: normalize(add(dir, scale(across, bend))),
                            color: *color,
                            spread,
                            aperture,
                            intensity: intensity * 0.8,
                            length: BEAM_LENGTH,
                        });
                    }
                } else {
                    if self.beams.len() == MAX_BEAMS {
                        return;
                    }
                    self.beams.push(Beam {
                        origin,
                        dir,
                        color: s.palette[k % 3],
                        spread,
                        aperture,
                        intensity,
                        length: BEAM_LENGTH,
                    });
                }
            }
        }
    }

    fn new_mote(&mut self) -> [f32; 3] {
        let rng = &mut self.rng;
        let z = -rng.uniform(1.5, 22.0);
        let reach = -z / FOCAL;
        [
            rng.uniform(-1.9, 1.9) * reach,
            rng.uniform(-1.1, 1.1) * reach,
            z,
        ]
    }

    fn drift_motes(&mut self, dt: f32) {
        let t = self.time;
        for i in 0..self.motes.len() {
            let m = &mut self.motes[i];
            let k = i as f32;
            m[0] += 0.03 * (t * 0.2 + k).sin() * dt;
            m[1] += (0.015 + 0.02 * (t * 0.13 + k * 1.7).cos()) * dt;
            m[2] += 0.02 * (t * 0.17 + k * 2.3).sin() * dt;
            let reach = -m[2] / FOCAL;
            if m[1] > 1.2 * reach {
                let mut fresh = self.new_mote();
                fresh[1] = -1.1 * (-fresh[2] / FOCAL);
                self.motes[i] = fresh;
            }
        }
    }
}

/// A slow S-curve from beyond one edge of the view (or out of the fog's depths) to beyond
/// another, wandering through the middle on the way.
fn new_path(rng: &mut Rng, size: f32) -> [[f32; 3]; 4] {
    let depth = rng.uniform(6.0, 18.0);
    let half_h = depth / FOCAL;
    let half_w = half_h * 1.8;
    let margin = size * 1.5 + 1.0;
    let outside = |rng: &mut Rng, side: usize| -> [f32; 3] {
        let z = -depth + rng.uniform(-3.0, 3.0);
        match side {
            0 => [-half_w - margin, rng.uniform(-0.7, 0.7) * half_h, z],
            1 => [half_w + margin, rng.uniform(-0.7, 0.7) * half_h, z],
            2 => [rng.uniform(-0.7, 0.7) * half_w, half_h + margin, z],
            3 => [rng.uniform(-0.7, 0.7) * half_w, -half_h - margin, z],
            // Out of (or into) the depths of the fog.
            _ => [
                rng.uniform(-0.5, 0.5) * half_w,
                rng.uniform(-0.5, 0.5) * half_h,
                -FOG_FAR - 4.0,
            ],
        }
    };
    let from = rng.pick(5);
    let to = loop {
        let side = rng.pick(5);
        if side != from {
            break side;
        }
    };
    let inside = |rng: &mut Rng| {
        [
            rng.uniform(-0.6, 0.6) * half_w,
            rng.uniform(-0.6, 0.6) * half_h,
            -depth + rng.uniform(-3.0, 3.0),
        ]
    };
    [
        outside(rng, from),
        inside(rng),
        inside(rng),
        outside(rng, to),
    ]
}

fn bezier(p: &[[f32; 3]; 4], t: f32) -> [f32; 3] {
    let u = 1.0 - t;
    let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
    (0..4).fold([0.0; 3], |acc, i| add(acc, scale(p[i], w[i])))
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

fn random_unit(rng: &mut Rng) -> [f32; 3] {
    let z = rng.uniform(-1.0, 1.0);
    let a = rng.uniform(0.0, TAU);
    let r = (1.0 - z * z).max(0.0).sqrt();
    [r * a.cos(), r * a.sin(), z]
}

fn normalize4(q: [f32; 4]) -> [f32; 4] {
    let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3])
        .sqrt()
        .max(1e-6);
    q.map(|c| c / len)
}

/// Turns orientation `q` by angular velocity `w` (radians per second) for `dt` seconds.
fn integrate(q: [f32; 4], w: [f32; 3], dt: f32) -> [f32; 4] {
    let angle = length(w) * dt;
    if angle < 1e-7 {
        return q;
    }
    let axis = normalize(w);
    let (s, c) = (angle * 0.5).sin_cos();
    let d = [axis[0] * s, axis[1] * s, axis[2] * s, c];
    normalize4(mul_q(d, q))
}

fn mul_q(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

/// Rotates `v` by unit quaternion `q`.
pub fn rotate_q(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let u = [q[0], q[1], q[2]];
    let t = scale(cross(u, v), 2.0);
    add(add(v, scale(t, q[3])), cross(u, t))
}

/// Columns of the rotation matrix for unit quaternion `q`.
pub fn matrix_q(q: [f32; 4]) -> [[f32; 3]; 3] {
    [
        rotate_q(q, [1.0, 0.0, 0.0]),
        rotate_q(q, [0.0, 1.0, 0.0]),
        rotate_q(q, [0.0, 0.0, 1.0]),
    ]
}

/// A beam's half-angle: lasers are needle-thin; crystals' fans are narrowish; everything else
/// ranges from a tight spot to a broad flood.
fn beam_angle(rng: &mut Rng, laser: bool, crystal: bool) -> f32 {
    if laser {
        return rng.uniform(0.006, 0.016);
    }
    let roll = rng.next_f32();
    let (lo, hi) = if crystal {
        if roll < 0.6 {
            (0.02, 0.06)
        } else {
            (0.08, 0.16)
        }
    } else if roll < 0.4 {
        (0.02, 0.07)
    } else if roll < 0.75 {
        (0.1, 0.22)
    } else {
        (0.3, 0.55)
    };
    rng.uniform(lo, hi)
}

/// Radius of the circle that fits inside a face (its inradius), for a solid of circumradius 1.
pub fn face_radius(face: &Face) -> f32 {
    let n = face.corners.len() as f32;
    length(sub(face.corners[0], face.center)) * (std::f32::consts::PI / n).cos()
}

/// Eases `value` towards `target`: up over about a second, down over about two.
fn follow_slowly(value: f32, target: f32, dt: f32) -> f32 {
    let rate = if target > value { 1.0 } else { 0.5 };
    value + (target - value) * (dt * rate).min(1.0)
}

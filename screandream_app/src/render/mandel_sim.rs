//! Flights into the Mandelbrot set and Julia sets, with lightning arcing between the peaks.
//! Kept free of GPU types so the rules can be unit-tested; `render::modes::mandelbrot` draws
//! the result.
//!
//! The fractal is laid out as a landscape: mountain ridges rise along the set's boundary
//! (height falls off with the distance estimate, measured in world units so the ridges keep
//! their size at any depth), the set's interior is a mesa topped with a still, mirror-like
//! pool just below the crests (so there are no cliffs at the shore), and the plains outside are
//! banded by escape time. Every band has a colour and a frequency: one end of the dive's
//! palette answers the bass, the other the treble, and each band is lifted by its part of the
//! spectrum. Each dive picks its own palette: a narrow slice of the colour wheel, from dark to
//! bright.
//!
//! A dive zooms towards a point on the boundary, found by repeatedly picking the slowest
//! escaping point of a shrinking view, and quickens with the music. When it runs out of
//! precision or time, it burns away and a new dive (into the Mandelbrot set or a Julia set)
//! takes its place. Lightning leaps from peak to peak, on onsets and more often when the
//! music is energetic, in the colour of the band that fired it; the bolts' points are pinned
//! to the fractal, so they stay on their peaks as the landscape swells around them.

use crate::analysis::audio_features::SPECTRUM_BINS;
use crate::render::sim_math::{Rng, add, cross, dot, length, normalize, scale, sub};
use std::f32::consts::PI;

pub const BANDS: usize = 4;
/// World units spanned by a dive's `span` in the fractal plane.
pub const WORLD_SPAN: f32 = 24.0;
/// The terrain grid covers x, z ∈ [-GRID_HALF, GRID_HALF] with this many cells a side.
pub const GRID_HALF: f32 = 48.0;
pub const GRID_CELLS: u32 = 360;
pub const PEAK_HEIGHT: f32 = 3.2;
/// How far from the boundary (in world units) the ridges fall away.
pub const RIDGE_WIDTH: f32 = 1.4;
/// The terrain averages four samples this far (in world units) around each point, which
/// rounds off spikes narrower than the grid.
pub const SMOOTHING: f32 = 0.45 * 2.0 * GRID_HALF / GRID_CELLS as f32;
/// The most a band's part of the spectrum lifts it.
pub const AUDIO_HEIGHT: f32 = 1.6;
/// Height of the set's interior.
pub const MESA_HEIGHT: f32 = 2.9;
/// Colour bands per escape iteration.
pub const BAND_FREQ: f32 = 0.045;
pub const BAILOUT2: f64 = 1e4;
pub const START_SPAN: f64 = 3.2;
/// Deeper than this, f32 on the GPU starts to show its grain.
pub const MIN_SPAN: f64 = 6e-4;
/// Longest a dive lasts before dissolving into the next.
pub const MAX_DIVE: f32 = 135.0;
pub const DISSOLVE: f32 = 5.0;
pub const FOCAL: f32 = 1.6;
pub const MAX_BOLTS: usize = 12;
pub const MAX_SEGMENTS: usize = 1536;
pub const MAX_LIGHTS: usize = 3;
/// Where each band sits on the palette (and in the spectrum).
pub const BAND_HUES: [f32; BANDS] = [0.04, 0.3, 0.6, 0.9];
/// Julia parameters with interesting sets.
pub const JULIAS: [[f64; 2]; 10] = [
    [-0.8, 0.156],
    [-0.7269, 0.1889],
    [0.285, 0.01],
    [-0.4, 0.6],
    [-0.835, -0.2321],
    [0.45, 0.1428],
    [-0.70176, -0.3842],
    [-0.123, 0.745],
    [-0.75, 0.11],
    [0.0, 0.8],
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Set {
    Mandelbrot,
    Julia([f64; 2]),
}

/// How a point escapes: smooth iteration count and distance estimate to the set.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Escape {
    pub nu: f32,
    pub de: f64,
}

/// Iterates z² + c; `None` if the point never escapes (is in the set).
pub fn escape(set: Set, p: [f64; 2], max_iter: u32) -> Option<Escape> {
    let (mut z, c, mut dz, one) = match set {
        Set::Mandelbrot => {
            if in_main_bulbs(p) {
                return None;
            }
            ([0.0, 0.0], p, [0.0, 0.0], 1.0)
        }
        Set::Julia(c) => (p, c, [1.0, 0.0], 0.0),
    };
    for n in 0..max_iter {
        dz = [
            2.0 * (z[0] * dz[0] - z[1] * dz[1]) + one,
            2.0 * (z[0] * dz[1] + z[1] * dz[0]),
        ];
        z = [z[0] * z[0] - z[1] * z[1] + c[0], 2.0 * z[0] * z[1] + c[1]];
        let r2 = z[0] * z[0] + z[1] * z[1];
        if r2 > BAILOUT2 {
            let r = r2.sqrt();
            let nu = n as f64 + 1.0 - r.log2().log2();
            let de = 0.5 * r * r.ln() / (dz[0] * dz[0] + dz[1] * dz[1]).sqrt().max(1e-300);
            return Some(Escape { nu: nu as f32, de });
        }
    }
    None
}

/// The main cardioid and period-2 bulb, which never escape.
pub fn in_main_bulbs(c: [f64; 2]) -> bool {
    let x = c[0] - 0.25;
    let q = x * x + c[1] * c[1];
    q * (q + x) <= 0.25 * c[1] * c[1] || (c[0] + 1.0).powi(2) + c[1] * c[1] <= 0.0625
}

/// Iteration budget for a view `span` across: deeper views need more.
pub fn iterations(span: f64) -> u32 {
    (120.0 + 40.0 * (START_SPAN / span).max(1.0).log2()).min(600.0) as u32
}

/// Position of an escape time on the palette, 0 (bass) to 1 (treble) and back.
pub fn band_of(nu: f32, phase: f32) -> f32 {
    let x = (nu * BAND_FREQ + phase).rem_euclid(1.0);
    1.0 - (2.0 * x - 1.0).abs()
}

/// A dive's colours: a slice of the colour wheel, dark at the bass end and bright at the
/// treble end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub hue: f32,
    /// How much of the wheel the palette spans; negative runs the other way round.
    pub range: f32,
    pub saturation: f32,
}

/// Widest slice of the colour wheel a palette spans.
pub const MAX_PALETTE_RANGE: f32 = 0.3;

impl Palette {
    pub fn random(rng: &mut Rng) -> Self {
        let range = rng.uniform(0.08, MAX_PALETTE_RANGE);
        Self {
            hue: rng.next_f32(),
            range: if rng.next_f32() < 0.5 { range } else { -range },
            saturation: rng.uniform(0.6, 1.0),
        }
    }

    /// The colour at `t` ∈ [0, 1]; matches `band_colour` in mandelbrot.wgsl.
    pub fn colour(&self, t: f32) -> [f32; 3] {
        let h = (self.hue + self.range * t).rem_euclid(1.0);
        let value = 0.55 + 0.45 * t;
        [0.0, 2.0 / 3.0, 1.0 / 3.0].map(|o: f32| {
            let pure = ((((h + o).fract()) * 6.0 - 3.0).abs() - 1.0).clamp(0.0, 1.0);
            (1.0 + (pure - 1.0) * self.saturation) * value
        })
    }
}

/// The spectrum at `t` ∈ [0, 1], linearly interpolated between bins.
pub fn spectrum_at(spectrum: &[f32; SPECTRUM_BINS], t: f32) -> f32 {
    let x = t.clamp(0.0, 1.0) * (SPECTRUM_BINS - 1) as f32;
    let i = x.floor() as usize;
    let j = (i + 1).min(SPECTRUM_BINS - 1);
    spectrum[i] + (spectrum[j] - spectrum[i]) * x.fract()
}

/// 1 on the boundary, falling to 0 a few `RIDGE_WIDTH`s away.
pub fn ridge(e: Escape, span: f64) -> f32 {
    (-(e.de / span) as f32 * WORLD_SPAN / RIDGE_WIDTH).exp()
}

/// Landscape height for an escape result; matches `height` in mandelbrot.wgsl.
pub fn height(e: Option<Escape>, span: f64, phase: f32, spectrum: &[f32; SPECTRUM_BINS]) -> f32 {
    match e {
        None => MESA_HEIGHT,
        Some(e) => {
            let r = ridge(e, span);
            let level = spectrum_at(spectrum, band_of(e.nu, phase));
            PEAK_HEIGHT * r + AUDIO_HEIGHT * level * (0.25 + 0.75 * r)
        }
    }
}

/// One flight into a set.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dive {
    pub set: Set,
    /// Zoom target, at world (0, 0).
    pub centre: [f64; 2],
    /// Width of the fractal plane across `WORLD_SPAN` world units; shrinks as the dive goes.
    pub span: f64,
    /// Rotation of the plane under the world.
    pub rot: f32,
    pub spin: f32,
    /// Offset of the bands on the palette; drifts so the bands flow.
    pub phase: f32,
    pub palette: Palette,
    pub age: f32,
    /// Distinguishes dives, so bolts know which landscape they belong to.
    pub id: u64,
    /// A Julia parameter's drift with the music, as a fraction of the span.
    pub wobble: [f32; 2],
}

impl Dive {
    /// The set as currently drawn, with a Julia parameter's drift applied.
    pub fn set_now(&self) -> Set {
        match self.set {
            Set::Mandelbrot => Set::Mandelbrot,
            Set::Julia(c) => Set::Julia([
                c[0] + self.wobble[0] as f64 * self.span,
                c[1] + self.wobble[1] as f64 * self.span,
            ]),
        }
    }

    pub fn iterations(&self) -> u32 {
        iterations(self.span)
    }

    pub fn to_plane(&self, w: [f32; 2]) -> [f64; 2] {
        let (s, c) = (self.rot as f64).sin_cos();
        let k = self.span / WORLD_SPAN as f64;
        let (x, y) = (w[0] as f64, w[1] as f64);
        [
            self.centre[0] + (c * x - s * y) * k,
            self.centre[1] + (s * x + c * y) * k,
        ]
    }

    pub fn to_world(&self, p: [f64; 2]) -> [f32; 2] {
        let (s, c) = (self.rot as f64).sin_cos();
        let k = WORLD_SPAN as f64 / self.span;
        let (x, y) = (p[0] - self.centre[0], p[1] - self.centre[1]);
        [((c * x + s * y) * k) as f32, ((c * y - s * x) * k) as f32]
    }

    /// Terrain height at world `w`, smoothed as the mesh is; matches `vs_terrain` in
    /// mandelbrot.wgsl.
    pub fn height_at(&self, w: [f32; 2], spectrum: &[f32; SPECTRUM_BINS]) -> f32 {
        let (set, iters) = (self.set_now(), self.iterations());
        [[1.0, 1.0], [-1.0, 1.0], [1.0, -1.0], [-1.0, -1.0]]
            .iter()
            .map(|o: &[f32; 2]| {
                let p = self.to_plane([w[0] + o[0] * SMOOTHING, w[1] + o[1] * SMOOTHING]);
                height(escape(set, p, iters), self.span, self.phase, spectrum)
            })
            .sum::<f32>()
            / 4.0
    }
}

/// Picks a zoom target on the set's boundary: repeatedly narrows the view onto one of its
/// slowest escaping points.
pub fn find_target(rng: &mut Rng, set: Set) -> [f64; 2] {
    const N: usize = 12;
    let mut p = match set {
        Set::Mandelbrot => [-0.6, 0.0],
        Set::Julia(_) => [0.0, 0.0],
    };
    let mut span = 3.0;
    while span > MIN_SPAN * 0.3 {
        let iters = iterations(span);
        let mut found: Vec<(f32, [f64; 2])> = Vec::with_capacity(N * N);
        for i in 0..N {
            for j in 0..N {
                let jx = rng.uniform(-0.4, 0.4) as f64;
                let jy = rng.uniform(-0.4, 0.4) as f64;
                let q = [
                    p[0] + ((i as f64 + 0.5 + jx) / N as f64 - 0.5) * span * 0.6,
                    p[1] + ((j as f64 + 0.5 + jy) / N as f64 - 0.5) * span * 0.6,
                ];
                if let Some(e) = escape(set, q, iters) {
                    found.push((e.nu, q));
                }
            }
        }
        if found.is_empty() {
            break;
        }
        found.sort_by(|a, b| b.0.total_cmp(&a.0));
        p = found[rng.pick(found.len().min(4))].1;
        span *= 0.4;
    }
    p
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drive {
    pub bands: [f32; BANDS],
    pub energy: f32,
    pub spectrum: [f32; SPECTRUM_BINS],
}

impl Default for Drive {
    fn default() -> Self {
        Self {
            bands: [0.0; BANDS],
            energy: 0.0,
            spectrum: [0.0; SPECTRUM_BINS],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub pos: [f32; 3],
    pub forward: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
}

/// One stretch of a bolt, drawn as a glowing capsule; `a == b` makes a glowing orb.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub width: f32,
    pub intensity: f32,
    pub colour: [f32; 3],
}

/// A point light thrown by a bolt onto the landscape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light {
    pub pos: [f32; 3],
    pub colour: [f32; 3],
    pub intensity: f32,
}

/// A point of a bolt, pinned to the fractal plane so it rides the zoom.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Pin {
    plane: [f64; 2],
    y: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Bolt {
    dive: u64,
    /// The main channel, then the forks; each a polyline.
    strokes: Vec<Vec<Pin>>,
    pub colour: [f32; 3],
    pub age: f32,
    pub life: f32,
    pub strength: f32,
    seed: u32,
}

impl Bolt {
    /// Brightness now: a bright first stroke, then a few flickering re-strikes, fading out.
    pub fn intensity(&self) -> f32 {
        let fade = (1.0 - self.age / self.life).max(0.0).powf(0.6);
        let burst = if self.age < 0.06 { 1.6 } else { 1.0 };
        let tick = (self.age * 22.0) as u32;
        let flicker = if hash(self.seed ^ tick.wrapping_mul(2654435761)) > 0.35 {
            1.0
        } else {
            0.3
        };
        self.strength * fade * burst * flicker
    }
}

pub struct MandelSim {
    rng: Rng,
    time: f32,
    current: Dive,
    next: Option<Dive>,
    dissolve: f32,
    next_id: u64,
    bolts: Vec<Bolt>,
    segments: Vec<Segment>,
    lights: Vec<Light>,
    /// Peak tops of the leading dive in world space, refreshed every so often.
    peaks: Vec<[f32; 3]>,
    peak_timer: f32,
    bolt_timer: f32,
    camera: Camera,
    yaw: f32,
    shake: f32,
    flash: f32,
    /// Bass shockwave rings: radius and strength.
    ring: [f32; 2],
    swell: [f32; BANDS],
    swell_energy: f32,
    last_level: [f32; BANDS],
    kick: [f32; BANDS],
    cooldown: [f32; BANDS],
    strikes: u32,
}

impl MandelSim {
    pub fn new(seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let yaw = rng.uniform(0.0, std::f32::consts::TAU);
        let mut sim = Self {
            current: placeholder_dive(),
            rng,
            time: 0.0,
            next: None,
            dissolve: 0.0,
            next_id: 0,
            bolts: Vec::new(),
            segments: Vec::new(),
            lights: Vec::new(),
            peaks: Vec::new(),
            peak_timer: 0.0,
            bolt_timer: 1.0,
            camera: Camera {
                pos: [0.0, 6.0, -15.0],
                forward: [0.0, 0.0, 1.0],
                right: [1.0, 0.0, 0.0],
                up: [0.0, 1.0, 0.0],
            },
            yaw,
            shake: 0.0,
            flash: 0.0,
            ring: [0.0; 2],
            swell: [0.0; BANDS],
            swell_energy: 0.0,
            last_level: [0.0; BANDS],
            kick: [0.0; BANDS],
            cooldown: [0.0; BANDS],
            strikes: 0,
        };
        sim.current = sim.new_dive();
        sim.move_camera();
        sim
    }

    /// The dive on show, and the one fading in over it (with its progress, 0 to 1).
    pub fn dives(&self) -> (&Dive, Option<(&Dive, f32)>) {
        (
            &self.current,
            self.next.as_ref().map(|n| (n, self.dissolve)),
        )
    }

    pub fn camera(&self) -> Camera {
        self.camera
    }

    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    pub fn lights(&self) -> &[Light] {
        &self.lights
    }

    pub fn bolts(&self) -> &[Bolt] {
        &self.bolts
    }

    pub fn peaks(&self) -> &[[f32; 3]] {
        &self.peaks
    }

    /// Sky brightness from lightning, 0 to about 1.
    pub fn flash(&self) -> f32 {
        self.flash
    }

    /// Radius and strength of the shockwave ring the last bass onset sent out.
    pub fn ring(&self) -> [f32; 2] {
        self.ring
    }

    /// Bolts struck so far.
    pub fn strikes(&self) -> u32 {
        self.strikes
    }

    pub fn step(&mut self, dt: f32, drive: &Drive) {
        self.time += dt;
        let energy = drive.energy.clamp(0.0, 1.0);
        for (swell, band) in self.swell.iter_mut().zip(drive.bands) {
            *swell += (band.clamp(0.0, 1.0) - *swell) * (dt * 1.5).min(1.0);
        }
        self.swell_energy += (energy - self.swell_energy) * (dt * 1.0).min(1.0);
        let onsets = self.detect_onsets(dt, &drive.bands);

        self.advance_dives(dt);

        self.ring[0] += dt * 22.0;
        self.ring[1] = (self.ring[1] - dt * 0.9).max(0.0);
        if let Some(strength) = onsets[0] {
            self.ring = [0.0, strength.min(1.0)];
        }
        self.shake = (self.shake - dt * 2.5).max(0.0);
        self.flash = (self.flash - dt * 6.0).max(0.0);
        self.move_camera();

        self.peak_timer -= dt;
        if self.peak_timer <= 0.0 {
            self.peak_timer = 0.12;
            self.peaks = self.find_peaks(&drive.spectrum);
        }
        self.update_bolts(dt, energy, &onsets);
        self.build_segments();
    }

    fn detect_onsets(&mut self, dt: f32, levels: &[f32; BANDS]) -> [Option<f32>; BANDS] {
        const COOLDOWN: [f32; BANDS] = [0.3, 0.25, 0.2, 0.18];
        std::array::from_fn(|b| {
            let level = levels[b].clamp(0.0, 1.0);
            let rise = (level - self.last_level[b]).max(0.0);
            self.last_level[b] = level;
            let last_kick = self.kick[b];
            self.kick[b] = (self.kick[b] * (1.0 - dt * 4.0).max(0.0) + rise * 3.0).min(1.5);
            self.cooldown[b] -= dt;
            let hit =
                self.kick[b] - last_kick > 0.08 && self.kick[b] > 0.25 && self.cooldown[b] <= 0.0;
            hit.then(|| {
                self.cooldown[b] = COOLDOWN[b];
                self.kick[b].min(1.5)
            })
        })
    }

    fn advance_dives(&mut self, dt: f32) {
        // Zoom quicker with the music; a kick lunges forward.
        let doublings = 0.073 + 0.15 * self.swell_energy + 0.17 * self.kick[0].min(1.0);
        let shrink = (-(std::f64::consts::LN_2 * doublings as f64 * dt as f64)).exp();
        let time = self.time;
        let wobble_amp = 0.03 * (0.3 + self.swell[1]);
        let phase_rate = 0.01 + 0.05 * self.swell[1] + 0.03 * self.swell[3];
        for dive in std::iter::once(&mut self.current).chain(self.next.as_mut()) {
            dive.age += dt;
            dive.span = (dive.span * shrink).max(MIN_SPAN * 0.5);
            dive.rot += dive.spin * dt;
            dive.phase = (dive.phase + dt * phase_rate).rem_euclid(1.0);
            let phase = time * 0.4 + dive.id as f32;
            dive.wobble = [wobble_amp * phase.cos(), wobble_amp * (phase * 1.3).sin()];
        }

        let spent = self.current.span <= MIN_SPAN || self.current.age >= MAX_DIVE;
        if self.next.is_none() && spent {
            self.next = Some(self.new_dive());
            self.dissolve = 0.0;
        }
        if self.next.is_some() {
            self.dissolve += dt / DISSOLVE;
            if self.dissolve >= 1.0 {
                self.current = self.next.take().expect("checked above");
                self.dissolve = 0.0;
            }
        }
    }

    fn new_dive(&mut self) -> Dive {
        let rng = &mut self.rng;
        let set = if rng.next_f32() < 0.55 {
            Set::Mandelbrot
        } else {
            Set::Julia(JULIAS[rng.pick(JULIAS.len())])
        };
        let centre = find_target(rng, set);
        let id = self.next_id;
        self.next_id += 1;
        Dive {
            set,
            centre,
            span: START_SPAN,
            rot: rng.uniform(0.0, std::f32::consts::TAU),
            spin: rng.uniform(-0.04, 0.04),
            phase: rng.next_f32(),
            palette: Palette::random(rng),
            age: 0.0,
            id,
            wobble: [0.0; 2],
        }
    }

    /// The dive that most of the screen shows.
    fn leading(&self) -> &Dive {
        match &self.next {
            Some(next) if self.dissolve > 0.5 => next,
            _ => &self.current,
        }
    }

    fn dive(&self, id: u64) -> Option<&Dive> {
        std::iter::once(&self.current)
            .chain(self.next.as_ref())
            .find(|d| d.id == id)
    }

    /// Hovers over the zoom target, circling slowly and looking down into it.
    fn move_camera(&mut self) {
        let t = self.time;
        let yaw = self.yaw + t * 0.035 + 0.6 * (t * 0.011).sin();
        let dist = 15.0 + 2.0 * (t * 0.05).sin();
        let height = 6.5 + 1.5 * (t * 0.07 + 1.0).sin();
        let mut pos = [yaw.sin() * dist, height, -yaw.cos() * dist];
        if self.shake > 0.0 {
            let s = self.shake * 0.12;
            pos = add(
                pos,
                [
                    s * (t * 61.0).sin(),
                    s * (t * 47.0 + 1.0).sin(),
                    s * (t * 53.0 + 2.0).sin(),
                ],
            );
        }
        let target = [0.6 * (t * 0.07).sin(), 0.8, 0.6 * (t * 0.05).cos()];
        let forward = normalize(sub(target, pos));
        let roll = 0.05 * (t * 0.09).sin();
        let flat_right = normalize(cross([0.0, 1.0, 0.0], forward));
        let flat_up = cross(forward, flat_right);
        let (sin, cos) = roll.sin_cos();
        self.camera = Camera {
            pos,
            forward,
            right: add(scale(flat_right, cos), scale(flat_up, sin)),
            up: sub(scale(flat_up, cos), scale(flat_right, sin)),
        };
    }

    /// Local maxima of the leading dive's landscape that the camera can see.
    fn find_peaks(&self, spectrum: &[f32; SPECTRUM_BINS]) -> Vec<[f32; 3]> {
        const N: usize = 30;
        const HALF: f32 = 21.0;
        let dive = self.leading();
        let at = |i: usize, j: usize| {
            [
                (i as f32 / (N - 1) as f32 * 2.0 - 1.0) * HALF,
                (j as f32 / (N - 1) as f32 * 2.0 - 1.0) * HALF,
            ]
        };
        // The mesas' flat tops are not peaks.
        let set = dive.set_now();
        let iters = dive.iterations();
        let heights: Vec<f32> = (0..N * N)
            .map(
                |k| match escape(set, dive.to_plane(at(k / N, k % N)), iters) {
                    None => f32::NEG_INFINITY,
                    Some(_) => dive.height_at(at(k / N, k % N), spectrum),
                },
            )
            .collect();
        let cam = self.camera;
        let mut peaks = Vec::new();
        for i in 1..N - 1 {
            for j in 1..N - 1 {
                let h = heights[i * N + j];
                if h < 0.5 * PEAK_HEIGHT {
                    continue;
                }
                let highest = (i - 1..=i + 1)
                    .flat_map(|a| (j - 1..=j + 1).map(move |b| (a, b)))
                    .all(|(a, b)| heights[a * N + b] <= h);
                let w = at(i, j);
                let p = [w[0], h, w[1]];
                let rel = sub(p, cam.pos);
                if highest && dot(rel, cam.forward) > 3.0 && length(rel) < 40.0 {
                    peaks.push(p);
                }
            }
        }
        peaks
    }

    fn update_bolts(&mut self, dt: f32, energy: f32, onsets: &[Option<f32>; BANDS]) {
        for bolt in &mut self.bolts {
            bolt.age += dt;
        }
        let live: Vec<u64> = std::iter::once(self.current.id)
            .chain(self.next.as_ref().map(|d| d.id))
            .collect();
        self.bolts
            .retain(|b| b.age < b.life && live.contains(&b.dive));

        // Onsets fire bolts in their band's colour; energetic music keeps them coming.
        for (band, onset) in onsets.iter().enumerate() {
            if let Some(strength) = onset {
                self.strike(band, 0.6 + 0.5 * strength.min(1.0));
            }
        }
        self.bolt_timer -= dt;
        if self.bolt_timer <= 0.0 {
            let rate = 0.25 + 2.5 * energy * energy;
            self.bolt_timer = self.rng.uniform(0.5, 1.5) / rate;
            let band = self.rng.pick(BANDS);
            self.strike(band, 0.5 + 0.5 * energy);
        }
    }

    /// Arcs a bolt from one peak to another nearby.
    fn strike(&mut self, band: usize, strength: f32) {
        if self.bolts.len() >= MAX_BOLTS || self.peaks.len() < 2 {
            return;
        }
        let dive = *self.leading();
        let rng = &mut self.rng;
        let a = self.peaks[rng.pick(self.peaks.len())];
        let near: Vec<[f32; 3]> = self
            .peaks
            .iter()
            .copied()
            .filter(|p| (2.5..12.0).contains(&length(sub(*p, a))))
            .collect();
        if near.is_empty() {
            return;
        }
        let b = near[rng.pick(near.len())];
        let lift = [0.0, 0.12, 0.0];
        let main = bolt_path(rng, add(a, lift), add(b, lift));

        let mut strokes = vec![main.clone()];
        for _ in 0..2 + rng.pick(3) {
            let k = (main.len() as f32 * rng.uniform(0.2, 0.8)) as usize;
            let along = normalize(sub(main[k + 1], main[k]));
            let len = length(sub(b, a)) * rng.uniform(0.15, 0.35);
            let dir = normalize(add(
                add(along, scale(rng.unit_vector(), 0.9)),
                [0.0, -0.6, 0.0],
            ));
            strokes.push(jagged(
                rng,
                main[k],
                add(main[k], scale(dir, len)),
                len * 0.15,
                4,
            ));
        }

        let pinned = strokes
            .into_iter()
            .map(|s| {
                s.into_iter()
                    .map(|p| Pin {
                        plane: dive.to_plane([p[0], p[2]]),
                        y: p[1],
                    })
                    .collect()
            })
            .collect();
        let colour = dive.palette.colour(BAND_HUES[band]);
        self.bolts.push(Bolt {
            dive: dive.id,
            strokes: pinned,
            colour: [0, 1, 2].map(|i| 0.4 + 0.6 * colour[i]),
            age: 0.0,
            life: rng.uniform(0.45, 0.9),
            strength,
            seed: (rng.next_f32() * u32::MAX as f32) as u32,
        });
        self.strikes += 1;
        self.flash = (self.flash + 0.35 * strength).min(1.0);
        self.shake = (self.shake + 0.4 * strength).min(1.0);
    }

    fn build_segments(&mut self) {
        self.segments.clear();
        self.lights.clear();
        let mut lights: Vec<(f32, Light)> = Vec::new();
        for bolt in &self.bolts {
            let Some(dive) = self.dive(bolt.dive).copied() else {
                continue;
            };
            let intensity = bolt.intensity();
            let world = |p: &Pin| {
                let w = dive.to_world(p.plane);
                [w[0], p.y, w[1]]
            };
            for (k, stroke) in bolt.strokes.iter().enumerate() {
                let (width, glow) = if k == 0 { (0.05, 1.0) } else { (0.03, 0.55) };
                for pair in stroke.windows(2) {
                    if self.segments.len() >= MAX_SEGMENTS {
                        break;
                    }
                    self.segments.push(Segment {
                        a: world(&pair[0]),
                        b: world(&pair[1]),
                        width,
                        intensity: intensity * glow,
                        colour: bolt.colour,
                    });
                }
            }
            // Orbs where the bolt touches the peaks.
            let main = &bolt.strokes[0];
            for end in [&main[0], &main[main.len() - 1]] {
                if self.segments.len() < MAX_SEGMENTS {
                    let p = world(end);
                    self.segments.push(Segment {
                        a: p,
                        b: p,
                        width: 0.14,
                        intensity: intensity * 0.8,
                        colour: bolt.colour,
                    });
                }
            }
            let mid = world(&main[main.len() / 2]);
            lights.push((
                intensity,
                Light {
                    pos: mid,
                    colour: bolt.colour,
                    intensity,
                },
            ));
        }
        lights.sort_by(|a, b| b.0.total_cmp(&a.0));
        self.lights
            .extend(lights.into_iter().take(MAX_LIGHTS).map(|(_, l)| l));
    }
}

/// An arching, jagged channel from `a` to `b`.
fn bolt_path(rng: &mut Rng, a: [f32; 3], b: [f32; 3]) -> Vec<[f32; 3]> {
    let d = length(sub(b, a));
    let arc = rng.uniform(0.15, 0.4) * d;
    let on_arc = |t: f32| {
        add(
            add(a, scale(sub(b, a), t)),
            [0.0, arc * (PI * t).sin(), 0.0],
        )
    };
    let mut points = vec![a];
    for k in 0..4 {
        let (p, q) = (on_arc(k as f32 / 4.0), on_arc((k + 1) as f32 / 4.0));
        let piece = jagged(rng, p, q, d * 0.06, 3);
        points.extend_from_slice(&piece[1..]);
    }
    points
}

/// Midpoint displacement: `2^depth` segments from `a` to `b`, each midpoint pushed aside.
fn jagged(rng: &mut Rng, a: [f32; 3], b: [f32; 3], amp: f32, depth: u32) -> Vec<[f32; 3]> {
    let mut points = vec![a, b];
    let mut amp = amp;
    for _ in 0..depth {
        let mut finer = Vec::with_capacity(points.len() * 2);
        for pair in points.windows(2) {
            let mid = scale(add(pair[0], pair[1]), 0.5);
            finer.push(pair[0]);
            finer.push(add(mid, scale(rng.unit_vector(), amp)));
        }
        finer.push(*points.last().expect("never empty"));
        points = finer;
        amp *= 0.5;
    }
    points
}

fn hash(x: u32) -> f32 {
    let mut h = x.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

fn placeholder_dive() -> Dive {
    Dive {
        set: Set::Mandelbrot,
        centre: [-0.6, 0.0],
        span: START_SPAN,
        rot: 0.0,
        spin: 0.0,
        phase: 0.0,
        palette: Palette {
            hue: 0.0,
            range: 0.2,
            saturation: 1.0,
        },
        age: 0.0,
        id: u64::MAX,
        wobble: [0.0; 2],
    }
}

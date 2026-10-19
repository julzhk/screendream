//! Rising, splitting, jostling bubbles. Kept free of GPU types so the rules can be
//! unit-tested; `render::modes::bubbles` draws the result.
//!
//! Every bubble belongs to one of [`BANDS`] frequency bands: low bands make big bubbles,
//! high bands small ones, and each bubble swells and wobbles with its band's level. Bubbles
//! split often — each half moves up one band, so splitting roughly conserves volume — and
//! rise gently until they pop at the surface. Each beat sets the water sloshing back and
//! forth like a swell, bending the bubbles' paths, and louder music releases more of them.

pub const BANDS: usize = 8;
pub const MAX_BUBBLES: usize = 160;
pub const MAX_POPS: usize = 24;
/// Height of the water surface in scene units (the scene spans y ∈ [-1, 1]).
pub const SURFACE_Y: f32 = 0.93;
pub const SEABED_Y: f32 = -0.8;
/// How long a pop's ripple lasts, in seconds.
pub const POP_LIFETIME: f32 = 1.4;

/// The beat-driven swell: natural period of the sloshing, and how quickly it dies away.
const SURGE_OMEGA: f32 = std::f32::consts::TAU / 1.6;
const SURGE_DAMPING: f32 = 0.3;
/// Beat strength → peak current speed (scene units per second).
const SURGE_KICK: f32 = 0.16;
/// How the swell's phase shifts with height, so it rolls up through the water.
const SURGE_WAVENUMBER: f32 = 2.2;

/// Resting radius of a bubble in `band` at `scale`, swollen by its band `level` (0..1).
pub fn target_radius(band: usize, scale: f32, level: f32) -> f32 {
    0.11 * 0.8f32.powi(band as i32) * scale * (0.65 + 0.7 * level.clamp(0.0, 1.0))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bubble {
    pub pos: [f32; 2],
    pub vel: [f32; 2],
    pub band: usize,
    pub scale: f32,
    pub radius: f32,
    /// How far the outline deviates from a circle.
    pub wobble: f32,
    /// Phase of the wobble, advancing faster for higher bands and louder music.
    pub phase: f32,
    /// Offset into the thin-film colour cycle, so neighbours shimmer differently.
    pub film: f32,
    pub level: f32,
    pub seed: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pop {
    pub pos: [f32; 2],
    pub age: f32,
    pub size: f32,
}

pub struct BubbleSim {
    bubbles: Vec<Bubble>,
    pops: Vec<Pop>,
    rng: u64,
    time: f32,
    big_debt: f32,
    small_debt: f32,
    /// Beat-driven horizontal current (a damped oscillator) and its rate of change.
    surge: f32,
    surge_rate: f32,
    /// Accumulated water displacement, for things that drift with the current.
    drift: f32,
    last_kick: f32,
    beat_cooldown: f32,
    beat_sign: f32,
}

/// Seabed vents that bubbles rise from, as a fraction of the half-width.
const VENTS: [f32; 3] = [-0.62, 0.08, 0.7];

impl BubbleSim {
    pub fn new(seed: u64) -> Self {
        Self {
            bubbles: Vec::with_capacity(MAX_BUBBLES),
            pops: Vec::with_capacity(MAX_POPS),
            rng: seed | 1,
            time: 0.0,
            big_debt: 0.0,
            small_debt: 0.0,
            surge: 0.0,
            surge_rate: 0.0,
            drift: 0.0,
            last_kick: 0.0,
            beat_cooldown: 0.0,
            beat_sign: 1.0,
        }
    }

    pub fn bubbles(&self) -> &[Bubble] {
        &self.bubbles
    }

    pub fn pops(&self) -> &[Pop] {
        &self.pops
    }

    /// Horizontal water current at mid-depth: a slow drift plus the beat-driven swell.
    pub fn current(&self) -> f32 {
        0.025 * (self.time * 0.05).sin() + self.surge
    }

    /// How far the water has moved sideways in total, for scenery that drifts with it.
    pub fn drift(&self) -> f32 {
        self.drift
    }

    /// The current at height `y`. The swell arrives at different depths at different times,
    /// so a rising bubble is pushed one way, then the other.
    fn current_at(&self, y: f32) -> [f32; 2] {
        let (sin, cos) = (y * SURGE_WAVENUMBER).sin_cos();
        let quadrature = self.surge_rate / SURGE_OMEGA;
        let slow = 0.025 * (self.time * 0.05).sin();
        [
            slow + self.surge * cos + quadrature * sin,
            0.3 * (quadrature * cos - self.surge * sin),
        ]
    }

    /// Beats shove the water one way, then the next beat the other, like a swell.
    fn slosh(&mut self, dt: f32, kick: f32) {
        self.beat_cooldown -= dt;
        if kick - self.last_kick > 0.1 && kick > 0.3 && self.beat_cooldown <= 0.0 {
            self.beat_cooldown = 0.25;
            self.beat_sign = -self.beat_sign;
            self.surge_rate += self.beat_sign * SURGE_OMEGA * SURGE_KICK * kick.min(1.0);
        }
        self.last_kick = kick;
        let accel = -SURGE_OMEGA * SURGE_OMEGA * self.surge
            - 2.0 * SURGE_DAMPING * SURGE_OMEGA * self.surge_rate;
        self.surge_rate += accel * dt;
        self.surge += self.surge_rate * dt;
        self.drift += self.current() * dt;
    }

    /// Advances the simulation. `levels[b]` is band `b`'s level (0..1), `aspect` the scene's
    /// half-width, and `kick` a decaying bass-hit impulse that shakes bubbles apart.
    pub fn step(&mut self, dt: f32, aspect: f32, levels: &[f32], amplitude: f32, kick: f32) {
        self.time += dt;
        let level = |band: usize| levels.get(band).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let bass = level(0).max(level(1));
        let treble = level(BANDS - 2).max(level(BANDS - 1));
        let intensity = (0..BANDS).map(level).sum::<f32>() / BANDS as f32;
        self.slosh(dt, kick);

        // Big bubbles burp from the vents with the bass; streams of tiny ones with the treble.
        // Overall intensity multiplies both, and loud passages bubble up all along the bed.
        let surge = intensity * intensity;
        self.big_debt += dt * (0.3 + 2.0 * bass + 5.0 * surge + amplitude);
        self.small_debt += dt * (0.4 + 6.0 * treble + 16.0 * surge);
        while self.big_debt >= 1.0 {
            self.big_debt -= 1.0;
            let band = self.pick(3);
            self.spawn(band, aspect, intensity);
        }
        while self.small_debt >= 1.0 {
            self.small_debt -= 1.0;
            let band = BANDS - 3 + self.pick(3);
            self.spawn(band, aspect, intensity);
        }

        self.split_some(dt, &level, kick);
        self.update_motion(dt, aspect, &level);
        self.collide();

        // Pop at the surface; let old ripples go.
        let mut popped = Vec::new();
        self.bubbles.retain(|b| {
            let surfaced = b.pos[1] + b.radius * 0.5 > SURFACE_Y;
            if surfaced {
                popped.push(Pop {
                    pos: [b.pos[0], SURFACE_Y],
                    age: 0.0,
                    size: b.radius,
                });
            }
            !surfaced && b.pos[0].abs() < aspect + 0.3
        });
        for pop in &mut self.pops {
            pop.age += dt;
        }
        self.pops.retain(|pop| pop.age < POP_LIFETIME);
        for pop in popped {
            if self.pops.len() == MAX_POPS {
                self.pops.remove(0);
            }
            self.pops.push(pop);
        }
    }

    fn spawn(&mut self, band: usize, aspect: f32, intensity: f32) {
        if self.bubbles.len() >= MAX_BUBBLES {
            return;
        }
        let vent = if self.next_f32() < 0.6 * intensity {
            self.uniform(-0.9, 0.9) * aspect
        } else {
            VENTS[self.pick(VENTS.len())] * aspect
        };
        let bubble = Bubble {
            pos: [vent + self.uniform(-0.04, 0.04), SEABED_Y],
            vel: [self.uniform(-0.02, 0.02), 0.02],
            band,
            scale: self.uniform(0.85, 1.15),
            radius: 0.0,
            wobble: 0.3,
            phase: self.uniform(0.0, std::f32::consts::TAU),
            film: self.uniform(0.0, 1.0),
            level: 0.0,
            seed: self.uniform(0.0, 1.0),
        };
        self.bubbles.push(bubble);
    }

    /// Splits bubbles at random, more often when their band is loud or the bass hits.
    fn split_some(&mut self, dt: f32, level: &impl Fn(usize) -> f32, kick: f32) {
        let mut born = Vec::new();
        for i in 0..self.bubbles.len() {
            let b = self.bubbles[i];
            let grown = b.radius > 0.8 * target_radius(b.band, b.scale, b.level);
            if b.band + 1 >= BANDS || !grown {
                continue;
            }
            let rate =
                0.12 + 0.9 * level(b.band) + 2.0 * kick * (1.0 - b.band as f32 / BANDS as f32);
            if self.next_f32() >= rate * dt || self.bubbles.len() + born.len() >= MAX_BUBBLES {
                continue;
            }
            let angle = self.uniform(0.0, std::f32::consts::TAU);
            let axis = [angle.cos(), angle.sin()];
            let child_radius = b.radius * 0.79;
            let make = |sign: f32, film: f32| Bubble {
                pos: [
                    b.pos[0] + sign * axis[0] * child_radius * 0.6,
                    b.pos[1] + sign * axis[1] * child_radius * 0.6,
                ],
                vel: [
                    b.vel[0] + sign * axis[0] * 0.07,
                    b.vel[1] + sign * axis[1] * 0.07,
                ],
                band: b.band + 1,
                radius: child_radius,
                wobble: b.wobble + 0.45,
                film,
                ..b
            };
            self.bubbles[i] = make(1.0, b.film);
            born.push(make(-1.0, (b.film + 0.37).fract()));
        }
        self.bubbles.extend(born);
    }

    fn update_motion(&mut self, dt: f32, aspect: f32, level: &impl Fn(usize) -> f32) {
        let t = self.time;
        let currents: Vec<[f32; 2]> = self
            .bubbles
            .iter()
            .map(|b| self.current_at(b.pos[1]))
            .collect();
        for (b, water) in self.bubbles.iter_mut().zip(currents) {
            b.level += (level(b.band) - b.level) * (dt * 3.0).min(1.0);
            let target = target_radius(b.band, b.scale, b.level);
            b.radius += (target - b.radius) * (dt * 2.5).min(1.0);
            let rest_wobble = 0.06 + 0.3 * b.level;
            b.wobble += (rest_wobble - b.wobble) * (dt * 1.5).min(1.0);
            b.phase += dt * (2.5 + 0.8 * b.band as f32) * (0.6 + b.level);
            b.film += dt * 0.04;

            // Gentle buoyancy (bigger rises a little faster), a lazy side-to-side sway, and the
            // swell. Small bubbles are carried by the water more readily than big ones.
            let rise = 0.04 + 0.6 * b.radius + water[1];
            let sway = 0.035 * (t * (0.7 + 0.15 * b.band as f32) + b.seed * 40.0).sin();
            let drag = 1.2 + 2.5 * (1.0 - (b.radius / 0.12).min(1.0));
            b.vel[1] += (rise - b.vel[1]) * (dt * 1.2).min(1.0);
            b.vel[0] += (sway + water[0] - b.vel[0]) * (dt * drag).min(1.0);
            // Keep bubbles on screen horizontally.
            let edge = aspect - b.radius;
            if b.pos[0].abs() > edge {
                b.vel[0] -= b.pos[0].signum() * dt * 0.5;
            }
            b.pos[0] += b.vel[0] * dt;
            b.pos[1] += b.vel[1] * dt;
        }
    }

    /// Soft collisions: overlapping bubbles nudge each other apart.
    fn collide(&mut self) {
        let n = self.bubbles.len();
        for i in 0..n {
            for j in i + 1..n {
                let (a, b) = (self.bubbles[i], self.bubbles[j]);
                let d = [b.pos[0] - a.pos[0], b.pos[1] - a.pos[1]];
                let dist = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-5);
                let overlap = a.radius + b.radius - dist;
                if overlap <= 0.0 {
                    continue;
                }
                let push = overlap * 0.25 / dist;
                let (wa, wb) = weights(a.radius, b.radius);
                self.bubbles[i].pos[0] -= d[0] * push * wa;
                self.bubbles[i].pos[1] -= d[1] * push * wa;
                self.bubbles[j].pos[0] += d[0] * push * wb;
                self.bubbles[j].pos[1] += d[1] * push * wb;
            }
        }
    }

    fn next_f32(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    fn uniform(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next_f32()
    }

    fn pick(&mut self, n: usize) -> usize {
        ((self.next_f32() * n as f32) as usize).min(n - 1)
    }
}

/// Share of a collision push each bubble takes: the smaller one moves more.
fn weights(ra: f32, rb: f32) -> (f32, f32) {
    let total = (ra + rb).max(1e-5);
    (2.0 * rb / total, 2.0 * ra / total)
}

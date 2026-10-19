//! A screen-filling clockwork mechanism. Kept free of GPU types so the rules can be
//! unit-tested; `render::modes::clockwork` draws the result.
//!
//! Gears sit in [`LAYERS`] parallel planes above a base plate, seen from straight above.
//! Within a layer every gear shares one tooth pitch, and each gear is placed touching the
//! pitch circle of the gear that drives it, so teeth always mesh: a driven gear turns at
//! `-n_driver / n_driven` times its driver, with a phase offset that slots its teeth into the
//! driver's gaps. Gears in higher layers are mounted on the axles of gears below and turn with
//! them. Flywheels, ratchet wheels and plain discs ride on axles; balance wheels swing back
//! and forth on their own hairsprings.
//!
//! One drive angle turns the whole train. It creeps forward with the music's energy, ticks
//! forward with a springy overshoot on every beat, and reverses every few bars or on a big hit.
//!
//! Now and then a gear at the end of a train disengages: it slides out of mesh, sinks into the
//! mechanism under its neighbours and is gone. A replacement of a different size drops in from
//! above the mechanism, already turning in step, and lands with a clunk and a shower of sparks.

use crate::render::sim_math::Rng;
use std::f32::consts::TAU;

pub const MAX_GEARS: usize = 256;
pub const LAYERS: usize = 3;
/// Distance from the camera to each layer's plane, back to front.
pub const LAYER_Z: [f32; LAYERS] = [4.2, 3.75, 3.3];
pub const PLATE_Z: f32 = 4.6;
/// Circular tooth pitch per layer, in world units.
pub const LAYER_PITCH: [f32; LAYERS] = [0.06, 0.07, 0.055];
/// Focal length: at the middle layer the screen spans y ∈ [-1, 1].
pub const FOCAL: f32 = 3.75;
/// Replacements start this far from the camera, above everything.
pub const ARRIVE_Z: f32 = 1.4;
pub const MAX_SPARKS: usize = 240;
pub const DUST_MOTES: usize = 90;
pub const LIGHTS: usize = 3;
/// Half-size of the area kept filled, in screen units, with room for the camera's drift.
const FILL_HALF: [f32; 2] = [2.45, 1.3];
/// Clearance between gears that don't mesh.
const GAP: f32 = 0.012;
const MIN_TEETH: u32 = 8;
const MAX_TEETH: u32 = 96;
/// Keep every gear's speed within this factor of the drive.
const MAX_RATIO: f32 = 6.0;
const MIN_RATIO: f32 = 0.06;
const LEAVE_TIME: f32 = 2.8;
const ARRIVE_TIME: f32 = 2.2;
/// Drive spring: snappy with a little overshoot, like an escapement's tick.
const DRIVE_STIFFNESS: f32 = 90.0;
const DRIVE_DAMPING: f32 = 0.45;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GearKind {
    /// Toothed wheel with straight spokes.
    Spoked = 0,
    /// Toothed wheel with round lightening holes.
    Holed = 1,
    /// Solid toothed disc with a recessed face.
    Disc = 2,
    /// Heavy rim on curved spokes, no teeth; rides on an axle.
    Flywheel = 3,
    /// Small solid gear with a hex nut.
    Pinion = 4,
    /// Thin oscillating wheel with timing screws and a hairspring.
    Balance = 5,
    /// Sawtooth wheel; rides on an axle.
    Ratchet = 6,
}

impl GearKind {
    /// Whether other gears can mesh with it.
    pub fn meshes(self) -> bool {
        matches!(
            self,
            GearKind::Spoked | GearKind::Holed | GearKind::Disc | GearKind::Pinion
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Material {
    Brass = 0,
    Gold = 1,
    Silver = 2,
    BrushedSteel = 3,
    Copper = 4,
    Gunmetal = 5,
    Bronze = 6,
    RoseGold = 7,
    Nickel = 8,
}

const MATERIALS: [Material; 9] = [
    Material::Brass,
    Material::Gold,
    Material::Silver,
    Material::BrushedSteel,
    Material::Copper,
    Material::Gunmetal,
    Material::Bronze,
    Material::RoseGold,
    Material::Nickel,
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Motion {
    Engaged,
    /// Dropping in from above; `t` runs 0..1.
    Arriving {
        t: f32,
    },
    /// Sliding out of mesh and sinking away; `t` runs 0..1.
    Leaving {
        t: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gear {
    pub layer: usize,
    /// Where it sits when engaged.
    pub home: [f32; 2],
    /// Where it is now (it differs while arriving or leaving).
    pub pos: [f32; 3],
    /// Tooth count; 0 for wheels without teeth.
    pub teeth: u32,
    /// Pitch radius (the rim radius for wheels without teeth).
    pub radius: f32,
    pub kind: GearKind,
    pub material: Material,
    pub spokes: u32,
    pub seed: f32,
    /// Turns `ratio` times the drive angle, plus `offset`.
    pub ratio: f32,
    offset: f32,
    /// The gear it meshes with, or whose axle it rides on.
    pub driver: Option<usize>,
    /// True when mounted on the driver's axle rather than meshing with it.
    pub on_axle: bool,
    pub angle: f32,
    /// Angular velocity, radians per second.
    pub spin: f32,
    pub motion: Motion,
    /// Brightness: sinking gears fall into shadow.
    pub fade: f32,
    leave_from: [f32; 3],
    leave_to: [f32; 3],
    arrive_from: [f32; 3],
}

impl Gear {
    /// Gear module: pitch diameter per tooth.
    pub fn module(&self) -> f32 {
        if self.teeth == 0 {
            0.0
        } else {
            2.0 * self.radius / self.teeth as f32
        }
    }

    /// Radius to the tips of the teeth.
    pub fn outer_radius(&self) -> f32 {
        self.radius + self.module()
    }

    pub fn engaged(&self) -> bool {
        self.motion == Motion::Engaged
    }
}

/// A spark or a dust mote.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    pub pos: [f32; 3],
    pub vel: [f32; 3],
    pub age: f32,
    pub life: f32,
    /// Sparks are hot streaks; motes just catch the light.
    pub spark: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light {
    pub pos: [f32; 3],
    pub color: [f32; 3],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Drive {
    pub bass: f32,
    pub energy: f32,
    pub treble: f32,
}

struct Pending {
    layer: usize,
    driver: usize,
    on_axle: bool,
    heading: f32,
    old_teeth: u32,
    delay: f32,
}

pub struct GearSim {
    gears: Vec<Option<Gear>>,
    sparks: Vec<Particle>,
    motes: Vec<Particle>,
    rng: Rng,
    time: f32,
    /// The drive angle every engaged gear follows, and its target and velocity.
    drive: f32,
    drive_target: f32,
    drive_vel: f32,
    direction: f32,
    beats: u32,
    flip_cooldown: f32,
    last_bass: f32,
    kick: f32,
    last_kick: f32,
    beat_cooldown: f32,
    beat_pulse: f32,
    balance: f32,
    balance_vel: f32,
    balance_sign: f32,
    swap_timer: f32,
    top_up_timer: f32,
    pending: Vec<Pending>,
    target_count: usize,
    shake: f32,
    shake_offset: [f32; 2],
    lights: [Light; LIGHTS],
    swaps: u32,
    reversals: u32,
}

impl GearSim {
    pub fn new(seed: u64) -> Self {
        let mut sim = Self {
            gears: vec![None; MAX_GEARS],
            sparks: Vec::with_capacity(MAX_SPARKS),
            motes: Vec::with_capacity(DUST_MOTES),
            rng: Rng::new(seed),
            time: 0.0,
            drive: 0.0,
            drive_target: 0.0,
            drive_vel: 0.0,
            direction: 1.0,
            beats: 0,
            flip_cooldown: 0.0,
            last_bass: 0.0,
            kick: 0.0,
            last_kick: 0.0,
            beat_cooldown: 0.0,
            beat_pulse: 0.0,
            balance: 0.0,
            balance_vel: 0.0,
            balance_sign: 1.0,
            swap_timer: 4.0,
            top_up_timer: 0.0,
            pending: Vec::new(),
            target_count: 0,
            shake: 0.0,
            shake_offset: [0.0; 2],
            lights: [Light {
                pos: [0.0, 0.0, 2.0],
                color: [1.0; 3],
            }; LIGHTS],
            swaps: 0,
            reversals: 0,
        };
        sim.build();
        sim.target_count = sim.count();
        for _ in 0..DUST_MOTES {
            let mote = sim.new_mote();
            sim.motes.push(mote);
        }
        sim.update_angles(0.0);
        sim.move_lights(0.0);
        sim
    }

    pub fn gears(&self) -> impl Iterator<Item = (usize, &Gear)> {
        self.gears
            .iter()
            .enumerate()
            .filter_map(|(i, g)| g.as_ref().map(|g| (i, g)))
    }

    pub fn gear(&self, i: usize) -> Option<&Gear> {
        self.gears.get(i).and_then(Option::as_ref)
    }

    pub fn count(&self) -> usize {
        self.gears.iter().flatten().count()
    }

    pub fn sparks(&self) -> &[Particle] {
        &self.sparks
    }

    pub fn motes(&self) -> &[Particle] {
        &self.motes
    }

    pub fn lights(&self) -> &[Light; LIGHTS] {
        &self.lights
    }

    /// The drive angle.
    pub fn drive_angle(&self) -> f32 {
        self.drive
    }

    /// +1 or -1: which way the train is being driven.
    pub fn direction(&self) -> f32 {
        self.direction
    }

    pub fn beat_pulse(&self) -> f32 {
        self.beat_pulse
    }

    /// Camera shake offset from landings, in world units.
    pub fn shake(&self) -> [f32; 2] {
        self.shake_offset
    }

    pub fn swaps(&self) -> u32 {
        self.swaps
    }

    pub fn reversals(&self) -> u32 {
        self.reversals
    }

    pub fn step(&mut self, dt: f32, drive: Drive) {
        self.time += dt;
        let energy = drive.energy.clamp(0.0, 1.0);
        let beat = self.detect_beat(dt, drive.bass);
        self.beat_pulse = (self.beat_pulse - dt * 3.0).max(0.0);
        if let Some(strength) = beat {
            self.beat_pulse = strength.min(1.0);
        }
        self.turn(dt, energy, beat);
        self.swing_balance(dt, beat);
        self.update_angles(dt);
        self.update_motion(dt);
        self.schedule_swaps(dt, energy, beat);
        self.update_particles(dt, drive.treble);
        self.move_lights(energy);
        self.shake *= (-6.0 * dt).exp();
        let t = self.time * 37.0;
        self.shake_offset = [t.sin() * self.shake, (t * 1.31).cos() * self.shake];
    }

    // ------------------------------------------------------------ building the mechanism

    fn build(&mut self) {
        for layer in 0..LAYERS {
            if layer == 0 {
                let teeth = 30 + self.rng.pick(40) as u32;
                let kind = GearKind::Spoked;
                let gear = self.make_gear(layer, [0.0, 0.0], teeth, kind, 1.0, 0.0, None, false);
                self.insert(gear);
            } else {
                // Mount a few gears on axles from the layer below to start this layer's trains.
                let below: Vec<usize> = self
                    .gears()
                    .filter(|(_, g)| g.layer == layer - 1)
                    .map(|(i, _)| i)
                    .collect();
                for _ in 0..14 {
                    let base = below[self.rng.pick(below.len())];
                    self.mount_on_axle(base, layer, None);
                }
            }
            for _ in 0..1400 {
                self.grow_in(layer, None);
            }
        }
        // A couple of balance wheels ticking away in the top layer.
        let top = LAYERS - 1;
        let mut placed = 0;
        for _ in 0..200 {
            if placed == 2 {
                break;
            }
            let r = self.rng.uniform(0.18, 0.3);
            let home = self.random_point(top);
            if self.fits(top, home, r, &[]) {
                let gear = self.make_gear(top, home, 0, GearKind::Balance, 0.0, 0.0, None, false);
                let gear = Gear { radius: r, ..gear };
                self.insert(gear);
                placed += 1;
            }
        }
    }

    fn random_point(&mut self, layer: usize) -> [f32; 2] {
        let scale = LAYER_Z[layer] / FOCAL;
        [
            self.rng.uniform(-FILL_HALF[0], FILL_HALF[0]) * scale,
            self.rng.uniform(-FILL_HALF[1], FILL_HALF[1]) * scale,
        ]
    }

    fn in_bounds(&self, layer: usize, home: [f32; 2], outer: f32) -> bool {
        let scale = LAYER_Z[layer] / FOCAL;
        home[0].abs() - outer * 0.5 < FILL_HALF[0] * scale
            && home[1].abs() - outer * 0.5 < FILL_HALF[1] * scale
    }

    /// Whether a gear of outer radius `outer` at `home` clears every gear in `layer`, apart
    /// from those in `except` (its driver, or a gear on its way out).
    fn fits(&self, layer: usize, home: [f32; 2], outer: f32, except: &[usize]) -> bool {
        self.gears().all(|(i, g)| {
            if g.layer != layer || except.contains(&i) || matches!(g.motion, Motion::Leaving { .. })
            {
                return true;
            }
            let d = ((g.home[0] - home[0]).powi(2) + (g.home[1] - home[1]).powi(2)).sqrt();
            d >= outer + g.outer_radius() + GAP
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn make_gear(
        &mut self,
        layer: usize,
        home: [f32; 2],
        teeth: u32,
        kind: GearKind,
        ratio: f32,
        offset: f32,
        driver: Option<usize>,
        on_axle: bool,
    ) -> Gear {
        let radius = teeth as f32 * LAYER_PITCH[layer] / TAU;
        let rng = &mut self.rng;
        let spokes = match kind {
            GearKind::Flywheel => 5 + rng.pick(4) as u32,
            GearKind::Balance => 3,
            _ => 3 + rng.pick(4) as u32,
        };
        Gear {
            layer,
            home,
            pos: [home[0], home[1], LAYER_Z[layer]],
            teeth,
            radius,
            kind,
            material: MATERIALS[rng.pick(MATERIALS.len())],
            spokes,
            seed: rng.next_f32(),
            ratio,
            offset,
            driver,
            on_axle,
            angle: offset,
            spin: 0.0,
            motion: Motion::Engaged,
            fade: 1.0,
            leave_from: [0.0; 3],
            leave_to: [0.0; 3],
            arrive_from: [0.0; 3],
        }
    }

    fn insert(&mut self, gear: Gear) -> Option<usize> {
        let slot = self.gears.iter().position(Option::is_none)?;
        self.gears[slot] = Some(gear);
        Some(slot)
    }

    /// A random tooth count, log-uniform so small gears are as common as big ones.
    fn random_teeth(&mut self, max: u32) -> u32 {
        let (lo, hi) = ((MIN_TEETH as f32).ln(), (max.max(MIN_TEETH) as f32).ln());
        self.rng.uniform(lo, hi).exp().round() as u32
    }

    fn random_kind(&mut self, teeth: u32) -> GearKind {
        if teeth < 16 {
            return if self.rng.next_f32() < 0.6 {
                GearKind::Pinion
            } else {
                GearKind::Disc
            };
        }
        [
            GearKind::Spoked,
            GearKind::Spoked,
            GearKind::Holed,
            GearKind::Disc,
        ][self.rng.pick(4)]
    }

    /// Tries to add a gear meshing with some gear already in `layer` (or with `driver`, at
    /// `heading`, if given). Returns the new gear's slot.
    fn grow_in(&mut self, layer: usize, attach: Option<(usize, f32, u32)>) -> Option<usize> {
        let (driver, heading, avoid_teeth) = match attach {
            Some(a) => a,
            None => {
                let drivers: Vec<usize> = self
                    .gears()
                    .filter(|(_, g)| g.layer == layer && g.kind.meshes() && g.engaged())
                    .map(|(i, _)| i)
                    .collect();
                if drivers.is_empty() {
                    return None;
                }
                let d = drivers[self.rng.pick(drivers.len())];
                (d, self.rng.uniform(0.0, TAU), 0)
            }
        };
        let parent = *self.gear(driver)?;
        if !parent.kind.meshes() {
            return None;
        }
        let pitch = LAYER_PITCH[layer];
        let mut teeth = self.random_teeth(MAX_TEETH);
        if avoid_teeth != 0 && teeth.abs_diff(avoid_teeth) < 4 {
            teeth = if avoid_teeth > 40 {
                self.random_teeth(avoid_teeth - 4)
            } else {
                avoid_teeth + 4 + self.rng.pick(40) as u32
            };
        }
        let ratio = -parent.ratio * parent.teeth as f32 / teeth as f32;
        if !(MIN_RATIO..=MAX_RATIO).contains(&ratio.abs()) {
            return None;
        }
        let radius = teeth as f32 * pitch / TAU;
        let module = 2.0 * radius / teeth as f32;
        let dist = parent.radius + radius;
        let home = [
            parent.home[0] + heading.cos() * dist,
            parent.home[1] + heading.sin() * dist,
        ];
        let outer = radius + module;
        if !self.in_bounds(layer, home, outer) || !self.fits(layer, home, outer, &[driver]) {
            return None;
        }
        // Slot the new gear's teeth into the driver's gaps at the contact point: the tooth
        // phases of meshing gears always sum to a half.
        let parent_phase = (heading - parent.offset) * parent.teeth as f32 / TAU;
        let offset = heading + std::f32::consts::PI - TAU / teeth as f32 * (0.5 - parent_phase);
        let kind = self.random_kind(teeth);
        let gear = self.make_gear(layer, home, teeth, kind, ratio, offset, Some(driver), false);
        self.insert(gear)
    }

    /// Mounts a new wheel in `layer` on the axle of gear `base`.
    fn mount_on_axle(
        &mut self,
        base: usize,
        layer: usize,
        avoid_teeth: Option<u32>,
    ) -> Option<usize> {
        let below = *self.gear(base)?;
        let roll = self.rng.next_f32();
        let kind = if roll < 0.15 {
            GearKind::Flywheel
        } else if roll < 0.25 {
            GearKind::Ratchet
        } else {
            GearKind::Spoked
        };
        let mut teeth = self.random_teeth(80).max(14);
        if let Some(old) = avoid_teeth
            && teeth.abs_diff(old) < 4
        {
            teeth = if old > 40 { old - 12 } else { old + 12 };
        }
        let offset = self.rng.uniform(0.0, TAU);
        let mut gear = self.make_gear(
            layer,
            below.home,
            teeth,
            kind,
            below.ratio,
            offset,
            Some(base),
            true,
        );
        if !kind.meshes() {
            // Wheels without meshing teeth are sized freely.
            gear.radius = self.rng.uniform(0.15, 0.45);
            gear.teeth = match kind {
                GearKind::Ratchet => (gear.radius * TAU / LAYER_PITCH[layer]).round() as u32,
                _ => 0,
            };
        }
        if !self.in_bounds(layer, gear.home, gear.outer_radius())
            || !self.fits(layer, gear.home, gear.outer_radius(), &[])
        {
            return None;
        }
        self.insert(gear)
    }

    // ------------------------------------------------------------ driving

    fn detect_beat(&mut self, dt: f32, bass: f32) -> Option<f32> {
        let rise = (bass - self.last_bass).max(0.0);
        self.last_bass = bass;
        self.kick = (self.kick * (1.0 - dt * 4.0).max(0.0) + rise * 3.0).min(1.5);
        self.beat_cooldown -= dt;
        let hit = self.kick - self.last_kick > 0.1 && self.kick > 0.3 && self.beat_cooldown <= 0.0;
        self.last_kick = self.kick;
        if !hit {
            return None;
        }
        self.beat_cooldown = 0.2;
        self.beats += 1;
        Some(self.kick.min(1.5))
    }

    fn turn(&mut self, dt: f32, energy: f32, beat: Option<f32>) {
        self.flip_cooldown -= dt;
        self.drive_target += self.direction * (0.1 + 0.5 * energy) * dt;
        if let Some(strength) = beat {
            // Every few bars, or on a big hit, the whole train reverses.
            let bar_end = self.beats.is_multiple_of(16);
            let big_hit = strength > 1.2 && self.rng.next_f32() < 0.35;
            if self.flip_cooldown <= 0.0 && (bar_end || big_hit) {
                self.direction = -self.direction;
                self.flip_cooldown = 4.0;
                self.reversals += 1;
                self.grind_sparks();
            }
            // Tick forward.
            self.drive_target += self.direction * 0.22 * strength.min(1.2);
        }
        let damping = 2.0 * DRIVE_DAMPING * DRIVE_STIFFNESS.sqrt();
        let accel = DRIVE_STIFFNESS * (self.drive_target - self.drive) - damping * self.drive_vel;
        self.drive_vel += accel * dt;
        self.drive += self.drive_vel * dt;
    }

    fn swing_balance(&mut self, dt: f32, beat: Option<f32>) {
        if let Some(strength) = beat {
            self.balance_sign = -self.balance_sign;
            self.balance_vel += self.balance_sign * 6.0 * strength.min(1.2);
        }
        let omega = TAU * 1.6;
        let accel = -omega * omega * self.balance - 1.2 * self.balance_vel;
        self.balance_vel += accel * dt;
        self.balance += self.balance_vel * dt;
    }

    fn update_angles(&mut self, dt: f32) {
        let (drive, vel, balance, balance_vel) =
            (self.drive, self.drive_vel, self.balance, self.balance_vel);
        for g in self.gears.iter_mut().flatten() {
            match g.motion {
                Motion::Leaving { .. } => {
                    // Freewheeling to a stop.
                    g.spin *= (-0.7 * dt).exp();
                    g.angle += g.spin * dt;
                }
                _ if g.kind == GearKind::Balance => {
                    let amplitude = 0.8 + 0.6 * g.seed;
                    g.angle = balance * amplitude + g.seed * TAU;
                    g.spin = balance_vel * amplitude;
                }
                _ => {
                    g.angle = g.ratio * drive + g.offset;
                    g.spin = g.ratio * vel;
                }
            }
        }
    }

    // ------------------------------------------------------------ swapping gears

    fn has_dependents(&self, i: usize) -> bool {
        self.gears().any(|(_, g)| g.driver == Some(i))
    }

    fn visible(&self, g: &Gear) -> bool {
        let scale = g.pos[2] / FOCAL;
        g.home[0].abs() < 1.6 * scale && g.home[1].abs() < 0.9 * scale
    }

    fn schedule_swaps(&mut self, dt: f32, energy: f32, beat: Option<f32>) {
        self.swap_timer -= dt * (0.6 + energy);
        if beat.is_some_and(|s| s > 1.1) && self.rng.next_f32() < 0.15 {
            self.swap_timer = 0.0;
        }
        if self.swap_timer <= 0.0 {
            self.swap_timer = self.rng.uniform(3.0, 7.0);
            self.start_swap();
        }

        // Replacements drop in a moment after their predecessor starts to leave.
        let mut ready = Vec::new();
        self.pending.retain_mut(|p| {
            p.delay -= dt;
            if p.delay <= 0.0 {
                ready.push((p.layer, p.driver, p.on_axle, p.heading, p.old_teeth));
                false
            } else {
                true
            }
        });
        for (layer, driver, on_axle, heading, old_teeth) in ready {
            self.drop_in(layer, driver, on_axle, heading, old_teeth);
        }

        // Keep the mechanism full: fill any gaps left by swaps that found no fit.
        self.top_up_timer -= dt;
        if self.top_up_timer <= 0.0 {
            self.top_up_timer = 1.5;
            let arriving = self
                .gears()
                .filter(|(_, g)| matches!(g.motion, Motion::Arriving { .. }))
                .count();
            if self.count() < self.target_count && arriving < 2 {
                let layer = self.rng.pick(LAYERS);
                for _ in 0..80 {
                    if let Some(slot) = self.grow_in(layer, None) {
                        self.begin_arrival(slot);
                        break;
                    }
                }
            }
        }
    }

    fn start_swap(&mut self) {
        let leaves: Vec<usize> = self
            .gears()
            .filter(|&(i, g)| {
                g.engaged()
                    && g.driver.is_some()
                    && g.kind != GearKind::Balance
                    && self.visible(g)
                    && !self.has_dependents(i)
            })
            .map(|(i, _)| i)
            .collect();
        if leaves.is_empty() {
            return;
        }
        let i = leaves[self.rng.pick(leaves.len())];
        let Some(g) = self.gears[i] else { return };
        let driver = g.driver.unwrap_or(i);
        let parent = self.gears[driver].unwrap_or(g);
        let heading = (g.home[1] - parent.home[1]).atan2(g.home[0] - parent.home[0]);

        // Slide out of mesh, then sink under the nearest neighbour in the same layer.
        let out = if g.on_axle {
            0.0
        } else {
            2.5 * g.module().max(0.02)
        };
        let from = [
            g.home[0] + heading.cos() * out,
            g.home[1] + heading.sin() * out,
            LAYER_Z[g.layer],
        ];
        let neighbour = self
            .gears()
            .filter(|&(j, n)| j != i && j != driver && n.layer == g.layer && n.engaged())
            .min_by(|a, b| dist2(a.1.home, g.home).total_cmp(&dist2(b.1.home, g.home)))
            .map(|(_, n)| n.home)
            .unwrap_or(parent.home);
        let to = [
            from[0] + (neighbour[0] - from[0]) * 0.55,
            from[1] + (neighbour[1] - from[1]) * 0.55,
            PLATE_Z - 0.03,
        ];
        let gear = self.gears[i].as_mut().unwrap();
        gear.motion = Motion::Leaving { t: 0.0 };
        gear.leave_from = from;
        gear.leave_to = to;
        self.swaps += 1;
        self.burst([from[0], from[1], from[2] - 0.05], 6, 0.6);
        self.pending.push(Pending {
            layer: g.layer,
            driver,
            on_axle: g.on_axle,
            heading,
            old_teeth: g.teeth,
            delay: 0.9,
        });
    }

    fn drop_in(&mut self, layer: usize, driver: usize, on_axle: bool, heading: f32, old: u32) {
        let driver_alive = self.gear(driver).is_some_and(|g| g.engaged());
        let mut slot = None;
        if driver_alive {
            for k in 0..60 {
                let jitter = if k < 30 {
                    0.0
                } else {
                    self.rng.uniform(-0.6, 0.6)
                };
                slot = if on_axle {
                    self.mount_on_axle(driver, layer, Some(old))
                } else {
                    self.grow_in(layer, Some((driver, heading + jitter, old)))
                };
                if slot.is_some() {
                    break;
                }
            }
        }
        if slot.is_none() {
            for _ in 0..80 {
                slot = self.grow_in(layer, None);
                if slot.is_some() {
                    break;
                }
            }
        }
        if let Some(slot) = slot {
            self.begin_arrival(slot);
        }
    }

    fn begin_arrival(&mut self, slot: usize) {
        let heading = self.rng.uniform(0.0, TAU);
        let drift = self.rng.uniform(0.3, 0.8);
        let g = self.gears[slot].as_mut().unwrap();
        g.arrive_from = [
            g.home[0] + heading.cos() * drift,
            g.home[1] + heading.sin() * drift,
            ARRIVE_Z,
        ];
        g.pos = g.arrive_from;
        g.motion = Motion::Arriving { t: 0.0 };
    }

    fn update_motion(&mut self, dt: f32) {
        let mut landed = Vec::new();
        for (i, slot) in self.gears.iter_mut().enumerate() {
            let Some(g) = slot else { continue };
            match g.motion {
                Motion::Engaged => {}
                Motion::Leaving { t } => {
                    let t = t + dt / LEAVE_TIME;
                    if t >= 1.0 {
                        *slot = None;
                        continue;
                    }
                    // A quick slide out of mesh, then a slow sink beneath the others.
                    let slide = (t / 0.15).min(1.0);
                    let sink = ((t - 0.15) / 0.85).max(0.0);
                    let sink = sink * sink * (3.0 - 2.0 * sink);
                    let home = [g.home[0], g.home[1], LAYER_Z[g.layer]];
                    let out = lerp3(home, g.leave_from, slide);
                    g.pos = lerp3(out, g.leave_to, sink);
                    g.fade = 1.0 - 0.75 * sink;
                    g.motion = Motion::Leaving { t };
                }
                Motion::Arriving { t } => {
                    let t = t + dt / ARRIVE_TIME;
                    let home = [g.home[0], g.home[1], LAYER_Z[g.layer]];
                    if t >= 1.0 {
                        g.pos = home;
                        g.motion = Motion::Engaged;
                        continue;
                    }
                    // Descend over the first 70%, then bounce once or twice: clunk.
                    let fall = (t / 0.7).min(1.0);
                    let ease = 1.0 - (1.0 - fall).powi(3);
                    g.pos = lerp3(g.arrive_from, home, ease);
                    if t > 0.7 {
                        let tau = (t - 0.7) * ARRIVE_TIME;
                        g.pos[2] -= 0.05 * (-7.0 * tau).exp() * (16.0 * tau).sin().abs();
                    }
                    if t > 0.7 && t - dt / ARRIVE_TIME <= 0.7 {
                        landed.push(i);
                    }
                    g.motion = Motion::Arriving { t };
                }
            }
        }
        for i in landed {
            self.land(i);
        }
    }

    fn land(&mut self, i: usize) {
        let Some(g) = self.gears[i] else { return };
        self.shake += 0.012 * (g.radius / 0.3).min(1.5);
        let at = match g.driver.and_then(|d| self.gears[d]) {
            Some(p) if !g.on_axle => {
                let d = [g.home[0] - p.home[0], g.home[1] - p.home[1]];
                let len = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-4);
                [
                    p.home[0] + d[0] / len * p.radius,
                    p.home[1] + d[1] / len * p.radius,
                    LAYER_Z[g.layer] - 0.02,
                ]
            }
            _ => [g.home[0], g.home[1], LAYER_Z[g.layer] - 0.02],
        };
        self.burst(at, 18, 1.4);
    }

    /// Reversing the train makes the teeth grind: sparks fly at a few meshes in view.
    fn grind_sparks(&mut self) {
        let meshes: Vec<[f32; 3]> = self
            .gears()
            .filter(|(_, g)| g.engaged() && !g.on_axle && self.visible(g))
            .filter_map(|(_, g)| {
                let p = self.gear(g.driver?)?;
                let d = [g.home[0] - p.home[0], g.home[1] - p.home[1]];
                let len = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-4);
                Some([
                    p.home[0] + d[0] / len * p.radius,
                    p.home[1] + d[1] / len * p.radius,
                    LAYER_Z[g.layer] - 0.02,
                ])
            })
            .collect();
        for _ in 0..meshes.len().min(4) {
            let at = meshes[self.rng.pick(meshes.len())];
            self.burst(at, 8, 1.0);
        }
    }

    // ------------------------------------------------------------ particles and lights

    fn burst(&mut self, at: [f32; 3], count: usize, speed: f32) {
        for _ in 0..count {
            if self.sparks.len() >= MAX_SPARKS {
                self.sparks.remove(0);
            }
            let rng = &mut self.rng;
            let angle = rng.uniform(0.0, TAU);
            let v = speed * rng.uniform(0.3, 1.0);
            self.sparks.push(Particle {
                pos: at,
                vel: [
                    angle.cos() * v,
                    angle.sin() * v + 0.4,
                    -rng.uniform(0.0, 0.6),
                ],
                age: 0.0,
                life: rng.uniform(0.4, 1.1),
                spark: true,
            });
        }
    }

    fn new_mote(&mut self) -> Particle {
        let rng = &mut self.rng;
        let z = rng.uniform(1.6, LAYER_Z[LAYERS - 1] - 0.1);
        let scale = z / FOCAL;
        Particle {
            pos: [
                rng.uniform(-1.9, 1.9) * scale,
                rng.uniform(-1.1, 1.1) * scale,
                z,
            ],
            vel: [rng.uniform(-0.02, 0.02), rng.uniform(0.0, 0.03), 0.0],
            age: 0.0,
            life: rng.uniform(8.0, 20.0),
            spark: false,
        }
    }

    fn update_particles(&mut self, dt: f32, treble: f32) {
        for s in &mut self.sparks {
            s.age += dt;
            s.vel[1] -= 3.5 * dt; // gravity
            s.vel = s.vel.map(|v| v * (-0.6 * dt).exp());
            for k in 0..3 {
                s.pos[k] += s.vel[k] * dt;
            }
        }
        self.sparks.retain(|s| s.age < s.life);
        let t = self.time;
        for i in 0..self.motes.len() {
            let m = &mut self.motes[i];
            m.age += dt;
            let swirl = 0.015 * (1.0 + 2.0 * treble);
            m.pos[0] += (m.vel[0] + swirl * (t * 0.3 + m.life).sin()) * dt;
            m.pos[1] += (m.vel[1] + swirl * (t * 0.23 + m.life * 1.7).cos()) * dt;
            if m.age > m.life {
                self.motes[i] = self.new_mote();
            }
        }
    }

    /// Three lamps wander over the mechanism: a warm tungsten, a cool steel-blue and a white
    /// key that swells on the beat.
    fn move_lights(&mut self, energy: f32) {
        let t = self.time;
        let pulse = self.beat_pulse;
        self.lights = [
            Light {
                pos: [
                    1.6 * (t * 0.11).sin(),
                    0.9 * (t * 0.17 + 1.0).sin(),
                    2.2 + 0.4 * (t * 0.07).sin(),
                ],
                color: [1.0, 0.72, 0.42].map(|c| c * (2.2 + 1.5 * pulse)),
            },
            Light {
                pos: [
                    1.8 * (t * 0.083 + 2.0).cos(),
                    1.0 * (t * 0.13).cos(),
                    1.8 + 0.5 * (t * 0.05 + 1.0).sin(),
                ],
                color: [0.5, 0.7, 1.0].map(|c| c * (1.4 + energy)),
            },
            Light {
                pos: [
                    1.2 * (t * 0.19 + 4.0).sin(),
                    0.7 * (t * 0.23 + 2.0).cos(),
                    2.6,
                ],
                color: [1.0, 0.97, 0.92].map(|c| c * (0.9 + 1.2 * pulse)),
            },
        ];
    }
}

fn dist2(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

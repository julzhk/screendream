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
//! Every second or so a gear at the end of a train in the top layer disengages: it slides out
//! of mesh and is lifted up and away over the mechanism. A replacement of a different size
//! drops in from above, already turning in step, and lands with a clunk and a shower of sparks.
//!
//! Crank pins on some top-layer gears work [`Linkage`]s just above the gears: walking beams
//! that rock on a pivot and pump a piston, plain crank-and-rod pistons, and scotch yokes. Each
//! cylinder chuffs steam at the end of its stroke; reversals vent big clouds. Sparks fly at
//! landings, reversals, on every beat and in a steady treble-driven grind, with slow embers
//! drifting up among the dust.

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
pub const MAX_SPARKS: usize = 1500;
pub const DUST_MOTES: usize = 320;
pub const MAX_PUFFS: usize = 180;
pub const MAX_LINKAGES: usize = 4;
/// Linkages work in this plane, just above the top layer of gears.
pub const LINK_Z: f32 = 3.12;
/// Three wandering lamps followed by two spotlights strafing the mechanism.
pub const LIGHTS: usize = 5;
/// Puffs released per second by the chimney vents, between chuffs and bursts.
const VENT_RATE: f32 = 7.0;
/// Industrial smoke colours: greys, verdigris and cobalt blue.
pub const SMOKE_TINTS: [[f32; 3]; 5] = [
    [0.55, 0.56, 0.58],
    [0.38, 0.39, 0.42],
    [0.33, 0.62, 0.52],
    [0.22, 0.45, 0.4],
    [0.24, 0.36, 0.85],
];
/// Half-size of the area kept filled, in screen units, with room for the camera's drift.
const FILL_HALF: [f32; 2] = [2.45, 1.3];
/// Clearance between gears that don't mesh.
const GAP: f32 = 0.012;
const MIN_TEETH: u32 = 8;
const MAX_TEETH: u32 = 96;
/// Keep every gear's speed within this factor of the drive.
const MAX_RATIO: f32 = 6.0;
const MIN_RATIO: f32 = 0.06;
const LEAVE_TIME: f32 = 2.4;
const ARRIVE_TIME: f32 = 2.6;
/// Arrivals hover while fading in for this share of their time, then fall.
const ARRIVE_HOVER: f32 = 0.2;
/// Arrivals land at this point in their time, then bounce.
const ARRIVE_LAND: f32 = 0.75;
/// Departures fade out over the last part of their path.
const LEAVE_FADE_FROM: f32 = 0.7;
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
    /// Opacity: arrivals fade in before they fall, departures fade out at the end of their path.
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
    /// Embers float up slowly instead of falling.
    pub ember: bool,
}

/// A billow of steam, expanding and rising as it fades.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Puff {
    pub pos: [f32; 3],
    pub vel: [f32; 3],
    pub age: f32,
    pub life: f32,
    pub size: f32,
    pub seed: f32,
    pub tint: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarStyle {
    /// Connecting rod with a pin eye at each end.
    Rod = 0,
    /// Riveted crossbeam with a lattice of cut-outs.
    Truss = 1,
    /// Steam cylinder: a ribbed tube with bolted end flanges.
    Cylinder = 2,
    /// Crosshead or guide block.
    Block = 3,
    /// Round boss: a pivot pedestal or crank pin.
    Boss = 4,
    /// Slotted yoke that a crank pin slides along.
    Yoke = 5,
}

/// One rigid part of a linkage, drawn as a capsule from `a` to `b`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bar {
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub z: f32,
    pub width: f32,
    pub style: BarStyle,
    pub material: Material,
    pub seed: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkageKind {
    /// A crossbeam rocking on a central pivot: one end is driven by a rod from the crank,
    /// the other pumps a piston.
    WalkingBeam,
    /// Crank, connecting rod and piston.
    SliderCrank,
    /// The crank pin slides in a slotted yoke, driving a rod back and forth.
    ScotchYoke,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Linkage {
    pub kind: LinkageKind,
    /// The gear whose face carries the crank pin.
    pub driver: usize,
    pub crank: f32,
    phase: f32,
    /// Main direction of the mechanism, away from the driving gear.
    axis: [f32; 2],
    /// Beam pivot and half-length, for walking beams.
    pub pivot: [f32; 2],
    pub beam_half: f32,
    /// Length of the rod from the crank (or beam end) to the piston.
    pub rod: f32,
    /// Length of the rod from the crank pin to the beam.
    pub drive_rod: f32,
    slider_origin: [f32; 2],
    // Current joint positions.
    pub crank_pin: [f32; 2],
    pub beam_ends: [[f32; 2]; 2],
    pub piston: [f32; 2],
    /// Piston travel along the axis, and which way it last moved, for steam chuffs.
    travel: f32,
    moving: f32,
    material: Material,
    seed: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light {
    pub pos: [f32; 3],
    pub color: [f32; 3],
    /// Where a spotlight points; unused for lamps.
    pub dir: [f32; 3],
    /// Cosine of a spotlight's half-angle, or `None` for a lamp shining every way.
    pub cone: Option<f32>,
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
    puffs: Vec<Puff>,
    linkages: Vec<Linkage>,
    bars: Vec<Bar>,
    grind_debt: f32,
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
    /// How fast the train runs since the last reversal.
    speed_scale: f32,
    /// Chimney vents: where they are and the colour of their smoke.
    vents: Vec<([f32; 2], [f32; 3])>,
    vent_debt: f32,
}

impl GearSim {
    pub fn new(seed: u64) -> Self {
        let mut sim = Self {
            gears: vec![None; MAX_GEARS],
            sparks: Vec::with_capacity(MAX_SPARKS),
            motes: Vec::with_capacity(DUST_MOTES),
            puffs: Vec::with_capacity(MAX_PUFFS),
            linkages: Vec::with_capacity(MAX_LINKAGES),
            bars: Vec::new(),
            grind_debt: 0.0,
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
                dir: [0.0, 0.0, 1.0],
                cone: None,
            }; LIGHTS],
            speed_scale: 1.0,
            vents: Vec::new(),
            vent_debt: 0.0,
            swaps: 0,
            reversals: 0,
        };
        sim.build();
        sim.target_count = sim.count();
        sim.update_angles(0.0);
        sim.add_linkages();
        for _ in 0..5 {
            let at = [sim.rng.uniform(-1.5, 1.5), sim.rng.uniform(-0.9, 0.6)];
            let tint = SMOKE_TINTS[sim.rng.pick(SMOKE_TINTS.len())];
            sim.vents.push((at, tint));
        }
        for _ in 0..DUST_MOTES {
            let mote = sim.new_mote();
            sim.motes.push(mote);
        }
        sim.update_linkages();
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

    pub fn puffs(&self) -> &[Puff] {
        &self.puffs
    }

    pub fn linkages(&self) -> &[Linkage] {
        &self.linkages
    }

    /// Every rigid part of every linkage, in its current position.
    pub fn bars(&self) -> &[Bar] {
        &self.bars
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

    /// Speed multiplier picked at the last reversal.
    pub fn speed_scale(&self) -> f32 {
        self.speed_scale
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
        self.update_linkages();
        self.update_motion(dt);
        self.schedule_swaps(dt, energy, beat);
        self.spray(dt, drive.treble, beat);
        self.vent(dt, energy);
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
        let pace = self.direction * self.speed_scale;
        self.drive_target += pace * (0.1 + 0.5 * energy) * dt;
        if let Some(strength) = beat {
            // Every bar or so, or on a big hit, the whole train reverses and picks a new
            // speed, from a crawl to a race.
            let bar_end = self.beats.is_multiple_of(4) && self.rng.next_f32() < 0.6;
            let big_hit = strength > 1.1 && self.rng.next_f32() < 0.6;
            if self.flip_cooldown <= 0.0 && (bar_end || big_hit) {
                self.direction = -self.direction;
                self.speed_scale = self.rng.uniform(0.35, 2.4);
                self.flip_cooldown = 1.2;
                self.reversals += 1;
                self.grind_sparks();
            }
            // Tick forward.
            let pace = self.direction * self.speed_scale.sqrt();
            self.drive_target += pace * 0.22 * strength.min(1.2);
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
            || self.linkages.iter().any(|l| l.driver == i)
    }

    fn visible(&self, g: &Gear) -> bool {
        let scale = g.pos[2] / FOCAL;
        g.home[0].abs() < 1.6 * scale && g.home[1].abs() < 0.9 * scale
    }

    fn schedule_swaps(&mut self, dt: f32, energy: f32, beat: Option<f32>) {
        self.swap_timer -= dt * (0.6 + energy);
        if beat.is_some_and(|s| s > 0.9) && self.rng.next_f32() < 0.5 {
            self.swap_timer = 0.0;
        }
        if self.swap_timer <= 0.0 {
            self.swap_timer = self.rng.uniform(0.3, 0.7);
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
            self.top_up_timer = 0.4;
            let arriving = self
                .gears()
                .filter(|(_, g)| matches!(g.motion, Motion::Arriving { .. }))
                .count();
            if self.count() < self.target_count && arriving < 6 {
                let layer = LAYERS - 1;
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
                    && g.layer == LAYERS - 1
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

        // Slide out of mesh, then lift up and away over the mechanism.
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
        let away = self.rng.uniform(0.0, TAU);
        let drift = self.rng.uniform(0.5, 1.1);
        let to = [
            from[0] + away.cos() * drift,
            from[1] + away.sin() * drift,
            ARRIVE_Z,
        ];
        let gear = self.gears[i].as_mut().unwrap();
        gear.motion = Motion::Leaving { t: 0.0 };
        gear.leave_from = from;
        gear.leave_to = to;
        self.swaps += 1;
        self.burst([from[0], from[1], from[2] - 0.05], 24, 0.8);
        self.pending.push(Pending {
            layer: g.layer,
            driver,
            on_axle: g.on_axle,
            heading,
            old_teeth: g.teeth,
            delay: 0.5,
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
        g.fade = 0.0;
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
                    // A quick slide out of mesh, then lifted up and away, never through
                    // the gears below.
                    let slide = (t / 0.15).min(1.0);
                    let lift = ((t - 0.15) / 0.85).max(0.0);
                    let lift = lift * lift * (3.0 - 2.0 * lift);
                    let home = [g.home[0], g.home[1], LAYER_Z[g.layer]];
                    let out = lerp3(home, g.leave_from, slide);
                    g.pos = lerp3(out, g.leave_to, lift);
                    g.fade = 1.0 - smoothstep01((t - LEAVE_FADE_FROM) / (1.0 - LEAVE_FADE_FROM));
                    g.motion = Motion::Leaving { t };
                }
                Motion::Arriving { t } => {
                    let t = t + dt / ARRIVE_TIME;
                    let home = [g.home[0], g.home[1], LAYER_Z[g.layer]];
                    if t >= 1.0 {
                        g.pos = home;
                        g.fade = 1.0;
                        g.motion = Motion::Engaged;
                        continue;
                    }
                    // Fade in while hovering high above, fall, then bounce once or twice:
                    // clunk.
                    g.fade = smoothstep01(t / ARRIVE_HOVER);
                    let fall = ((t - ARRIVE_HOVER) / (ARRIVE_LAND - ARRIVE_HOVER)).clamp(0.0, 1.0);
                    let ease = fall * fall * fall;
                    g.pos = lerp3(g.arrive_from, home, ease);
                    if t > ARRIVE_LAND {
                        let tau = (t - ARRIVE_LAND) * ARRIVE_TIME;
                        g.pos[2] -= 0.05 * (-7.0 * tau).exp() * (16.0 * tau).sin().abs();
                    }
                    if t > ARRIVE_LAND && t - dt / ARRIVE_TIME <= ARRIVE_LAND {
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
        self.burst(at, 70, 1.6);
        self.puff([at[0], at[1], LINK_Z - 0.1], [0.0, 0.1, 0.0], 0.6);
    }

    /// Reversing the train makes the teeth grind: sparks fly at a few meshes in view.
    fn grind_sparks(&mut self) {
        let meshes = self.mesh_points();
        for _ in 0..meshes.len().min(8) {
            let at = meshes[self.rng.pick(meshes.len())];
            self.burst(at, 30, 1.3);
        }
        // And the safety valves blow.
        for _ in 0..4 {
            let x = self.rng.uniform(-1.4, 1.4);
            let y = self.rng.uniform(-0.8, 0.8);
            let up = self.rng.uniform(0.15, 0.35);
            self.puff([x, y, LINK_Z - 0.15], [0.0, up, 0.0], 1.4);
        }
    }

    // ------------------------------------------------------------ linkages

    /// Fits a few linkages onto big gears in the top layer, in view.
    fn add_linkages(&mut self) {
        let top = LAYERS - 1;
        let kinds = [
            LinkageKind::WalkingBeam,
            LinkageKind::SliderCrank,
            LinkageKind::ScotchYoke,
            LinkageKind::WalkingBeam,
        ];
        let mut footprints: Vec<([f32; 2], f32)> = Vec::new();
        for kind in kinds {
            for _ in 0..200 {
                let candidates: Vec<usize> = self
                    .gears()
                    .filter(|(_, g)| {
                        g.layer == top
                            && g.engaged()
                            && g.kind != GearKind::Balance
                            && (0.16..0.5).contains(&g.radius)
                            && g.home[0].abs() < 1.5
                            && g.home[1].abs() < 0.85
                    })
                    .map(|(i, _)| i)
                    .collect();
                if candidates.is_empty() {
                    return;
                }
                let driver = candidates[self.rng.pick(candidates.len())];
                if self.linkages.iter().any(|l| l.driver == driver) {
                    continue;
                }
                let Some(linkage) = self.design_linkage(kind, driver) else {
                    continue;
                };
                let (centre, reach) = footprint(&linkage, &self.gears[driver].unwrap());
                let clear = footprints
                    .iter()
                    .all(|(c, r)| dist2(*c, centre).sqrt() > (r + reach) * 0.55);
                let gear = self.gears[driver].unwrap();
                let end = linkage.cylinder_end();
                let pivot_ok = linkage.kind != LinkageKind::WalkingBeam
                    || (linkage.pivot[0].abs() < 1.8 && linkage.pivot[1].abs() < 1.0);
                let on_screen = end[0].abs() < 1.8
                    && end[1].abs() < 1.0
                    && gear.home[0].abs() < 1.6
                    && gear.home[1].abs() < 0.9
                    && pivot_ok;
                if clear && on_screen {
                    footprints.push((centre, reach));
                    self.linkages.push(linkage);
                    break;
                }
            }
        }
    }

    fn design_linkage(&mut self, kind: LinkageKind, driver: usize) -> Option<Linkage> {
        let g = self.gears[driver]?;
        let rng = &mut self.rng;
        let heading = rng.uniform(0.0, TAU);
        let axis = [heading.cos(), heading.sin()];
        let side = if rng.next_f32() < 0.5 { 1.0 } else { -1.0 };
        let perp = [-axis[1] * side, axis[0] * side];
        let crank = g.radius * rng.uniform(0.45, 0.6);
        let mut l = Linkage {
            kind,
            driver,
            crank,
            phase: rng.uniform(0.0, TAU),
            axis,
            pivot: [0.0; 2],
            beam_half: 0.0,
            rod: 0.0,
            drive_rod: 0.0,
            slider_origin: [0.0; 2],
            crank_pin: [0.0; 2],
            beam_ends: [[0.0; 2]; 2],
            piston: [0.0; 2],
            travel: 0.0,
            moving: 0.0,
            material: MATERIALS[rng.pick(MATERIALS.len())],
            seed: rng.next_f32(),
        };
        let at =
            |base: [f32; 2], dir: [f32; 2], d: f32| [base[0] + dir[0] * d, base[1] + dir[1] * d];
        match kind {
            LinkageKind::WalkingBeam => {
                // Pivot beyond the gear; at rest the beam lies across the axis.
                l.pivot = at(g.home, axis, g.radius + rng.uniform(0.3, 0.5));
                l.beam_half = rng.uniform(0.26, 0.4);
                let near_end = at(l.pivot, perp, l.beam_half);
                l.drive_rod = dist2(near_end, g.home).sqrt();
                let far_end = at(l.pivot, perp, -l.beam_half);
                l.rod = rng.uniform(0.2, 0.3);
                l.slider_origin = at(far_end, axis, l.rod);
                l.beam_ends = [near_end, far_end];
            }
            LinkageKind::SliderCrank => {
                l.rod = g.radius + crank + rng.uniform(0.15, 0.3);
                l.slider_origin = g.home;
            }
            LinkageKind::ScotchYoke => {
                l.rod = g.radius + rng.uniform(0.25, 0.4);
                l.slider_origin = g.home;
            }
        }
        // The mechanism must work all the way round.
        for k in 0..64 {
            let angle = k as f32 / 64.0 * TAU;
            if !l.solve(&g, angle) {
                return None;
            }
        }
        l.solve(&g, g.angle);
        Some(l)
    }

    /// Moves every linkage to follow its crank, rebuilds the bars, and lets cylinders chuff
    /// steam at the end of each stroke.
    fn update_linkages(&mut self) {
        self.bars.clear();
        let mut chuffs = Vec::new();
        for l in &mut self.linkages {
            let Some(g) = self.gears[l.driver] else {
                continue;
            };
            let before = l.travel;
            l.solve(&g, g.angle);
            let moving = (l.travel - before).signum();
            if moving != 0.0 && l.moving != 0.0 && moving != l.moving {
                chuffs.push((l.cylinder_end(), l.axis));
            }
            if moving != 0.0 {
                l.moving = moving;
            }
            l.push_bars(&mut self.bars);
        }
        for (at, dir) in chuffs {
            let up = self.rng.uniform(0.05, 0.15);
            self.puff(
                [at[0], at[1], LINK_Z - 0.05],
                [dir[0] * 0.25, dir[1] * 0.25 + up, 0.0],
                0.5,
            );
        }
    }

    /// Sparks on every beat at meshes in view, a steady grind with the treble, and embers.
    fn spray(&mut self, dt: f32, treble: f32, beat: Option<f32>) {
        let beat_sparks = beat.map_or(0, |s| 1 + (s * 2.0) as usize);
        self.grind_debt += dt * 30.0 * treble;
        let grinds = self.grind_debt as usize;
        self.grind_debt -= grinds as f32;
        if beat_sparks == 0 && grinds == 0 {
            return;
        }
        let meshes = self.mesh_points();
        if meshes.is_empty() {
            return;
        }
        let strength = beat.unwrap_or(0.0);
        for _ in 0..beat_sparks {
            let at = meshes[self.rng.pick(meshes.len())];
            self.burst(at, 10 + (strength * 14.0) as usize, 1.1);
        }
        for _ in 0..grinds {
            let at = meshes[self.rng.pick(meshes.len())];
            self.burst(at, 2, 0.7);
        }
        if beat.is_some() && self.rng.next_f32() < 0.3 && !self.linkages.is_empty() {
            let l = self.linkages[self.rng.pick(self.linkages.len())];
            let at = l.cylinder_end();
            self.puff([at[0], at[1], LINK_Z - 0.05], [0.0, 0.15, 0.0], 0.7);
        }
    }

    /// Where engaged, visible gears meet their drivers.
    fn mesh_points(&self) -> Vec<[f32; 3]> {
        self.gears()
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
            .collect()
    }

    /// A plume of smoke: five billows of one colour, jostled apart.
    fn puff(&mut self, at: [f32; 3], vel: [f32; 3], size: f32) {
        let tint = SMOKE_TINTS[self.rng.pick(SMOKE_TINTS.len())];
        for _ in 0..5 {
            self.billow(at, vel, size, tint);
        }
    }

    fn billow(&mut self, at: [f32; 3], vel: [f32; 3], size: f32, tint: [f32; 3]) {
        if self.puffs.len() >= MAX_PUFFS {
            self.puffs.remove(0);
        }
        let rng = &mut self.rng;
        let spread = 0.08 * size;
        self.puffs.push(Puff {
            pos: [
                at[0] + rng.uniform(-spread, spread),
                at[1] + rng.uniform(-spread, spread),
                at[2] + rng.uniform(-0.03, 0.03),
            ],
            vel: [
                vel[0] + rng.uniform(-0.07, 0.07),
                vel[1] + rng.uniform(-0.02, 0.07),
                vel[2],
            ],
            age: 0.0,
            life: rng.uniform(2.5, 4.5) * (0.7 + 0.3 * size),
            size: 0.12 * size * rng.uniform(0.7, 1.3),
            seed: rng.next_f32(),
            tint,
        });
    }

    /// Chimney vents smoke steadily, harder when the music is loud.
    fn vent(&mut self, dt: f32, energy: f32) {
        self.vent_debt += dt * VENT_RATE * (0.5 + energy);
        while self.vent_debt >= 1.0 {
            self.vent_debt -= 1.0;
            let Some(&(at, tint)) = self.vents.get(self.rng.pick(self.vents.len().max(1))) else {
                return;
            };
            let up = self.rng.uniform(0.12, 0.25);
            self.billow([at[0], at[1], LINK_Z - 0.12], [0.0, up, 0.0], 0.8, tint);
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
                life: rng.uniform(0.5, 1.6),
                spark: true,
                ember: rng.next_f32() < 0.15,
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
            ember: false,
        }
    }

    fn update_particles(&mut self, dt: f32, treble: f32) {
        for s in &mut self.sparks {
            s.age += dt;
            if s.ember {
                // Embers are carried up on the hot air.
                s.vel[1] += (0.25 - s.vel[1]) * (dt * 1.5).min(1.0);
                s.vel[0] += 0.3 * (s.age * 5.0 + s.life * 20.0).sin() * dt;
            } else {
                s.vel[1] -= 3.5 * dt; // gravity
            }
            s.vel = s.vel.map(|v| v * (-0.6 * dt).exp());
            for k in 0..3 {
                s.pos[k] += s.vel[k] * dt;
            }
        }
        self.sparks
            .retain(|s| s.age < s.life * if s.ember { 2.5 } else { 1.0 });
        for p in &mut self.puffs {
            p.age += dt;
            p.vel = p.vel.map(|v| v * (-1.2 * dt).exp());
            p.vel[1] += 0.06 * dt;
            p.size += dt * 0.12;
            for k in 0..3 {
                p.pos[k] += p.vel[k] * dt;
            }
        }
        self.puffs.retain(|p| p.age < p.life);
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
        let lamp = |pos: [f32; 3], color: [f32; 3]| Light {
            pos,
            color,
            dir: [0.0, 0.0, 1.0],
            cone: None,
        };
        // Spotlights mounted high at the sides, sweeping their pools across the works.
        let sweep = t * (1.0 + energy);
        let spot = |pos: [f32; 3], target: [f32; 3], color: [f32; 3], half_angle: f32| {
            let d = [target[0] - pos[0], target[1] - pos[1], target[2] - pos[2]];
            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            Light {
                pos,
                color,
                dir: d.map(|v| v / len),
                cone: Some(half_angle.cos()),
            }
        };
        self.lights = [
            lamp(
                [
                    1.6 * (t * 0.11).sin(),
                    0.9 * (t * 0.17 + 1.0).sin(),
                    2.2 + 0.4 * (t * 0.07).sin(),
                ],
                [1.0, 0.72, 0.42].map(|c| c * (2.2 + 1.5 * pulse)),
            ),
            lamp(
                [
                    1.8 * (t * 0.083 + 2.0).cos(),
                    1.0 * (t * 0.13).cos(),
                    1.8 + 0.5 * (t * 0.05 + 1.0).sin(),
                ],
                [0.5, 0.7, 1.0].map(|c| c * (1.4 + energy)),
            ),
            lamp(
                [
                    1.2 * (t * 0.19 + 4.0).sin(),
                    0.7 * (t * 0.23 + 2.0).cos(),
                    2.6,
                ],
                [1.0, 0.97, 0.92].map(|c| c * (0.9 + 1.2 * pulse)),
            ),
            spot(
                [-2.8, 1.5 + 0.3 * (t * 0.2).sin(), 0.7],
                [
                    1.4 * (sweep * 0.37).sin(),
                    0.8 * (sweep * 0.61 + 1.0).sin(),
                    LAYER_Z[1],
                ],
                [0.85, 0.92, 1.0].map(|c| c * (5.0 + 3.0 * pulse)),
                0.17,
            ),
            spot(
                [2.8, -1.4 + 0.3 * (t * 0.17).cos(), 0.9],
                [
                    1.4 * (sweep * 0.29 + 2.5).cos(),
                    0.8 * (sweep * 0.47 + 0.4).cos(),
                    LAYER_Z[1],
                ],
                [1.0, 0.78, 0.45].map(|c| c * (4.5 + 3.0 * pulse)),
                0.2,
            ),
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

impl Linkage {
    /// Positions every joint for crank angle `gear_angle`. Returns false if the parts can't
    /// reach each other there.
    fn solve(&mut self, gear: &Gear, gear_angle: f32) -> bool {
        let a = gear_angle + self.phase;
        self.crank_pin = [
            gear.home[0] + a.cos() * self.crank,
            gear.home[1] + a.sin() * self.crank,
        ];
        match self.kind {
            LinkageKind::WalkingBeam => {
                let Some((e1, e2)) =
                    circles_meet(self.pivot, self.beam_half, self.crank_pin, self.drive_rod)
                else {
                    return false;
                };
                // Stay on the same branch as last time so the beam never flips over.
                let prev = self.beam_ends[0];
                let near = if dist2(e1, prev) <= dist2(e2, prev) {
                    e1
                } else {
                    e2
                };
                let far = [2.0 * self.pivot[0] - near[0], 2.0 * self.pivot[1] - near[1]];
                self.beam_ends = [near, far];
                match slide(self.slider_origin, self.axis, far, self.rod) {
                    Some(s) => self.set_piston(s),
                    None => return false,
                }
            }
            LinkageKind::SliderCrank => match slide(gear.home, self.axis, self.crank_pin, self.rod)
            {
                Some(s) => self.set_piston(s),
                None => return false,
            },
            LinkageKind::ScotchYoke => {
                let rel = [
                    self.crank_pin[0] - gear.home[0],
                    self.crank_pin[1] - gear.home[1],
                ];
                let s = rel[0] * self.axis[0] + rel[1] * self.axis[1];
                self.set_piston(s + self.rod);
            }
        }
        true
    }

    fn set_piston(&mut self, s: f32) {
        self.travel = s;
        self.piston = [
            self.slider_origin[0] + self.axis[0] * s,
            self.slider_origin[1] + self.axis[1] * s,
        ];
    }

    fn along(&self, s: f32) -> [f32; 2] {
        [
            self.slider_origin[0] + self.axis[0] * s,
            self.slider_origin[1] + self.axis[1] * s,
        ]
    }

    /// The closed end of the cylinder, where steam escapes.
    fn cylinder_end(&self) -> [f32; 2] {
        let reach = match self.kind {
            LinkageKind::WalkingBeam => self.beam_half * 0.5 + 0.2,
            _ => self.crank + 0.25,
        };
        self.along(self.rod + reach)
    }

    fn push_bars(&self, bars: &mut Vec<Bar>) {
        let bar = |a, b, z, width, style, material| Bar {
            a,
            b,
            z,
            width,
            style,
            material,
            seed: self.seed,
        };
        let steel = Material::BrushedSteel;
        let stroke = match self.kind {
            LinkageKind::WalkingBeam => self.beam_half * 0.5,
            _ => self.crank,
        };
        // Cylinder (or guide) deepest, then piston rod and crosshead, rods, beam, bosses.
        let cylinder_from = self.along(self.rod + stroke * 0.3);
        bars.push(bar(
            cylinder_from,
            self.cylinder_end(),
            LINK_Z + 0.04,
            0.085,
            BarStyle::Cylinder,
            self.material,
        ));
        bars.push(bar(
            self.piston,
            cylinder_from,
            LINK_Z + 0.03,
            0.018,
            BarStyle::Rod,
            steel,
        ));
        let head = [self.axis[0] * 0.035, self.axis[1] * 0.035];
        let p = self.piston;
        bars.push(bar(
            [p[0] - head[0], p[1] - head[1]],
            [p[0] + head[0], p[1] + head[1]],
            LINK_Z + 0.02,
            0.05,
            BarStyle::Block,
            steel,
        ));
        match self.kind {
            LinkageKind::WalkingBeam => {
                let [near, far] = self.beam_ends;
                bars.push(bar(
                    self.crank_pin,
                    near,
                    LINK_Z,
                    0.026,
                    BarStyle::Rod,
                    steel,
                ));
                bars.push(bar(
                    far,
                    self.piston,
                    LINK_Z + 0.01,
                    0.022,
                    BarStyle::Rod,
                    steel,
                ));
                bars.push(bar(
                    near,
                    far,
                    LINK_Z - 0.03,
                    0.055,
                    BarStyle::Truss,
                    self.material,
                ));
                bars.push(bar(
                    self.pivot,
                    self.pivot,
                    LINK_Z - 0.05,
                    0.05,
                    BarStyle::Boss,
                    Material::Gunmetal,
                ));
            }
            LinkageKind::SliderCrank => {
                bars.push(bar(
                    self.crank_pin,
                    self.piston,
                    LINK_Z,
                    0.028,
                    BarStyle::Rod,
                    steel,
                ));
            }
            LinkageKind::ScotchYoke => {
                let across = [-self.axis[1], self.axis[0]];
                let reach = self.crank * 1.25;
                let centre = self.along(self.travel - self.rod);
                bars.push(bar(
                    [centre[0] - across[0] * reach, centre[1] - across[1] * reach],
                    [centre[0] + across[0] * reach, centre[1] + across[1] * reach],
                    LINK_Z,
                    0.05,
                    BarStyle::Yoke,
                    self.material,
                ));
                bars.push(bar(
                    centre,
                    self.piston,
                    LINK_Z + 0.01,
                    0.022,
                    BarStyle::Rod,
                    steel,
                ));
            }
        }
        bars.push(bar(
            self.crank_pin,
            self.crank_pin,
            LINK_Z - 0.06,
            0.03,
            BarStyle::Boss,
            Material::Gunmetal,
        ));
    }
}

/// Where the circle about `c0` of radius `r0` meets the one about `c1` of radius `r1`.
fn circles_meet(c0: [f32; 2], r0: f32, c1: [f32; 2], r1: f32) -> Option<([f32; 2], [f32; 2])> {
    let d = dist2(c0, c1).sqrt();
    if d < 1e-5 || d > r0 + r1 || d < (r0 - r1).abs() {
        return None;
    }
    let a = (r0 * r0 - r1 * r1 + d * d) / (2.0 * d);
    let h = (r0 * r0 - a * a).max(0.0).sqrt();
    let u = [(c1[0] - c0[0]) / d, (c1[1] - c0[1]) / d];
    let m = [c0[0] + u[0] * a, c0[1] + u[1] * a];
    Some((
        [m[0] - u[1] * h, m[1] + u[0] * h],
        [m[0] + u[1] * h, m[1] - u[0] * h],
    ))
}

/// How far along `dir` from `origin` a point must be to sit `len` from `p` (the far root).
fn slide(origin: [f32; 2], dir: [f32; 2], p: [f32; 2], len: f32) -> Option<f32> {
    let w = [origin[0] - p[0], origin[1] - p[1]];
    let b = w[0] * dir[0] + w[1] * dir[1];
    let disc = b * b - (w[0] * w[0] + w[1] * w[1]) + len * len;
    (disc >= 0.0).then(|| -b + disc.sqrt())
}

/// A circle that roughly covers a linkage, for keeping linkages apart.
fn footprint(l: &Linkage, gear: &Gear) -> ([f32; 2], f32) {
    let end = l.cylinder_end();
    let centre = [(gear.home[0] + end[0]) * 0.5, (gear.home[1] + end[1]) * 0.5];
    (
        centre,
        dist2(gear.home, end).sqrt() * 0.5 + gear.radius * 0.3 + l.beam_half * 0.5,
    )
}

fn smoothstep01(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

//! A living neural culture seen through a microscope. Kept free of GPU types so the rules can be
//! unit-tested; `render::modes::neurons` draws the result.
//!
//! Nodes (cell bodies) drift through a 3D volume that the camera flies through. A node buds a
//! tendril towards a neighbour: the tip wanders, seeks its target harder as it nears, and its
//! growth cone opens like a flower just before contact. Connected tendrils are springs, so the
//! network holds itself in tension; every so often one snaps, both halves recoil and retract,
//! and the freed nodes spring back and the network settles into a new shape.
//!
//! Each tendril is a short verlet rope, which gives it its organic sway. Music onsets in each
//! of four bands fire nodes, and the impulses race along tendrils at a speed set by the band
//! (bass slow and heavy, treble quick and fine), cascading from node to node with decaying
//! energy. A refractory period stops nodes firing straight back.
//!
//! Organelles and bacteria float about: vesicles, mitochondria and lipid droplets drift, while
//! flagellated bacteria swim in runs and tumble to a new heading, more often on bass hits.

use crate::render::sim_math::{Rng, add, cross, dot, length, normalize, rotate, scale, sub};

pub const MAX_NODES: usize = 56;
pub const MAX_TENDRILS: usize = 160;
/// Points per tendril rope.
pub const CHAIN: usize = 14;
pub const MAX_PULSES: usize = 256;
pub const MAX_MICROBES: usize = 80;
pub const MAX_DEBRIS: usize = 160;
pub const BANDS: usize = 4;
/// Nodes live between these distances ahead of the camera.
pub const NEAR_SPAWN: f32 = 2.0;
pub const FAR_SPAWN: f32 = 28.0;
/// Pulse speed per band, in world units per second: bass slow, treble fast.
pub const PULSE_SPEED: [f32; BANDS] = [2.0, 3.2, 4.8, 7.5];
/// Fraction of a pulse's energy passed on at each node.
const PULSE_DECAY: f32 = 0.72;
const PULSE_MIN: f32 = 0.18;
/// A node can't fire again this soon after firing.
const REFRACTORY: f32 = 0.3;
const MAX_GROWING: usize = 16;
const LINK_MIN: f32 = 1.2;
const LINK_MAX: f32 = 6.0;
const SPRING: f32 = 0.8;
const NODE_DAMPING: f32 = 1.4;
/// A bud gives up if it hasn't reached its target after this long.
const GROW_TIMEOUT: f32 = 14.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Node {
    pub pos: [f32; 3],
    pub vel: [f32; 3],
    pub radius: f32,
    /// Position along the current stain's two-colour scale, 0..1.
    pub stain: f32,
    /// Calcium flash from firing, decaying.
    pub glow: f32,
    /// The band that last fired it, for the flash colour.
    pub glow_band: usize,
    pub phase: f32,
    pub seed: f32,
    pub fade: f32,
    pub max_degree: usize,
    refractory: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// The tip is reaching out towards `to`.
    Growing,
    /// Both ends are attached; the tendril is a spring between its nodes.
    Connected,
    /// Severed or abandoned: anchored at `from` only, recoiling and shrinking back.
    Retracting,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tendril {
    pub from: usize,
    pub to: usize,
    pub phase: Phase,
    pub points: [[f32; 3]; CHAIN],
    prev: [[f32; 3]; CHAIN],
    /// Total rest length of the rope.
    pub rest: f32,
    /// Rest length of the spring between the two nodes, once connected.
    link_rest: f32,
    tip_dir: [f32; 3],
    /// How far the growth cone has opened, 0 bud .. 1 full flower.
    pub open: f32,
    /// Bright flash on contact, decaying.
    pub flash: f32,
    pub thickness: f32,
    pub age: f32,
    pub seed: f32,
    pub stain_from: f32,
    pub stain_to: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pulse {
    pub tendril: usize,
    /// Position along the tendril, 0 at `from` .. 1 at `to`.
    pub s: f32,
    pub forward: bool,
    pub speed: f32,
    pub band: usize,
    pub energy: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MicrobeKind {
    Vesicle = 0,
    Mitochondrion = 1,
    /// Rod-shaped bacterium trailing a bundle of flagella.
    Bacillus = 2,
    /// A short chain of round bacteria.
    Coccus = 3,
    /// Corkscrew bacterium with tufts of flagella at both ends.
    Spirillum = 4,
    /// Comma-shaped bacterium with a single flagellum.
    Vibrio = 5,
    /// Shape-shifting blob with pseudopods.
    Amoeba = 6,
    Lipid = 7,
}

const MICROBE_KINDS: [MicrobeKind; 8] = [
    MicrobeKind::Vesicle,
    MicrobeKind::Mitochondrion,
    MicrobeKind::Bacillus,
    MicrobeKind::Coccus,
    MicrobeKind::Spirillum,
    MicrobeKind::Vibrio,
    MicrobeKind::Amoeba,
    MicrobeKind::Lipid,
];

impl MicrobeKind {
    pub fn swims(self) -> bool {
        matches!(
            self,
            MicrobeKind::Bacillus | MicrobeKind::Spirillum | MicrobeKind::Vibrio
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Microbe {
    pub kind: MicrobeKind,
    pub pos: [f32; 3],
    pub vel: [f32; 3],
    /// Body axis; swimmers swim along it.
    pub axis: [f32; 3],
    spin_axis: [f32; 3],
    spin: f32,
    pub size: f32,
    /// Flagella / pseudopod animation phase.
    pub phase: f32,
    pub tint: [f32; 3],
    pub seed: f32,
    swim_speed: f32,
    run_left: f32,
}

/// A speck flung out when a tendril snaps or a synapse forms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Debris {
    pub pos: [f32; 3],
    pub vel: [f32; 3],
    pub age: f32,
    pub life: f32,
    pub size: f32,
    pub stain: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub pos: [f32; 3],
    pub forward: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
}

/// What the culture hears each frame: the four band levels and overall energy, all 0..1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Drive {
    pub bands: [f32; BANDS],
    pub energy: f32,
}

pub struct NeuronSim {
    nodes: Vec<Node>,
    tendrils: Vec<Tendril>,
    pulses: Vec<Pulse>,
    microbes: Vec<Microbe>,
    debris: Vec<Debris>,
    rng: Rng,
    time: f32,
    camera: Camera,
    roll: f32,
    /// When false, nothing grows, snaps or respawns by itself (for tests).
    autonomous: bool,
    last_level: [f32; BANDS],
    kick: [f32; BANDS],
    last_kick: [f32; BANDS],
    cooldown: [f32; BANDS],
    beat_flash: [f32; BANDS],
    grow_debt: f32,
    sever_debt: f32,
    focus: f32,
    focus_target: f32,
    focus_timer: f32,
}

impl NeuronSim {
    /// A full culture: nodes already wired to their neighbours, buds on the way, microbes.
    pub fn new(seed: u64) -> Self {
        let mut sim = Self::empty(seed);
        sim.autonomous = true;
        for _ in 0..MAX_NODES {
            let pos = sim.spawn_position(NEAR_SPAWN * 0.5, FAR_SPAWN);
            let n = sim.add_node(pos);
            sim.nodes[n].fade = 1.0;
        }
        for a in 0..MAX_NODES {
            let links = if sim.rng.next_f32() < 0.4 { 2 } else { 1 };
            for _ in 0..links {
                if let Some(b) = sim.pick_target(a) {
                    sim.connect(a, b);
                }
            }
        }
        for _ in 0..6 {
            let a = sim.rng.pick(MAX_NODES);
            if let Some(b) = sim.pick_target(a) {
                sim.grow(a, b);
            }
        }
        for _ in 0..MAX_MICROBES {
            let microbe = sim.new_microbe(1.0, FAR_SPAWN);
            sim.microbes.push(microbe);
        }
        sim
    }

    /// An empty, passive culture: nodes, tendrils and pulses only appear when asked for.
    pub fn empty(seed: u64) -> Self {
        Self {
            nodes: Vec::with_capacity(MAX_NODES),
            tendrils: Vec::with_capacity(MAX_TENDRILS),
            pulses: Vec::with_capacity(MAX_PULSES),
            microbes: Vec::with_capacity(MAX_MICROBES),
            debris: Vec::with_capacity(MAX_DEBRIS),
            rng: Rng::new(seed),
            time: 0.0,
            camera: Camera {
                pos: [0.0; 3],
                forward: [0.0, 0.0, -1.0],
                right: [1.0, 0.0, 0.0],
                up: [0.0, 1.0, 0.0],
            },
            roll: 0.0,
            autonomous: false,
            last_level: [0.0; BANDS],
            kick: [0.0; BANDS],
            last_kick: [0.0; BANDS],
            cooldown: [0.0; BANDS],
            beat_flash: [0.0; BANDS],
            grow_debt: 0.0,
            sever_debt: 0.0,
            focus: 6.0,
            focus_target: 6.0,
            focus_timer: 0.0,
        }
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn tendrils(&self) -> &[Tendril] {
        &self.tendrils
    }

    pub fn pulses(&self) -> &[Pulse] {
        &self.pulses
    }

    pub fn microbes(&self) -> &[Microbe] {
        &self.microbes
    }

    pub fn debris(&self) -> &[Debris] {
        &self.debris
    }

    pub fn camera(&self) -> Camera {
        self.camera
    }

    /// Distance ahead of the camera that is in sharp focus.
    pub fn focus(&self) -> f32 {
        self.focus
    }

    /// Per band, 1 on an onset and decaying.
    pub fn beat_flash(&self) -> [f32; BANDS] {
        self.beat_flash
    }

    pub fn add_node(&mut self, pos: [f32; 3]) -> usize {
        let node = self.new_node(pos);
        if self.nodes.len() < MAX_NODES {
            self.nodes.push(node);
            self.nodes.len() - 1
        } else {
            let slot = self.rng.pick(MAX_NODES);
            self.replace_node(slot, node);
            slot
        }
    }

    /// Wires `a` to `b` straight away with a relaxed tendril.
    pub fn connect(&mut self, a: usize, b: usize) -> Option<usize> {
        let (pa, pb) = (self.nodes[a].pos, self.nodes[b].pos);
        let mut t = self.new_tendril(a, b);
        let bow = scale(self.rng.unit_vector(), length(sub(pb, pa)) * 0.12);
        for (i, p) in t.points.iter_mut().enumerate() {
            let s = i as f32 / (CHAIN - 1) as f32;
            let sag = (s * std::f32::consts::PI).sin();
            *p = add(add(pa, scale(sub(pb, pa), s)), scale(bow, sag));
        }
        t.prev = t.points;
        t.phase = Phase::Connected;
        t.link_rest = length(sub(pb, pa)) * 0.92;
        t.rest = polyline_length(&t.points);
        t.open = 0.3;
        self.push_tendril(t)
    }

    /// Starts a bud on `a` reaching for `b`.
    pub fn grow(&mut self, a: usize, b: usize) -> Option<usize> {
        let pa = self.nodes[a].pos;
        let mut t = self.new_tendril(a, b);
        t.tip_dir = normalize(add(
            normalize(sub(self.nodes[b].pos, pa)),
            scale(self.rng.unit_vector(), 0.6),
        ));
        let tip = add(pa, scale(t.tip_dir, self.nodes[a].radius * 1.1));
        for (i, p) in t.points.iter_mut().enumerate() {
            *p = add(pa, scale(sub(tip, pa), i as f32 / (CHAIN - 1) as f32));
        }
        t.prev = t.points;
        t.rest = length(sub(tip, pa));
        self.push_tendril(t)
    }

    /// Fires node `n`: it flashes and sends impulses down all its connected tendrils.
    pub fn fire(&mut self, n: usize, band: usize, energy: f32) {
        self.fire_from(n, band, energy, None);
    }

    /// Snaps a connected tendril somewhere along its length. Both halves recoil and shrink
    /// back to their nodes, which are flung apart.
    pub fn sever(&mut self, t: usize) {
        if self.tendrils[t].phase != Phase::Connected {
            return;
        }
        let original = self.tendrils[t].clone();
        let at = 3 + self.rng.pick(CHAIN - 6);
        let near = &original.points[..=at];
        let mut far = original.points[at..].to_vec();
        far.reverse();
        let break_point = original.points[at];
        let half = |points: &[[f32; 3]], anchor: usize, stain: f32| {
            let mut half = original.clone();
            half.from = anchor;
            half.to = anchor;
            half.phase = Phase::Retracting;
            half.points = resample(points);
            // Give the loose end a kick back towards its node: the snap.
            half.prev = half.points;
            let n = CHAIN - 1;
            let back = normalize(sub(half.points[n - 1], half.points[n]));
            for i in CHAIN / 2..CHAIN {
                let k = (i - CHAIN / 2) as f32 / (CHAIN / 2) as f32;
                half.prev[i] = sub(half.points[i], scale(back, 0.06 * k));
            }
            half.rest = polyline_length(&half.points) * 0.6;
            half.open = 0.0;
            half.stain_from = stain;
            half.stain_to = stain;
            half
        };
        let a = half(near, original.from, original.stain_from);
        let b = half(&far, original.to, original.stain_to);
        self.pulses.retain(|p| p.tendril != t);
        self.tendrils[t] = a;
        self.push_tendril(b);

        // The spring is gone: the nodes recoil away from each other.
        let dir = normalize(sub(
            self.nodes[original.to].pos,
            self.nodes[original.from].pos,
        ));
        let kick = 0.6;
        let from = &mut self.nodes[original.from];
        from.vel = sub(from.vel, scale(dir, kick));
        let to = &mut self.nodes[original.to];
        to.vel = add(to.vel, scale(dir, kick));
        self.burst(break_point, 14, original.stain_from, 0.9);
    }

    /// Advances everything by `dt` seconds.
    pub fn step(&mut self, dt: f32, drive: Drive) {
        self.time += dt;
        let energy = drive.energy.clamp(0.0, 1.0);
        let onsets = self.detect_onsets(dt, &drive.bands);
        for (flash, onset) in self.beat_flash.iter_mut().zip(onsets) {
            *flash = (*flash - dt * 3.0).max(0.0);
            if let Some(strength) = onset {
                *flash = strength.min(1.0);
            }
        }
        self.move_camera(dt, energy);
        if self.autonomous {
            self.recycle_nodes();
        }
        self.update_nodes(dt);
        self.update_tendrils(dt, energy);
        self.update_pulses(dt);
        if self.autonomous {
            self.fire_on_onsets(&onsets);
            self.grow_some(dt, energy, onsets[0].is_some());
            self.sever_some(dt, energy, onsets[0]);
        }
        self.update_microbes(dt, energy, onsets[0].is_some());
        self.update_debris(dt);
        self.update_focus(dt);
    }

    /// Where a pulse is in the world.
    pub fn pulse_position(&self, pulse: &Pulse) -> [f32; 3] {
        point_along(&self.tendrils[pulse.tendril].points, pulse.s)
    }

    /// Brightness added to each rope point of tendril `t` by the pulses running along it.
    pub fn point_glow(&self, t: usize) -> [f32; CHAIN] {
        let tendril = &self.tendrils[t];
        let len = tendril.rest.max(0.1);
        let mut glow = [0.0; CHAIN];
        for pulse in self.pulses.iter().filter(|p| p.tendril == t) {
            for (i, g) in glow.iter_mut().enumerate() {
                let s = i as f32 / (CHAIN - 1) as f32;
                let d = (s - pulse.s) * len / 0.45;
                *g += pulse.energy * (-d * d).exp();
            }
        }
        if tendril.flash > 0.0 {
            // The contact flash spreads back from the tip.
            for (i, g) in glow.iter_mut().enumerate() {
                let s = i as f32 / (CHAIN - 1) as f32;
                *g += tendril.flash * s.powi(3);
            }
        }
        glow
    }

    /// The growth cone of tendril `t`, if it has one: position, facing and how open it is.
    /// Growing tendrils carry it at the tip; connected ones keep a small bouton on the
    /// target's surface.
    pub fn flower(&self, t: usize) -> Option<([f32; 3], [f32; 3], f32)> {
        let tendril = &self.tendrils[t];
        match tendril.phase {
            Phase::Growing => Some((tendril.points[CHAIN - 1], tendril.tip_dir, tendril.open)),
            Phase::Connected => {
                let target = &self.nodes[tendril.to];
                let out = normalize(sub(tendril.points[CHAIN - 3], target.pos));
                let pos = add(target.pos, scale(out, target.radius * 0.95));
                Some((pos, scale(out, -1.0), tendril.open))
            }
            Phase::Retracting => None,
        }
    }

    fn new_node(&mut self, pos: [f32; 3]) -> Node {
        let rng = &mut self.rng;
        Node {
            pos,
            vel: [0.0; 3],
            radius: rng.uniform(0.22, 0.42),
            stain: if rng.next_f32() < 0.6 {
                rng.uniform(0.0, 0.25)
            } else {
                rng.uniform(0.75, 1.0)
            },
            glow: 0.0,
            glow_band: 0,
            phase: rng.uniform(0.0, std::f32::consts::TAU),
            seed: rng.next_f32(),
            fade: 0.0,
            max_degree: 2 + rng.pick(3),
            refractory: 0.0,
        }
    }

    fn new_tendril(&mut self, a: usize, b: usize) -> Tendril {
        let pa = self.nodes[a].pos;
        Tendril {
            from: a,
            to: b,
            phase: Phase::Growing,
            points: [pa; CHAIN],
            prev: [pa; CHAIN],
            rest: 0.0,
            link_rest: 0.0,
            tip_dir: [0.0, 0.0, -1.0],
            open: 0.0,
            flash: 0.0,
            thickness: self.rng.uniform(0.03, 0.065),
            age: 0.0,
            seed: self.rng.next_f32(),
            stain_from: self.nodes[a].stain,
            stain_to: self.nodes[b].stain,
        }
    }

    fn push_tendril(&mut self, t: Tendril) -> Option<usize> {
        if self.tendrils.len() >= MAX_TENDRILS {
            return None;
        }
        self.tendrils.push(t);
        Some(self.tendrils.len() - 1)
    }

    fn remove_tendril(&mut self, t: usize) {
        let last = self.tendrils.len() - 1;
        self.pulses.retain(|p| p.tendril != t);
        for p in &mut self.pulses {
            if p.tendril == last {
                p.tendril = t;
            }
        }
        self.tendrils.swap_remove(t);
    }

    fn linked(&self, a: usize, b: usize) -> bool {
        self.tendrils.iter().any(|t| {
            t.phase != Phase::Retracting
                && ((t.from == a && t.to == b) || (t.from == b && t.to == a))
        })
    }

    fn degree(&self, n: usize) -> usize {
        self.tendrils
            .iter()
            .filter(|t| t.phase != Phase::Retracting && (t.from == n || t.to == n))
            .count()
    }

    /// A random reachable, unlinked neighbour with a free connection.
    fn pick_target(&mut self, a: usize) -> Option<usize> {
        if self.degree(a) >= self.nodes[a].max_degree {
            return None;
        }
        let pa = self.nodes[a].pos;
        let candidates: Vec<usize> = (0..self.nodes.len())
            .filter(|&b| {
                let d = length(sub(self.nodes[b].pos, pa));
                b != a
                    && (LINK_MIN..LINK_MAX).contains(&d)
                    && !self.linked(a, b)
                    && self.degree(b) < self.nodes[b].max_degree + 1
            })
            .collect();
        if candidates.is_empty() {
            return None;
        }
        // Favour near neighbours.
        let mut best = candidates[self.rng.pick(candidates.len())];
        for _ in 0..2 {
            let other = candidates[self.rng.pick(candidates.len())];
            if length(sub(self.nodes[other].pos, pa)) < length(sub(self.nodes[best].pos, pa)) {
                best = other;
            }
        }
        Some(best)
    }

    fn spawn_position(&mut self, near: f32, far: f32) -> [f32; 3] {
        let cam = self.camera;
        let dist = self.rng.uniform(near, far);
        let spread = 1.0 + 0.25 * dist;
        let (x, y) = (
            self.rng.uniform(-spread, spread),
            self.rng.uniform(-spread, spread) * 0.8,
        );
        add(
            add(cam.pos, scale(cam.forward, dist)),
            add(scale(cam.right, x), scale(cam.up, y)),
        )
    }

    /// Puts a new node in `slot`, letting go of every tendril the old one had.
    fn replace_node(&mut self, slot: usize, node: Node) {
        let mut t = 0;
        while t < self.tendrils.len() {
            let tendril = &self.tendrils[t];
            let (from, to, phase) = (tendril.from == slot, tendril.to == slot, tendril.phase);
            if from && phase != Phase::Connected {
                self.remove_tendril(t);
                continue;
            }
            if from || to {
                self.pulses.retain(|p| p.tendril != t);
                let tendril = &mut self.tendrils[t];
                if from {
                    // Re-anchor a connection on its surviving end.
                    tendril.points.reverse();
                    tendril.prev.reverse();
                    tendril.from = tendril.to;
                    tendril.stain_from = tendril.stain_to;
                }
                tendril.phase = Phase::Retracting;
                tendril.to = tendril.from;
                tendril.open = 0.0;
            }
            t += 1;
        }
        self.nodes[slot] = node;
    }

    fn recycle_nodes(&mut self) {
        let cam = self.camera;
        for n in 0..self.nodes.len() {
            let rel = sub(self.nodes[n].pos, cam.pos);
            let ahead = dot(rel, cam.forward);
            let side = length(sub(rel, scale(cam.forward, ahead)));
            if ahead < -1.5 || side > 2.0 + 0.4 * ahead.max(0.0) + 4.0 {
                let pos = self.spawn_position(FAR_SPAWN * 0.75, FAR_SPAWN);
                let node = self.new_node(pos);
                self.replace_node(n, node);
            }
        }
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
            if hit {
                self.cooldown[b] = COOLDOWN[b];
                Some(self.kick[b].min(1.5))
            } else {
                None
            }
        })
    }

    fn move_camera(&mut self, dt: f32, energy: f32) {
        let t = self.time;
        let yaw = 0.4 * (t * 0.027).sin() + 0.2 * (t * 0.071).sin();
        let pitch = 0.25 * (t * 0.033 + 2.0).sin();
        // A slow, continuous roll, quicker when the music is loud.
        self.roll += dt * (0.05 + 0.15 * energy);
        let forward = [
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            -yaw.cos() * pitch.cos(),
        ];
        let flat_right = normalize(cross(forward, [0.0, 1.0, 0.0]));
        let flat_up = cross(flat_right, forward);
        let (sin, cos) = self.roll.sin_cos();
        let speed = 0.35 + 0.9 * energy + 0.5 * self.kick[0].min(1.0);
        self.camera = Camera {
            pos: add(self.camera.pos, scale(forward, speed * dt)),
            forward,
            right: add(scale(flat_right, cos), scale(flat_up, sin)),
            up: sub(scale(flat_up, cos), scale(flat_right, sin)),
        };
    }

    fn update_nodes(&mut self, dt: f32) {
        let n = self.nodes.len();
        let mut force = vec![[0.0f32; 3]; n];
        for t in self.tendrils.iter().filter(|t| t.phase == Phase::Connected) {
            let d = sub(self.nodes[t.to].pos, self.nodes[t.from].pos);
            let dist = length(d).max(1e-4);
            let f = scale(d, SPRING * (dist - t.link_rest) / dist);
            force[t.from] = add(force[t.from], f);
            force[t.to] = sub(force[t.to], f);
        }
        // Cells keep a little personal space.
        for i in 0..n {
            for j in i + 1..n {
                let d = sub(self.nodes[j].pos, self.nodes[i].pos);
                let dist = length(d).max(1e-4);
                let room = self.nodes[i].radius + self.nodes[j].radius + 0.6;
                if dist < room {
                    let f = scale(d, 1.5 * (room - dist) / dist);
                    force[i] = sub(force[i], f);
                    force[j] = add(force[j], f);
                }
            }
        }
        let t = self.time;
        let damping = (-NODE_DAMPING * dt).exp();
        for (node, f) in self.nodes.iter_mut().zip(force) {
            let s = node.seed * 50.0;
            let drift = [
                (t * 0.21 + s).sin(),
                (t * 0.17 + s * 1.3).sin(),
                (t * 0.19 + s * 1.7).sin(),
            ];
            node.vel = add(node.vel, scale(add(f, scale(drift, 0.04)), dt));
            node.vel = scale(node.vel, damping);
            node.pos = add(node.pos, scale(node.vel, dt));
            node.glow *= (-3.0 * dt).exp();
            node.refractory -= dt;
            node.phase += dt * (0.6 + 3.0 * node.glow);
            node.fade = (node.fade + dt / 2.0).min(1.0);
        }
    }

    fn update_tendrils(&mut self, dt: f32, energy: f32) {
        let mut t = 0;
        while t < self.tendrils.len() {
            if self.update_tendril(t, dt, energy) {
                t += 1;
            } else {
                self.remove_tendril(t);
            }
        }
    }

    /// Returns false once the tendril has fully retracted.
    fn update_tendril(&mut self, t: usize, dt: f32, energy: f32) -> bool {
        let time = self.time;
        let tendril = &mut self.tendrils[t];
        tendril.age += dt;
        tendril.flash = (tendril.flash - dt * 1.5).max(0.0);
        let root = self.nodes[tendril.from].pos;
        let mut tip_pin = None;
        let mut contact = None;
        match tendril.phase {
            Phase::Growing => {
                let target = self.nodes[tendril.to];
                let tip = tendril.points[CHAIN - 1];
                let to_target = sub(target.pos, tip);
                let dist = length(to_target).max(1e-4);
                let s = tendril.seed * 40.0;
                let wander = [
                    (time * 1.3 + s).sin(),
                    (time * 1.1 + s * 1.7).sin(),
                    (time * 0.9 + s * 2.3).sin(),
                ];
                let seek = 1.5 + 4.0 * smoothstep(3.0, 0.5, dist);
                let steer = add(scale(to_target, seek / dist), scale(wander, 1.2));
                tendril.tip_dir = normalize(add(tendril.tip_dir, scale(steer, dt)));
                let speed = 0.45 + 1.6 * energy;
                let tip = add(tip, scale(tendril.tip_dir, speed * dt));
                tendril.open = smoothstep(1.8, target.radius + 0.25, dist).max(tendril.open);
                tendril.rest = length(sub(tip, root)) * 1.15;
                tip_pin = Some(tip);
                if dist < target.radius * 0.85 + 0.05 {
                    tendril.phase = Phase::Connected;
                    tendril.link_rest = length(sub(target.pos, root)) * 0.92;
                    tendril.rest = polyline_length(&tendril.points);
                    tendril.flash = 1.0;
                    tip_pin = Some(target.pos);
                    contact = Some(tip);
                } else if tendril.age > GROW_TIMEOUT {
                    tendril.phase = Phase::Retracting;
                    tendril.to = tendril.from;
                    tendril.open = 0.0;
                }
            }
            Phase::Connected => {
                tendril.open += (0.3 - tendril.open) * (dt * 0.6).min(1.0);
                let target = self.nodes[tendril.to].pos;
                // Stretches taut when the nodes pull apart, sags a little when they close in.
                let span = length(sub(target, root));
                tendril.rest = (tendril.link_rest * 1.12).max(span * 1.01);
                tip_pin = Some(target);
            }
            Phase::Retracting => {
                tendril.rest *= (-1.3 * dt).exp();
                tendril.rest -= dt * 0.08;
                if tendril.rest < 0.05 {
                    return false;
                }
            }
        }

        // Verlet rope: inertia plus a gentle writhing, then length constraints.
        let s = tendril.seed * 30.0;
        let damping = 0.92;
        for i in 1..CHAIN {
            let p = tendril.points[i];
            let v = scale(sub(p, tendril.prev[i]), damping);
            let k = i as f32;
            let writhe = [
                (time * 1.7 + k * 0.9 + s).sin(),
                (time * 1.3 + k * 1.1 + s * 1.3).sin(),
                (time * 1.5 + k * 0.7 + s * 1.9).sin(),
            ];
            tendril.prev[i] = p;
            tendril.points[i] = add(add(p, v), scale(writhe, 0.25 * dt * dt));
        }
        let segment = tendril.rest / (CHAIN - 1) as f32;
        for _ in 0..8 {
            tendril.points[0] = root;
            if let Some(tip) = tip_pin {
                tendril.points[CHAIN - 1] = tip;
            }
            for i in 0..CHAIN - 1 {
                let (a, b) = (tendril.points[i], tendril.points[i + 1]);
                let d = sub(b, a);
                let dist = length(d).max(1e-6);
                let pinned_a = i == 0;
                let pinned_b = i + 1 == CHAIN - 1 && tip_pin.is_some();
                let correction = scale(d, (dist - segment) / dist);
                match (pinned_a, pinned_b) {
                    (true, true) => {}
                    (true, false) => tendril.points[i + 1] = sub(b, correction),
                    (false, true) => tendril.points[i] = add(a, correction),
                    (false, false) => {
                        tendril.points[i] = add(a, scale(correction, 0.5));
                        tendril.points[i + 1] = sub(b, scale(correction, 0.5));
                    }
                }
            }
        }
        tendril.points[0] = root;
        if let Some(tip) = tip_pin {
            tendril.points[CHAIN - 1] = tip;
        }

        if let Some(at) = contact {
            let (to, stain) = (tendril.to, tendril.stain_from);
            // Contact: a puff of transmitter and the target lights up.
            self.burst(at, 10, stain, 0.5);
            self.nodes[to].glow = self.nodes[to].glow.max(0.8);
        }
        true
    }

    fn fire_from(&mut self, n: usize, band: usize, energy: f32, except: Option<usize>) {
        let node = &mut self.nodes[n];
        if node.refractory > 0.0 {
            return;
        }
        node.refractory = REFRACTORY;
        node.glow = node.glow.max(energy.min(1.0));
        node.glow_band = band;
        for (t, tendril) in self.tendrils.iter().enumerate() {
            if tendril.phase != Phase::Connected || Some(t) == except {
                continue;
            }
            let forward = tendril.from == n;
            if !forward && tendril.to != n {
                continue;
            }
            if self.pulses.len() >= MAX_PULSES {
                break;
            }
            self.pulses.push(Pulse {
                tendril: t,
                s: if forward { 0.0 } else { 1.0 },
                forward,
                speed: PULSE_SPEED[band] * self.rng.uniform(0.85, 1.15),
                band,
                energy,
            });
        }
    }

    fn update_pulses(&mut self, dt: f32) {
        let mut arrivals = Vec::new();
        let tendrils = &self.tendrils;
        self.pulses.retain_mut(|p| {
            let tendril = &tendrils[p.tendril];
            if tendril.phase != Phase::Connected {
                return false;
            }
            let ds = p.speed * dt / tendril.rest.max(0.1);
            p.s += if p.forward { ds } else { -ds };
            if (0.0..=1.0).contains(&p.s) {
                return true;
            }
            let node = if p.forward { tendril.to } else { tendril.from };
            arrivals.push((node, p.band, p.energy * PULSE_DECAY, p.tendril));
            false
        });
        for (node, band, energy, via) in arrivals {
            if energy >= PULSE_MIN {
                self.fire_from(node, band, energy, Some(via));
            } else {
                let glow = &mut self.nodes[node].glow;
                *glow = glow.max(energy);
            }
        }
    }

    /// Each band onset fires a few nodes in view: bass several at full strength, treble
    /// one lightly.
    fn fire_on_onsets(&mut self, onsets: &[Option<f32>; BANDS]) {
        if self.nodes.is_empty() {
            return;
        }
        for (band, onset) in onsets.iter().enumerate() {
            let Some(strength) = *onset else { continue };
            let count = [3, 2, 2, 1][band];
            let energy = [1.0, 0.85, 0.7, 0.55][band] * strength.clamp(0.5, 1.0);
            for _ in 0..count {
                let n = self.visible_node();
                self.fire_from(n, band, energy, None);
            }
        }
    }

    /// A random node, preferring ones near the middle of the view.
    fn visible_node(&mut self) -> usize {
        let cam = self.camera;
        let mut best = self.rng.pick(self.nodes.len());
        let score = |n: &Node| {
            let rel = sub(n.pos, cam.pos);
            let ahead = dot(rel, cam.forward);
            if ahead < 1.0 {
                return f32::MAX;
            }
            length(sub(rel, scale(cam.forward, ahead))) / ahead + (ahead / FAR_SPAWN) * 0.3
        };
        for _ in 0..3 {
            let other = self.rng.pick(self.nodes.len());
            if score(&self.nodes[other]) < score(&self.nodes[best]) {
                best = other;
            }
        }
        best
    }

    fn grow_some(&mut self, dt: f32, energy: f32, beat: bool) {
        self.grow_debt += dt * (0.8 + 2.5 * energy) + if beat { 0.4 } else { 0.0 };
        while self.grow_debt >= 1.0 {
            self.grow_debt -= 1.0;
            let growing = self
                .tendrils
                .iter()
                .filter(|t| t.phase == Phase::Growing)
                .count();
            if growing >= MAX_GROWING {
                break;
            }
            // Try a few nodes: many are already fully wired.
            for _ in 0..6 {
                let a = self.visible_node();
                if let Some(b) = self.pick_target(a) {
                    self.grow(a, b);
                    break;
                }
            }
        }
    }

    /// Now and then, and more on heavy bass hits, a connection snaps.
    fn sever_some(&mut self, dt: f32, energy: f32, bass: Option<f32>) {
        self.sever_debt += dt * (0.08 + 0.25 * energy);
        if bass.is_some_and(|s| s > 1.0) && self.rng.next_f32() < 0.1 {
            self.sever_debt += 1.0;
        }
        if self.sever_debt < 1.0 {
            return;
        }
        self.sever_debt = 0.0;
        let connected: Vec<usize> = (0..self.tendrils.len())
            .filter(|&t| self.tendrils[t].phase == Phase::Connected && self.tendrils[t].age > 4.0)
            .collect();
        if connected.is_empty() {
            return;
        }
        // Prefer the most stretched.
        let mut pick = connected[self.rng.pick(connected.len())];
        for _ in 0..2 {
            let other = connected[self.rng.pick(connected.len())];
            if self.strain(other) > self.strain(pick) {
                pick = other;
            }
        }
        self.sever(pick);
    }

    fn strain(&self, t: usize) -> f32 {
        let tendril = &self.tendrils[t];
        let span = length(sub(
            self.nodes[tendril.to].pos,
            self.nodes[tendril.from].pos,
        ));
        span / tendril.link_rest.max(0.1)
    }

    fn new_microbe(&mut self, near: f32, far: f32) -> Microbe {
        let pos = self.spawn_position(near, far);
        let rng = &mut self.rng;
        let kind = MICROBE_KINDS[rng.pick(MICROBE_KINDS.len())];
        let size = match kind {
            MicrobeKind::Vesicle => rng.uniform(0.06, 0.14),
            MicrobeKind::Mitochondrion => rng.uniform(0.1, 0.18),
            MicrobeKind::Bacillus => rng.uniform(0.08, 0.14),
            MicrobeKind::Coccus => rng.uniform(0.06, 0.09),
            MicrobeKind::Spirillum => rng.uniform(0.1, 0.16),
            MicrobeKind::Vibrio => rng.uniform(0.08, 0.12),
            MicrobeKind::Amoeba => rng.uniform(0.18, 0.32),
            MicrobeKind::Lipid => rng.uniform(0.04, 0.1),
        };
        // Muted, slightly varied tints: these are unstained bystanders.
        let base: [f32; 3] = match kind {
            MicrobeKind::Mitochondrion => [1.0, 0.55, 0.3],
            MicrobeKind::Lipid => [1.0, 0.9, 0.55],
            MicrobeKind::Vesicle => [0.6, 0.8, 1.0],
            MicrobeKind::Amoeba => [0.55, 1.0, 0.8],
            _ => [0.75, 0.85, 0.7],
        };
        let jitter = [
            rng.uniform(0.8, 1.1),
            rng.uniform(0.8, 1.1),
            rng.uniform(0.8, 1.1),
        ];
        Microbe {
            kind,
            pos,
            vel: [0.0; 3],
            axis: rng.unit_vector(),
            spin_axis: rng.unit_vector(),
            spin: rng.uniform(-0.6, 0.6),
            size,
            phase: rng.uniform(0.0, std::f32::consts::TAU),
            tint: [
                base[0] * jitter[0],
                base[1] * jitter[1],
                base[2] * jitter[2],
            ],
            seed: rng.next_f32(),
            swim_speed: rng.uniform(0.15, 0.5),
            run_left: rng.uniform(0.5, 3.0),
        }
    }

    fn update_microbes(&mut self, dt: f32, energy: f32, bass: bool) {
        let cam = self.camera;
        for i in 0..self.microbes.len() {
            let rel = sub(self.microbes[i].pos, cam.pos);
            let ahead = dot(rel, cam.forward);
            let side = length(sub(rel, scale(cam.forward, ahead)));
            if self.autonomous && (ahead < -0.5 || side > 3.0 + 0.4 * ahead.max(0.0)) {
                self.microbes[i] = self.new_microbe(FAR_SPAWN * 0.6, FAR_SPAWN);
                continue;
            }
            let rng = &mut self.rng;
            let m = &mut self.microbes[i];
            if m.kind.swims() {
                // Run and tumble, like E. coli hunting a scent; bass hits startle them.
                m.run_left -= dt;
                if m.run_left <= 0.0 || (bass && rng.next_f32() < 0.2) {
                    m.axis = normalize(add(m.axis, scale(rng.unit_vector(), 1.4)));
                    m.run_left = rng.uniform(1.0, 4.0);
                }
                let target = scale(m.axis, m.swim_speed * (0.7 + energy));
                m.vel = add(m.vel, scale(sub(target, m.vel), (dt * 2.0).min(1.0)));
                m.phase += dt * (10.0 + 8.0 * energy);
            } else {
                let jiggle = scale(rng.unit_vector(), 0.15 * dt);
                m.vel = scale(add(m.vel, jiggle), (-0.8 * dt).exp());
                m.axis = normalize(rotate(m.axis, m.spin_axis, m.spin * dt));
                m.phase += dt * (1.0 + energy);
            }
            m.pos = add(m.pos, scale(m.vel, dt));
        }
    }

    fn burst(&mut self, at: [f32; 3], count: usize, stain: f32, speed: f32) {
        for _ in 0..count {
            if self.debris.len() >= MAX_DEBRIS {
                self.debris.remove(0);
            }
            let rng = &mut self.rng;
            self.debris.push(Debris {
                pos: at,
                vel: scale(rng.unit_vector(), speed * rng.uniform(0.3, 1.0)),
                age: 0.0,
                life: rng.uniform(1.0, 2.5),
                size: rng.uniform(0.015, 0.04),
                stain,
            });
        }
    }

    fn update_debris(&mut self, dt: f32) {
        let drag = (-2.0 * dt).exp();
        for d in &mut self.debris {
            d.age += dt;
            d.vel = scale(d.vel, drag);
            d.pos = add(d.pos, scale(d.vel, dt));
        }
        self.debris.retain(|d| d.age < d.life);
    }

    /// Autofocus: every few seconds it hunts for the cell nearest the middle of the view
    /// and racks focus onto it.
    fn update_focus(&mut self, dt: f32) {
        self.focus_timer -= dt;
        if self.focus_timer <= 0.0 && !self.nodes.is_empty() {
            self.focus_timer = self.rng.uniform(4.0, 9.0);
            let cam = self.camera;
            let subject = self
                .nodes
                .iter()
                .filter_map(|n| {
                    let rel = sub(n.pos, cam.pos);
                    let ahead = dot(rel, cam.forward);
                    (2.5..14.0).contains(&ahead).then(|| {
                        let off = length(sub(rel, scale(cam.forward, ahead))) / ahead;
                        (off, ahead)
                    })
                })
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((_, depth)) = subject {
                self.focus_target = depth;
            }
        }
        // Keep the subject sharp as the camera closes in on it.
        let closing = 0.35 * dt;
        self.focus_target = (self.focus_target - closing).max(2.0);
        self.focus += (self.focus_target - self.focus) * (dt * 1.2).min(1.0);
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn polyline_length(points: &[[f32; 3]]) -> f32 {
    points.windows(2).map(|w| length(sub(w[1], w[0]))).sum()
}

/// The point a fraction `s` of the way along a polyline, by arc length.
fn point_along(points: &[[f32; 3]], s: f32) -> [f32; 3] {
    let total = polyline_length(points);
    let mut left = s.clamp(0.0, 1.0) * total;
    for w in points.windows(2) {
        let seg = length(sub(w[1], w[0]));
        if left <= seg && seg > 0.0 {
            return add(w[0], scale(sub(w[1], w[0]), left / seg));
        }
        left -= seg;
    }
    points[points.len() - 1]
}

/// Resamples a polyline to [`CHAIN`] evenly spaced points.
fn resample(points: &[[f32; 3]]) -> [[f32; 3]; CHAIN] {
    std::array::from_fn(|i| point_along(points, i as f32 / (CHAIN - 1) as f32))
}

//! A field of 2D cellular automata hanging in 3D space. Kept free of GPU types so the rules can
//! be unit-tested; `render::modes::automata` draws the result.
//!
//! Each automaton is a square grid on its own tilted plane, running one rule from [`RULES`]:
//! Wolfram elementary rules (drawn as their space-time diagram, one row per generation),
//! life-like and "Generations" rules in B/S notation, and cyclic automata. Rules are tagged
//! with their Wolfram class (1 fixed, 2 periodic, 3 chaotic, 4 complex).
//!
//! Automata grow outwards from a seed: cells may only live inside a disc (a band for
//! elementary rules) whose radius widens on every beat. The rate of generations follows the
//! detected tempo, so the whole field breathes with the music. Where two planes cross, live
//! cells on one seed cells on the other along the line of intersection, carrying their own
//! palette across, so colonies of one automaton bloom inside another.
//!
//! The camera drifts forward through the field; automata that fall behind it, die out or
//! reach the end of their life fade and are reborn ahead with a new rule, angle, size and
//! palette. A handful of [`ScreenWarp`]s (lenses, swirls, shear bands and beat shockwaves)
//! traverse the screen on top.

/// Side of the largest grid; every automaton's cells fit in a `GRID`×`GRID` texture layer.
pub const GRID: usize = 128;
/// Grids are stored with a one-cell dead border so neighbour lookups need no bounds checks.
const STRIDE: usize = GRID + 2;
pub const MAX_AUTOMATA: usize = 36;
pub const MAX_WARPS: usize = 8;
/// Grid sizes to choose from: with plane sizes this sets each automaton's magnification.
pub const DIMS: [usize; 4] = [40, 64, 96, 128];
/// Automata live between these distances ahead of the camera.
pub const NEAR_SPAWN: f32 = 4.0;
pub const FAR_SPAWN: f32 = 30.0;
/// Beats per second assumed when no beat has been heard for a while.
const IDLE_BPS: f32 = 0.6;
/// At most this many generations per automaton per frame, to bound CPU time.
const MAX_GENERATIONS_PER_FRAME: u32 = 4;
/// Plane-crossing interactions run at this rate (per second).
const INTERACT_HZ: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// Outer-totalistic Moore-neighbourhood rule in B/S notation (bit `n` set = `n` live
    /// neighbours). `states > 2` makes it a "Generations" rule: a cell that fails to survive
    /// lingers through `states - 2` dying states before it is dead.
    LifeLike {
        birth: u16,
        survive: u16,
        states: u8,
    },
    /// Wolfram elementary rule, grown as its space-time diagram: one new row per generation.
    Elementary { rule: u8 },
    /// Cyclic automaton: a cell advances to the next of `states` colours once at least
    /// `threshold` of its Moore neighbours already show that colour.
    Cyclic { states: u8, threshold: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rule {
    pub name: &'static str,
    /// Wolfram class, 1–4.
    pub class: u8,
    pub kind: Kind,
    /// Chance that a newly unlocked cell starts alive. For elementary rules, the density of
    /// the first row, with 0 meaning a single seed cell.
    pub density: f32,
    /// Generations per beat.
    pub speed: f32,
}

const fn bits(digits: &[u8]) -> u16 {
    let mut mask = 0;
    let mut i = 0;
    while i < digits.len() {
        mask |= 1 << digits[i];
        i += 1;
    }
    mask
}

const fn life(name: &'static str, class: u8, birth: &[u8], survive: &[u8], density: f32) -> Rule {
    generations(name, class, birth, survive, 2, density)
}

const fn generations(
    name: &'static str,
    class: u8,
    birth: &[u8],
    survive: &[u8],
    states: u8,
    density: f32,
) -> Rule {
    Rule {
        name,
        class,
        kind: Kind::LifeLike {
            birth: bits(birth),
            survive: bits(survive),
            states,
        },
        density,
        speed: 2.0,
    }
}

const fn elementary(name: &'static str, rule: u8, class: u8, density: f32) -> Rule {
    Rule {
        name,
        class,
        kind: Kind::Elementary { rule },
        density,
        speed: 4.0,
    }
}

const fn cyclic(name: &'static str, class: u8, states: u8, threshold: u8) -> Rule {
    Rule {
        name,
        class,
        kind: Kind::Cyclic { states, threshold },
        density: 1.0,
        speed: 2.0,
    }
}

pub const RULES: &[Rule] = &[
    // Wolfram's elementary rules, one or more per class.
    elementary("Rule 160", 160, 1, 0.6),
    elementary("Rule 168", 168, 1, 0.55),
    elementary("Rule 108", 108, 2, 0.5),
    elementary("Rule 184", 184, 2, 0.45),
    elementary("Rule 73", 73, 2, 0.5),
    elementary("Rule 30", 30, 3, 0.0),
    elementary("Rule 45", 45, 3, 0.0),
    elementary("Rule 90", 90, 3, 0.0),
    elementary("Rule 105", 105, 3, 0.0),
    elementary("Rule 150", 150, 3, 0.0),
    elementary("Rule 18", 18, 3, 0.0),
    elementary("Rule 110", 110, 4, 0.5),
    elementary("Rule 54", 54, 4, 0.4),
    elementary("Rule 137", 137, 4, 0.5),
    // Life-like rules, by the class their behaviour resembles.
    life("Life", 4, &[3], &[2, 3], 0.3),
    life("HighLife", 4, &[3, 6], &[2, 3], 0.3),
    life("Day & Night", 4, &[3, 6, 7, 8], &[3, 4, 6, 7, 8], 0.5),
    life("Morley", 4, &[3, 6, 8], &[2, 4, 5], 0.3),
    life("Seeds", 3, &[2], &[], 0.015),
    life("Replicator", 3, &[1, 3, 5, 7], &[1, 3, 5, 7], 0.004),
    life("Amoeba", 3, &[3, 5, 7], &[1, 3, 5, 8], 0.35),
    life("Gnarl", 3, &[1], &[1], 0.003),
    life("Maze", 2, &[3], &[1, 2, 3, 4, 5], 0.12),
    life("Coral", 2, &[3], &[4, 5, 6, 7, 8], 0.2),
    life("Diamoeba", 2, &[3, 5, 6, 7, 8], &[5, 6, 7, 8], 0.48),
    life("Anneal", 2, &[4, 6, 7, 8], &[3, 5, 6, 7, 8], 0.5),
    life("2x2", 2, &[3, 6], &[1, 2, 5], 0.3),
    life("Inkspot", 2, &[3], &[0, 1, 2, 3, 4, 5, 6, 7, 8], 0.05),
    generations("Brian's Brain", 3, &[2], &[], 3, 0.04),
    generations("Star Wars", 4, &[2], &[3, 4, 5], 4, 0.1),
    generations("Frogs", 3, &[3, 4], &[1, 2], 3, 0.2),
    cyclic("Cyclic 313", 4, 3, 3),
    cyclic("Cyclic waves", 2, 14, 1),
];

pub fn rule_index(name: &str) -> Option<usize> {
    RULES.iter().position(|rule| rule.name == name)
}

/// Colour schemes: each automaton draws its cells as tones between three colours.
pub const PALETTES: &[[[f32; 3]; 3]] = &[
    [[1.0, 0.22, 0.04], [1.0, 0.6, 0.1], [1.0, 0.95, 0.6]], // ember
    [[0.08, 0.25, 1.0], [0.3, 0.8, 1.0], [0.85, 0.95, 1.0]], // ice
    [[0.15, 0.75, 0.1], [0.7, 1.0, 0.2], [0.05, 0.45, 0.3]], // toxic
    [[0.55, 0.1, 0.9], [1.0, 0.3, 0.7], [0.9, 0.7, 1.0]],   // orchid
    [[0.0, 0.7, 0.7], [1.0, 0.45, 0.35], [1.0, 0.85, 0.7]], // reef
    [[0.8, 0.45, 0.05], [1.0, 0.85, 0.3], [0.55, 0.2, 0.05]], // gold
    [[1.0, 0.2, 0.6], [0.2, 0.9, 1.0], [0.6, 0.3, 1.0]],    // vapor
    [[0.7, 0.02, 0.1], [1.0, 0.3, 0.2], [0.35, 0.0, 0.25]], // blood moon
    [[0.1, 1.0, 0.6], [0.2, 0.5, 1.0], [0.7, 0.2, 1.0]],    // aurora
    [[0.95, 0.35, 0.2], [0.95, 0.75, 0.55], [0.3, 0.15, 0.6]], // dusk
    [[0.6, 0.9, 1.0], [0.95, 0.95, 1.0], [0.4, 0.5, 0.9]],  // frost
    [[0.2, 1.0, 0.85], [0.0, 0.5, 0.45], [0.8, 1.0, 0.95]], // jade
];

const NEIGHBOURS: [isize; 8] = {
    let s = STRIDE as isize;
    [-s - 1, -s, -s + 1, -1, 1, s - 1, s, s + 1]
};

fn idx(x: usize, y: usize) -> usize {
    (y + 1) * STRIDE + x + 1
}

/// xorshift64: small, fast and good enough for visuals.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    pub fn next_f32(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn uniform(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next_f32()
    }

    pub fn pick(&mut self, n: usize) -> usize {
        ((self.next_f32() * n as f32) as usize).min(n - 1)
    }

    fn unit_vector(&mut self) -> [f32; 3] {
        let z = self.uniform(-1.0, 1.0);
        let angle = self.uniform(0.0, std::f32::consts::TAU);
        let r = (1.0 - z * z).max(0.0).sqrt();
        [r * angle.cos(), r * angle.sin(), z]
    }
}

pub struct Automaton {
    pub rule: usize,
    /// Cells per side actually used (≤ [`GRID`]).
    pub dim: usize,
    state: Vec<u8>,
    /// Generations since the cell was born, saturating.
    age: Vec<u8>,
    /// 0 for the automaton's own cells, otherwise 1 + the slot of the automaton whose cells
    /// seeded this one, so it is drawn in that automaton's palette.
    owner: Vec<u8>,
    next: Vec<u8>,
    next_owner: Vec<u8>,
    /// Cells may only live within this distance (in cells) of the centre: the growth front.
    pub radius: f32,
    pub generation: u32,
    /// Next row to write, for elementary rules.
    row: usize,
    /// Live cells (life-like, elementary) or cells that changed (cyclic) last generation.
    pub activity: usize,
    dirty: bool,

    pub center: [f32; 3],
    /// In-plane unit axes; the grid spans `center ± axis * half_size`.
    pub axis_u: [f32; 3],
    pub axis_v: [f32; 3],
    pub half_size: f32,
    spin_axis: [f32; 3],
    spin_rate: f32,
    /// How the plane is bent out of flat: bend, twist, ripple amplitude, ripple frequency.
    pub warp: [f32; 4],
    pub palette: [[f32; 3]; 3],
    /// Cell shape: 0 rounded square, 1 dot, 2 diamond, 3 ring.
    pub shape: u8,
    pub seed: f32,
    /// Per-automaton multiplier on its rule's speed.
    pub speed: f32,
    gen_debt: f32,
    pub alive_for: f32,
    lifetime: f32,
    idle_for: f32,
    /// Visibility, 0..1: fades in after birth and out before rebirth.
    pub fade: f32,
    dying: bool,
    /// Brightens the growth front for a moment after each beat.
    pub front_pulse: f32,
}

impl Automaton {
    /// An empty automaton facing +z at the origin, with every cell unlocked. For tests and as
    /// the starting point for [`AutomataSim`]'s spawns.
    pub fn blank(rule: usize, dim: usize) -> Self {
        let dim = dim.clamp(4, GRID);
        Self {
            rule,
            dim,
            state: vec![0; STRIDE * STRIDE],
            age: vec![0; STRIDE * STRIDE],
            owner: vec![0; STRIDE * STRIDE],
            next: vec![0; STRIDE * STRIDE],
            next_owner: vec![0; STRIDE * STRIDE],
            radius: max_radius(&RULES[rule], dim),
            generation: 0,
            row: 0,
            activity: 0,
            dirty: true,
            center: [0.0; 3],
            axis_u: [1.0, 0.0, 0.0],
            axis_v: [0.0, 1.0, 0.0],
            half_size: 1.0,
            spin_axis: [0.0, 1.0, 0.0],
            spin_rate: 0.0,
            warp: [0.0; 4],
            palette: PALETTES[0],
            shape: 0,
            seed: 0.0,
            speed: 1.0,
            gen_debt: 0.0,
            alive_for: 0.0,
            lifetime: 40.0,
            idle_for: 0.0,
            fade: 1.0,
            dying: false,
            front_pulse: 0.0,
        }
    }

    pub fn rule(&self) -> &'static Rule {
        &RULES[self.rule]
    }

    pub fn state(&self, x: usize, y: usize) -> u8 {
        self.state[idx(x, y)]
    }

    pub fn owner(&self, x: usize, y: usize) -> u8 {
        self.owner[idx(x, y)]
    }

    pub fn set(&mut self, x: usize, y: usize, state: u8) {
        let i = idx(x, y);
        self.state[i] = state;
        self.age[i] = 0;
        self.dirty = true;
        // An elementary automaton grows from its last written row.
        if matches!(self.rule().kind, Kind::Elementary { .. }) {
            self.row = self.row.max(y + 1);
        }
    }

    pub fn normal(&self) -> [f32; 3] {
        normalize(cross(self.axis_u, self.axis_v))
    }

    /// Whether the cell counts as "on" for interactions: alive, or for cyclic rules part of
    /// a wavefront that has just advanced.
    fn active(&self, i: usize) -> bool {
        match self.rule().kind {
            Kind::Cyclic { .. } => self.state[i] > 0 && self.age[i] < 2,
            _ => self.state[i] == 1,
        }
    }

    fn inside(&self, x: usize, y: usize) -> bool {
        let c = (self.dim as f32 - 1.0) * 0.5;
        let dx = x as f32 - c;
        match self.rule().kind {
            Kind::Elementary { .. } => dx.abs() <= self.radius && y < self.row,
            _ => {
                let dy = y as f32 - c;
                dx * dx + dy * dy <= self.radius * self.radius
            }
        }
    }

    /// The grid cell under world point `p`, ignoring the plane's warp.
    fn cell_at(&self, p: [f32; 3]) -> Option<(usize, usize)> {
        let rel = sub(p, self.center);
        let x = dot(rel, self.axis_u) / self.half_size;
        let y = dot(rel, self.axis_v) / self.half_size;
        if x.abs() >= 1.0 || y.abs() >= 1.0 {
            return None;
        }
        let to_cell = |t: f32| (((t * 0.5 + 0.5) * self.dim as f32) as usize).min(self.dim - 1);
        Some((to_cell(x), to_cell(y)))
    }

    /// Runs one generation.
    pub fn advance(&mut self) {
        match self.rule().kind {
            Kind::LifeLike {
                birth,
                survive,
                states,
            } => self.step_life(birth, survive, states),
            Kind::Elementary { rule } => self.step_row(rule),
            Kind::Cyclic { states, threshold } => self.step_cyclic(states, threshold),
        }
        self.generation += 1;
        self.dirty = true;
    }

    /// Owner of the first neighbour matching `pred`, scanning from a rotating start so no
    /// direction is favoured.
    fn inherit(&self, i: usize, pred: impl Fn(u8) -> bool) -> u8 {
        let start = self.generation as usize;
        for k in 0..8 {
            let j = (i as isize + NEIGHBOURS[(start + k) % 8]) as usize;
            if pred(self.state[j]) {
                return self.owner[j];
            }
        }
        0
    }

    fn step_life(&mut self, birth: u16, survive: u16, states: u8) {
        let n = self.dim;
        let mut live = 0;
        for y in 0..n {
            for x in 0..n {
                let i = idx(x, y);
                if !self.inside(x, y) {
                    self.next[i] = 0;
                    self.next_owner[i] = 0;
                    continue;
                }
                let count = NEIGHBOURS
                    .iter()
                    .filter(|&&o| self.state[(i as isize + o) as usize] == 1)
                    .count();
                let s = self.state[i];
                let ns = match s {
                    0 => (birth >> count & 1) as u8,
                    1 if survive >> count & 1 == 1 => 1,
                    1 if states > 2 => 2,
                    1 => 0,
                    k if k + 1 >= states => 0,
                    k => k + 1,
                };
                self.next_owner[i] = match (s, ns) {
                    (0, 1) => self.inherit(i, |st| st == 1),
                    (_, 0) => 0,
                    _ => self.owner[i],
                };
                self.age[i] = if s == 0 {
                    0
                } else {
                    self.age[i].saturating_add(1)
                };
                self.next[i] = ns;
                live += (ns == 1) as usize;
            }
        }
        std::mem::swap(&mut self.state, &mut self.next);
        std::mem::swap(&mut self.owner, &mut self.next_owner);
        self.activity = live;
    }

    fn step_cyclic(&mut self, states: u8, threshold: u8) {
        // State 0 is unlocked-not-yet soil; live cells hold colours 1..=states.
        let n = self.dim;
        let mut changed = 0;
        for y in 0..n {
            for x in 0..n {
                let i = idx(x, y);
                let s = self.state[i];
                if s == 0 || !self.inside(x, y) {
                    self.next[i] = 0;
                    self.next_owner[i] = 0;
                    continue;
                }
                let want = s % states + 1;
                let count = NEIGHBOURS
                    .iter()
                    .filter(|&&o| self.state[(i as isize + o) as usize] == want)
                    .count();
                if count >= threshold as usize {
                    self.next[i] = want;
                    self.next_owner[i] = self.inherit(i, |st| st == want);
                    self.age[i] = 0;
                    changed += 1;
                } else {
                    self.next[i] = s;
                    self.next_owner[i] = self.owner[i];
                    self.age[i] = self.age[i].saturating_add(1);
                }
            }
        }
        std::mem::swap(&mut self.state, &mut self.next);
        std::mem::swap(&mut self.owner, &mut self.next_owner);
        self.activity = changed;
    }

    fn step_row(&mut self, rule: u8) {
        let n = self.dim;
        if self.row >= n {
            // Scroll the diagram up so it keeps unrolling.
            for buffer in [&mut self.state, &mut self.age, &mut self.owner] {
                buffer.copy_within(idx(0, 1)..idx(0, n), idx(0, 0));
                buffer[idx(0, n - 1)..idx(0, n)].fill(0);
            }
            self.row = n - 1;
        }
        for a in &mut self.age {
            *a = a.saturating_add(1);
        }
        let prev = self.row.saturating_sub(1);
        let c = (n as f32 - 1.0) * 0.5;
        let mut live = 0;
        for x in 0..n {
            let i = idx(x, self.row);
            if (x as f32 - c).abs() > self.radius {
                self.state[i] = 0;
                self.owner[i] = 0;
                continue;
            }
            let parent = |dx: usize| idx((x + n + dx - 1) % n, prev);
            let (l, m, r) = (parent(0), parent(1), parent(2));
            let pattern = (self.state[l] << 2) | (self.state[m] << 1) | self.state[r];
            let alive = rule >> pattern & 1;
            self.state[i] = alive;
            self.age[i] = 0;
            self.owner[i] = if alive == 0 {
                0
            } else {
                [m, l, r]
                    .into_iter()
                    .find(|&j| self.state[j] == 1)
                    .map_or(0, |j| self.owner[j])
            };
            live += alive as usize;
        }
        self.row += 1;
        self.activity = live;
    }

    /// Widens the growth front to `radius`, seeding the newly unlocked cells.
    pub fn grow(&mut self, radius: f32, rng: &mut Rng) {
        let radius = radius.min(max_radius(self.rule(), self.dim));
        if radius <= self.radius {
            return;
        }
        let old = self.radius;
        self.radius = radius;
        let rule = *self.rule();
        if let Kind::Elementary { .. } = rule.kind {
            return;
        }
        let n = self.dim;
        let c = (n as f32 - 1.0) * 0.5;
        for y in 0..n {
            for x in 0..n {
                let (dx, dy) = (x as f32 - c, y as f32 - c);
                let d2 = dx * dx + dy * dy;
                if d2 <= old * old || d2 > radius * radius {
                    continue;
                }
                let i = idx(x, y);
                match rule.kind {
                    Kind::Cyclic { states, .. } => {
                        self.state[i] = 1 + rng.pick(states as usize) as u8;
                    }
                    _ if rng.next_f32() < rule.density => self.state[i] = 1,
                    _ => continue,
                }
                self.age[i] = 0;
                self.owner[i] = 0;
            }
        }
        self.dirty = true;
    }

    /// A beat scatters a few fresh cells inside the grown region, so settled rules keep
    /// stirring and class-1 rules keep sprouting.
    fn spark(&mut self, strength: f32, rng: &mut Rng) {
        let n = self.dim;
        match self.rule().kind {
            Kind::Elementary { .. } => {
                if self.row == 0 {
                    return;
                }
                let y = self.row - 1;
                for _ in 0..1 + (strength * 3.0) as usize {
                    let x = rng.pick(n);
                    if self.inside(x, y) {
                        let i = idx(x, y);
                        self.state[i] ^= 1;
                        self.age[i] = 0;
                    }
                }
            }
            Kind::LifeLike { .. } => {
                for _ in 0..1 + (strength * 2.0) as usize {
                    let (cx, cy) = (rng.pick(n), rng.pick(n));
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1), (2, 1), (1, 2)] {
                        let (x, y) = ((cx + dx).min(n - 1), (cy + dy).min(n - 1));
                        if self.inside(x, y) && rng.next_f32() < 0.6 {
                            let i = idx(x, y);
                            self.state[i] = 1;
                            self.age[i] = 0;
                        }
                    }
                }
            }
            Kind::Cyclic { .. } => {}
        }
        self.dirty = true;
    }

    /// Clears the grid and plants the starting seed.
    fn plant(&mut self, rng: &mut Rng) {
        for buffer in [&mut self.state, &mut self.age, &mut self.owner] {
            buffer.fill(0);
        }
        self.generation = 0;
        self.row = 0;
        self.activity = 0;
        self.dirty = true;
        let rule = *self.rule();
        let n = self.dim;
        let c = n / 2;
        match rule.kind {
            Kind::Elementary { .. } => {
                if rule.density == 0.0 {
                    self.radius = n as f32 * 0.5;
                    self.state[idx(c, 0)] = 1;
                } else {
                    self.radius = n as f32 * 0.12;
                    for x in 0..n {
                        if (x as f32 - c as f32).abs() <= self.radius
                            && rng.next_f32() < rule.density
                        {
                            self.state[idx(x, 0)] = 1;
                        }
                    }
                }
                self.row = 1;
            }
            _ => {
                self.radius = 0.0;
                self.grow((n as f32 * 0.06).max(3.0), rng);
                if let Kind::LifeLike { .. } = rule.kind {
                    // A dense core so sparse-seeded rules always have something to grow from.
                    for y in c - 2..=c + 2 {
                        for x in c - 2..=c + 2 {
                            if rng.next_f32() < 0.5 {
                                self.state[idx(x, y)] = 1;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Writes the used `dim`×`dim` region as RGBA8 texels: intensity, tone, owner, freshness.
    pub fn write_texels(&self, out: &mut [u8]) {
        let rule = self.rule();
        for y in 0..self.dim {
            for x in 0..self.dim {
                let i = idx(x, y);
                let (s, age) = (self.state[i], self.age[i]);
                let (intensity, tone) = match rule.kind {
                    Kind::LifeLike { states, .. } => match s {
                        0 => (0.0, 0.0),
                        1 => (1.0, (age as f32 / 48.0).min(1.0)),
                        k => {
                            let dying = (k - 1) as f32 / (states - 1) as f32;
                            (0.55 * (1.0 - dying), 0.6 + 0.4 * dying)
                        }
                    },
                    Kind::Elementary { .. } => (s as f32, (age as f32 / 96.0).min(1.0)),
                    Kind::Cyclic { states, .. } => match s {
                        0 => (0.0, 0.0),
                        k => (0.5, (k - 1) as f32 / states as f32),
                    },
                };
                let fresh = if intensity > 0.0 {
                    (1.0 - age as f32 / 6.0).max(0.0)
                } else {
                    0.0
                };
                let o = (y * self.dim + x) * 4;
                out[o] = (intensity * 255.0) as u8;
                out[o + 1] = (tone * 255.0) as u8;
                out[o + 2] = self.owner[i];
                out[o + 3] = (fresh * 255.0) as u8;
            }
        }
    }

    /// True once if the cells changed since the last call, so the texture layer is uploaded.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }
}

fn max_radius(rule: &Rule, dim: usize) -> f32 {
    match rule.kind {
        Kind::Elementary { .. } => dim as f32 * 0.5,
        _ => dim as f32 * 0.72,
    }
}

/// Where two planes cross, live cells of `source` seed cells of `target` along the line of
/// intersection, each with probability `chance`. Seeded cells keep the palette they came
/// from. Returns how many cells were seeded.
pub fn cross_seed(
    source: &Automaton,
    source_slot: usize,
    target: &mut Automaton,
    target_slot: usize,
    chance: f32,
    rng: &mut Rng,
) -> usize {
    let (na, nb) = (source.normal(), target.normal());
    let dir = cross(na, nb);
    let len = length(dir);
    if len < 0.08 {
        return 0; // near parallel: they barely meet
    }
    let dir = scale(dir, 1.0 / len);
    // The point on the intersection line closest to the origin, then the stretch of the line
    // near the source plane.
    let c = dot(na, nb);
    let (d1, d2) = (dot(na, source.center), dot(nb, target.center));
    let denom = 1.0 - c * c;
    let p0 = add(
        scale(na, (d1 - d2 * c) / denom),
        scale(nb, (d2 - d1 * c) / denom),
    );
    let reach = source.half_size * std::f32::consts::SQRT_2;
    let t0 = dot(sub(source.center, p0), dir);
    let cell = |a: &Automaton| 2.0 * a.half_size / a.dim as f32;
    let step = cell(source).min(cell(target)) * 0.7;
    let samples = ((2.0 * reach / step) as usize).min(400);
    let target_kind = target.rule().kind;
    let mut seeded = 0;
    for k in 0..samples {
        let p = add(p0, scale(dir, t0 - reach + k as f32 * step));
        let (Some((sx, sy)), Some((tx, ty))) = (source.cell_at(p), target.cell_at(p)) else {
            continue;
        };
        let si = idx(sx, sy);
        if !source.active(si) || !target.inside(tx, ty) || rng.next_f32() >= chance {
            continue;
        }
        let ti = idx(tx, ty);
        let ts = target.state[ti];
        target.state[ti] = match target_kind {
            Kind::Cyclic { states, .. } if ts > 0 => ts % states + 1,
            Kind::Cyclic { .. } => continue,
            _ if ts == 1 => continue,
            _ => 1,
        };
        let from = match source.owner[si] {
            0 => source_slot as u8 + 1,
            owner => owner,
        };
        target.owner[ti] = if from as usize == target_slot + 1 {
            0
        } else {
            from
        };
        target.age[ti] = 0;
        target.dirty = true;
        seeded += 1;
    }
    seeded
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub pos: [f32; 3],
    pub forward: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WarpKind {
    /// A magnifying (or shrinking) lens drifting across the screen.
    Lens = 0,
    /// A whirlpool that twists the image around its centre.
    Swirl = 1,
    /// A ring that bursts outwards from a point on a strong beat.
    Shock = 2,
    /// A rippling band that sweeps across the screen.
    Band = 3,
}

/// A distortion of the whole image. Positions are in screen units: x ∈ [-aspect, aspect],
/// y ∈ [-1, 1].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenWarp {
    pub kind: WarpKind,
    pub pos: [f32; 2],
    pub vel: [f32; 2],
    /// Lens / swirl radius, band half-width, or the shockwave ring's current radius.
    pub radius: f32,
    pub strength: f32,
    pub age: f32,
    pub life: f32,
}

impl ScreenWarp {
    /// Fades the warp in and out so it never pops.
    pub fn envelope(&self) -> f32 {
        if self.kind == WarpKind::Shock {
            return (1.0 - self.age / self.life).max(0.0).powi(2);
        }
        let edge = (self.life * 0.2).min(3.0);
        (self.age / edge)
            .min((self.life - self.age) / edge)
            .clamp(0.0, 1.0)
    }
}

/// What the field hears each frame, all 0..1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Drive {
    pub bass: f32,
    pub energy: f32,
    pub treble: f32,
}

pub struct AutomataSim {
    automata: Vec<Automaton>,
    warps: Vec<ScreenWarp>,
    rng: Rng,
    time: f32,
    camera: Camera,
    last_bass: f32,
    kick: f32,
    last_kick: f32,
    beat_cooldown: f32,
    since_beat: f32,
    beat_interval: f32,
    beat_pulse: f32,
    shock_cooldown: f32,
    interact_debt: f32,
    beats: u32,
    total_generations: u64,
}

impl AutomataSim {
    pub fn new(seed: u64) -> Self {
        let mut sim = Self {
            automata: Vec::with_capacity(MAX_AUTOMATA),
            warps: Vec::with_capacity(MAX_WARPS),
            rng: Rng::new(seed),
            time: 0.0,
            camera: Camera {
                pos: [0.0; 3],
                forward: [0.0, 0.0, -1.0],
                right: [1.0, 0.0, 0.0],
                up: [0.0, 1.0, 0.0],
            },
            last_bass: 0.0,
            kick: 0.0,
            last_kick: 0.0,
            beat_cooldown: 0.0,
            since_beat: 99.0,
            beat_interval: 0.5,
            beat_pulse: 0.0,
            shock_cooldown: 0.0,
            interact_debt: 0.0,
            beats: 0,
            total_generations: 0,
        };
        for _ in 0..MAX_AUTOMATA {
            let mut a = Automaton::blank(0, GRID);
            sim.respawn(&mut a, NEAR_SPAWN * 0.6, FAR_SPAWN);
            // Stagger lifetimes and give each a head start so the field starts full.
            a.alive_for = sim.rng.uniform(0.0, a.lifetime * 0.7);
            a.fade = 1.0;
            let head_start = 4 + sim.rng.pick(12);
            for _ in 0..head_start {
                a.advance();
            }
            let grown = a.radius + a.dim as f32 * sim.rng.uniform(0.05, 0.3);
            a.grow(grown, &mut sim.rng);
            sim.automata.push(a);
        }
        sim
    }

    pub fn automata(&self) -> &[Automaton] {
        &self.automata
    }

    pub fn automata_mut(&mut self) -> &mut [Automaton] {
        &mut self.automata
    }

    pub fn warps(&self) -> &[ScreenWarp] {
        &self.warps
    }

    pub fn camera(&self) -> Camera {
        self.camera
    }

    /// Detected tempo, falling back to a slow idle pace when no beat has come for a while.
    pub fn beats_per_second(&self) -> f32 {
        if self.since_beat < 2.5 {
            1.0 / self.beat_interval
        } else {
            IDLE_BPS
        }
    }

    /// 1 on a beat, decaying quickly.
    pub fn beat_pulse(&self) -> f32 {
        self.beat_pulse
    }

    pub fn beats(&self) -> u32 {
        self.beats
    }

    /// Generations run by all automata since the start.
    pub fn total_generations(&self) -> u64 {
        self.total_generations
    }

    /// Advances everything by `dt` seconds. `aspect` is the screen's width / height.
    pub fn step(&mut self, dt: f32, aspect: f32, drive: Drive) {
        self.time += dt;
        let energy = drive.energy.clamp(0.0, 1.0);
        let beat = self.detect_beat(dt, drive.bass);
        self.beat_pulse = (self.beat_pulse - dt * 3.0).max(0.0);
        if let Some(strength) = beat {
            self.beat_pulse = strength.min(1.0);
        }
        self.move_camera(dt, energy);

        let bps = self.beats_per_second();
        let mut automata = std::mem::take(&mut self.automata);
        for a in &mut automata {
            self.update_automaton(a, dt, bps, energy, beat);
        }
        self.automata = automata;

        self.interact_debt += dt * INTERACT_HZ;
        if self.interact_debt >= 1.0 {
            self.interact_debt = 0.0;
            let chance = 0.25 + 0.5 * energy + 0.5 * self.beat_pulse;
            self.interact(chance.min(1.0));
        }
        self.update_warps(dt, aspect, energy, beat);
    }

    fn detect_beat(&mut self, dt: f32, bass: f32) -> Option<f32> {
        let rise = (bass - self.last_bass).max(0.0);
        self.last_bass = bass;
        self.kick = (self.kick * (1.0 - dt * 4.0).max(0.0) + rise * 3.0).min(1.5);
        self.beat_cooldown -= dt;
        self.since_beat += dt;
        let hit = self.kick - self.last_kick > 0.1 && self.kick > 0.3 && self.beat_cooldown <= 0.0;
        self.last_kick = self.kick;
        if !hit {
            return None;
        }
        if (0.25..2.0).contains(&self.since_beat) {
            self.beat_interval = self.beat_interval * 0.75 + self.since_beat * 0.25;
        }
        self.since_beat = 0.0;
        self.beat_cooldown = 0.2;
        self.beats += 1;
        Some(self.kick.min(1.5))
    }

    fn move_camera(&mut self, dt: f32, energy: f32) {
        let t = self.time;
        let yaw = 0.35 * (t * 0.031).sin() + 0.15 * (t * 0.087).sin();
        let pitch = 0.18 * (t * 0.041 + 1.0).sin();
        let roll = 0.35 * (t * 0.019).sin() + 0.1 * (t * 0.07).sin();
        let forward = [
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            -yaw.cos() * pitch.cos(),
        ];
        let flat_right = normalize(cross(forward, [0.0, 1.0, 0.0]));
        let flat_up = cross(flat_right, forward);
        let (sin, cos) = roll.sin_cos();
        let right = add(scale(flat_right, cos), scale(flat_up, sin));
        let up = sub(scale(flat_up, cos), scale(flat_right, sin));
        let speed = 0.35 + 1.1 * energy + 0.8 * self.kick.min(1.0);
        self.camera = Camera {
            pos: add(self.camera.pos, scale(forward, speed * dt)),
            forward,
            right,
            up,
        };
    }

    fn update_automaton(
        &mut self,
        a: &mut Automaton,
        dt: f32,
        bps: f32,
        energy: f32,
        beat: Option<f32>,
    ) {
        let cam = self.camera;
        let ahead = dot(sub(a.center, cam.pos), cam.forward);
        if ahead < -a.half_size * 1.5 {
            // Passed: reborn far ahead, out of sight in the haze.
            self.respawn(a, FAR_SPAWN * 0.7, FAR_SPAWN);
            return;
        }
        a.alive_for += dt;
        if a.dying {
            a.fade -= dt / 2.0;
            if a.fade <= 0.0 {
                self.respawn(a, NEAR_SPAWN * 1.5, FAR_SPAWN);
            }
            return;
        }
        a.fade = (a.fade + dt / 1.5).min(1.0);
        a.idle_for = if a.activity == 0 {
            a.idle_for + dt
        } else {
            0.0
        };
        if a.alive_for > a.lifetime || a.idle_for > 3.0 {
            a.dying = true;
        }

        // Tumble slowly.
        if a.spin_rate != 0.0 {
            let angle = a.spin_rate * dt * (1.0 + energy);
            a.axis_u = rotate(a.axis_u, a.spin_axis, angle);
            a.axis_v = rotate(a.axis_v, a.spin_axis, angle);
            a.axis_u = normalize(a.axis_u);
            a.axis_v = normalize(sub(a.axis_v, scale(a.axis_u, dot(a.axis_v, a.axis_u))));
        }

        // Generations follow the tempo, faster when the music is loud; each beat also
        // pushes the growth front out and gives an immediate burst.
        let speed = a.rule().speed * a.speed;
        a.gen_debt += dt * speed * bps * (0.25 + 1.2 * energy);
        let mut radius = a.radius + dt * a.dim as f32 * 0.004 * (0.2 + energy);
        a.front_pulse = (a.front_pulse - dt * 2.0).max(0.0);
        if let Some(strength) = beat {
            a.gen_debt += speed * 0.5 * strength.min(1.0);
            radius += a.dim as f32 * 0.03 * (0.5 + strength);
            a.front_pulse = strength.min(1.0);
            if self.rng.next_f32() < 0.5 {
                a.spark(strength, &mut self.rng);
            }
        }
        a.grow(radius, &mut self.rng);
        a.gen_debt = a.gen_debt.min(MAX_GENERATIONS_PER_FRAME as f32 * 2.0);
        let mut ran = 0;
        while a.gen_debt >= 1.0 && ran < MAX_GENERATIONS_PER_FRAME {
            a.gen_debt -= 1.0;
            a.advance();
            ran += 1;
        }
        self.total_generations += ran as u64;
    }

    /// Replaces `a` with a fresh automaton between `near` and `far` ahead of the camera.
    fn respawn(&mut self, a: &mut Automaton, near: f32, far: f32) {
        let rng = &mut self.rng;
        let cam = self.camera;
        a.rule = rng.pick(RULES.len());
        a.dim = DIMS[rng.pick(DIMS.len())];
        let dist = rng.uniform(near, far);
        let spread = 1.0 + 0.45 * dist;
        a.center = add(
            add(cam.pos, scale(cam.forward, dist)),
            add(
                scale(cam.right, rng.uniform(-spread, spread)),
                scale(cam.up, rng.uniform(-spread, spread) * 0.75),
            ),
        );
        a.half_size = rng.uniform(0.7, 3.6);
        // An oblique angle to the view: never face-on, never edge-on.
        let normal = loop {
            let n = rng.unit_vector();
            let facing = dot(n, cam.forward).abs();
            if (0.3..0.93).contains(&facing) {
                break n;
            }
        };
        let helper = if normal[1].abs() < 0.9 {
            [0.0, 1.0, 0.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        let u = normalize(cross(helper, normal));
        let roll = rng.uniform(0.0, std::f32::consts::TAU);
        let v = cross(normal, u);
        a.axis_u = add(scale(u, roll.cos()), scale(v, roll.sin()));
        a.axis_v = cross(normal, a.axis_u);
        a.spin_axis = rng.unit_vector();
        a.spin_rate = rng.uniform(-0.08, 0.08);
        a.warp = [
            rng.uniform(-0.5, 0.5) * (rng.next_f32() < 0.6) as u8 as f32,
            rng.uniform(-0.4, 0.4) * (rng.next_f32() < 0.4) as u8 as f32,
            rng.uniform(0.0, 0.12) * (rng.next_f32() < 0.5) as u8 as f32,
            rng.uniform(1.5, 5.0),
        ];
        a.palette = PALETTES[rng.pick(PALETTES.len())];
        a.shape = rng.pick(4) as u8;
        a.seed = rng.next_f32();
        a.speed = rng.uniform(0.6, 1.6);
        a.lifetime = rng.uniform(25.0, 60.0);
        a.alive_for = 0.0;
        a.idle_for = 0.0;
        a.gen_debt = 0.0;
        a.fade = 0.0;
        a.dying = false;
        a.front_pulse = 0.0;
        a.plant(rng);
    }

    /// Cross-seeds every pair of visible automata whose planes meet.
    fn interact(&mut self, chance: f32) {
        let n = self.automata.len();
        for i in 0..n {
            for j in i + 1..n {
                let (a, b) = (&self.automata[i], &self.automata[j]);
                if a.fade < 0.3 || b.fade < 0.3 {
                    continue;
                }
                let reach = (a.half_size + b.half_size) * std::f32::consts::SQRT_2;
                if length(sub(a.center, b.center)) > reach {
                    continue;
                }
                let (left, right) = self.automata.split_at_mut(j);
                let (a, b) = (&mut left[i], &mut right[0]);
                cross_seed(a, i, b, j, chance, &mut self.rng);
                cross_seed(b, j, a, i, chance, &mut self.rng);
            }
        }
    }

    fn update_warps(&mut self, dt: f32, aspect: f32, energy: f32, beat: Option<f32>) {
        let pace = 0.6 + energy;
        for w in &mut self.warps {
            w.age += dt * if w.kind == WarpKind::Shock { 1.0 } else { pace };
            w.pos[0] += w.vel[0] * dt * pace;
            w.pos[1] += w.vel[1] * dt * pace;
            if w.kind == WarpKind::Shock {
                w.radius = 0.05 + w.age * 1.1;
            }
        }
        self.warps.retain(|w| w.age < w.life);

        let drifting = self
            .warps
            .iter()
            .filter(|w| w.kind != WarpKind::Shock)
            .count();
        if drifting < 3 && self.warps.len() < MAX_WARPS {
            let warp = self.drifting_warp(aspect);
            self.warps.push(warp);
        }
        self.shock_cooldown -= dt;
        if let Some(strength) = beat
            && strength > 0.7
            && self.shock_cooldown <= 0.0
            && self.warps.len() < MAX_WARPS
        {
            self.shock_cooldown = 1.2;
            let rng = &mut self.rng;
            self.warps.push(ScreenWarp {
                kind: WarpKind::Shock,
                pos: [rng.uniform(-0.7, 0.7) * aspect, rng.uniform(-0.6, 0.6)],
                vel: [0.0; 2],
                radius: 0.05,
                strength: 0.05 * strength.min(1.2),
                age: 0.0,
                life: 1.4,
            });
        }
    }

    /// A lens, swirl or band that enters from one edge and leaves by the opposite one.
    fn drifting_warp(&mut self, aspect: f32) -> ScreenWarp {
        let rng = &mut self.rng;
        let kind = [WarpKind::Lens, WarpKind::Swirl, WarpKind::Band][rng.pick(3)];
        let heading = rng.uniform(0.0, std::f32::consts::TAU);
        let dir = [heading.cos(), heading.sin()];
        let speed = rng.uniform(0.08, 0.2);
        // Start beyond the screen, behind the direction of travel, offset sideways.
        let span = aspect.max(1.0) + 0.6;
        let side = rng.uniform(-0.6, 0.6);
        let pos = [
            -dir[0] * span - dir[1] * side,
            -dir[1] * span + dir[0] * side,
        ];
        let (radius, strength) = match kind {
            WarpKind::Lens => (
                rng.uniform(0.3, 0.6),
                rng.uniform(0.08, 0.16) * if rng.next_f32() < 0.7 { 1.0 } else { -1.0 },
            ),
            WarpKind::Swirl => (rng.uniform(0.3, 0.55), rng.uniform(-1.4, 1.4)),
            _ => (rng.uniform(0.08, 0.16), rng.uniform(0.025, 0.05)),
        };
        ScreenWarp {
            kind,
            pos,
            vel: [dir[0] * speed, dir[1] * speed],
            radius,
            strength,
            age: 0.0,
            life: 2.0 * span / speed,
        }
    }
}

// Small vector helpers.
fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn length(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

fn normalize(a: [f32; 3]) -> [f32; 3] {
    scale(a, 1.0 / length(a).max(1e-6))
}

/// Rodrigues' rotation of `v` about unit `axis` by `angle` radians.
fn rotate(v: [f32; 3], axis: [f32; 3], angle: f32) -> [f32; 3] {
    let (sin, cos) = angle.sin_cos();
    add(
        add(scale(v, cos), scale(cross(axis, v), sin)),
        scale(axis, dot(axis, v) * (1.0 - cos)),
    )
}

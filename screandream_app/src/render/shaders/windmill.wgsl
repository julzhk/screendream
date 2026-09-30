// Summer windmill fields (prefixed with common.wgsl at load time).
//   vs_sky/fs_sky       sun, drifting clouds, hot-air balloons and a flock of birds
//   vs_ground/fs_ground rolling patchwork farmland with hedgerows, wind waves, cloud shadows
//   vs_mill/fs_mill     instanced smock mills; each spins and grows with one spectrum bin
//   vs_pollen/fs_pollen sunlit pollen motes that glint with the treble
struct Meadow {
  cam_pos: vec3f,
  warmth: f32, // 0 = midday, 1 = golden hour
  cam_right: vec3f,
  kick: f32,   // decaying bass-hit impulse
  cam_up: vec3f,
  wind: f32,   // accumulated wind phase
  cam_fwd: vec3f,
  _pad: f32,
  angles: array<vec4f, 16>, // sail angle per spectrum bin
  levels: array<vec4f, 16>, // smoothed level per spectrum bin
};

@group(1) @binding(0) var<uniform> m: Meadow;

const GROUND_COLS: f32 = 321.0;  // matches windmill.rs
const GROUND_STEP: f32 = 2.0;
const MILL_GRID: u32 = 32u;      // 32 × 32 instance cells, matches windmill.rs
const MILL_SPACING: f32 = 20.0;
const HUB: vec3f = vec3f(0.0, 10.2, 1.9);
const WIND_YAW: f32 = 0.6;
const FOG_END: f32 = 300.0;
const CLOUD_H: f32 = 320.0;
const FOV_F: f32 = 1.4281;       // 1 / tan(35°)
const NEAR: f32 = 0.3;
const FAR: f32 = 1500.0;

// ---------------------------------------------------------------- shared scene helpers

fn project(world: vec3f) -> vec4f {
  let d = world - m.cam_pos;
  let v = vec3f(dot(d, m.cam_right), dot(d, m.cam_up), dot(d, m.cam_fwd));
  return vec4f(v.x * FOV_F / u.aspect, v.y * FOV_F, (v.z - NEAR) * FAR / (FAR - NEAR), v.z);
}

fn rot_y(a: f32) -> mat3x3f {
  let c = cos(a); let s = sin(a);
  return mat3x3f(vec3f(c, 0.0, -s), vec3f(0.0, 1.0, 0.0), vec3f(s, 0.0, c));
}

fn rot_z(a: f32) -> mat3x3f {
  let c = cos(a); let s = sin(a);
  return mat3x3f(vec3f(c, s, 0.0), vec3f(-s, c, 0.0), vec3f(0.0, 0.0, 1.0));
}

fn ground_height(p: vec2f) -> f32 {
  return 9.0 * fbm(p * 0.0045) + 2.5 * noise(p * 0.03) - 4.0;
}

fn sun_dir() -> vec3f {
  return normalize(vec3f(0.35, 0.62 - 0.4 * m.warmth, 0.75));
}

fn sun_col() -> vec3f {
  return mix(vec3f(1.0, 0.93, 0.75), vec3f(1.0, 0.62, 0.28), m.warmth) * (1.0 + 0.35 * m.kick);
}

fn horizon_col() -> vec3f {
  return mix(vec3f(0.5, 0.72, 0.95), vec3f(1.0, 0.6, 0.28), m.warmth);
}

fn zenith_col() -> vec3f {
  return mix(vec3f(0.02, 0.17, 0.7), vec3f(0.1, 0.15, 0.5), m.warmth);
}

// Sky gradient and sun glow; also the haze colour for distant land.
fn sky(dir: vec3f) -> vec3f {
  let el = clamp(dir.y, 0.0, 1.0);
  var col = mix(horizon_col(), zenith_col(), pow(el, 0.5));
  let s = max(dot(dir, sun_dir()), 0.0);
  col += sun_col() * (pow(s, 8.0) * 0.18 + pow(s, 64.0) * 0.6) * (0.9 + 0.8 * u.bands.x);
  return col;
}

fn cloud_density(xz: vec2f) -> f32 {
  let q = xz * 0.0028 + vec2f(m.wind * 0.02, m.wind * 0.012);
  return smoothstep(0.5, 0.74, fbm(q + fbm(q * 1.7) * 0.6));
}

// Fraction of sunlight blocked by the cloud above `p` along the sun direction.
fn cloud_shadow(p: vec3f) -> f32 {
  let s = sun_dir();
  return cloud_density(p.xz + s.xz / s.y * (CLOUD_H - p.y));
}

fn aerial(col: vec3f, world: vec3f) -> vec3f {
  let d = length(world.xz - m.cam_pos.xz);
  let fog = max(1.0 - exp(-d * 0.0018), smoothstep(FOG_END * 0.75, FOG_END, d));
  return mix(col, sky(normalize(world - m.cam_pos)), fog);
}

fn finish(col: vec3f) -> vec4f {
  return idle_fade(col / (1.0 + 0.25 * max(col.r, max(col.g, col.b))));
}

// ---------------------------------------------------------------- sky

fn clouds(dir: vec3f, behind: vec3f) -> vec3f {
  if (dir.y < 0.01) { return behind; }
  let t = (CLOUD_H - m.cam_pos.y) / dir.y;
  let xz = m.cam_pos.xz + dir.xz * t;
  let d = cloud_density(xz);
  let toward_sun = cloud_density(xz + sun_dir().xz * 30.0);
  let lit = mix(vec3f(1.0, 0.98, 0.94), sun_col(), 0.35);
  let col = mix(lit * 1.1, lit * 0.62 + horizon_col() * 0.2, toward_sun * 0.8);
  let fade = exp(-t * 0.0005) * smoothstep(0.01, 0.12, dir.y);
  return mix(behind, col, d * fade);
}

// Ray/axis-aligned-box hit distance, or -1.
fn hit_box(ro: vec3f, rd: vec3f, lo: vec3f, hi: vec3f) -> f32 {
  let inv = 1.0 / rd;
  let a = (lo - ro) * inv;
  let b = (hi - ro) * inv;
  let near = max(max(min(a.x, b.x), min(a.y, b.y)), min(a.z, b.z));
  let far = min(min(max(a.x, b.x), max(a.y, b.y)), max(a.z, b.z));
  return select(-1.0, near, near > 0.0 && near < far);
}

// A few striped hot-air balloons drifting high above the fields. They always fly above
// the tallest windmill and the camera, so drawing them in the sky pass is depth-correct.
fn balloons(dir: vec3f, behind: vec3f) -> vec3f {
  var col = behind;
  var best = 1e9;
  let tile = 900.0;
  for (var i = 0; i < 4; i++) {
    let fi = f32(i);
    let home = vec2f(hash2(vec2f(fi, 3.0)), hash2(vec2f(fi, 5.0))) * tile
      + vec2f(m.wind * 2.5, m.wind * 1.2);
    let xz = m.cam_pos.xz + (fract((home - m.cam_pos.xz) / tile + 0.5) - 0.5) * tile;
    let centre = vec3f(xz.x, 95.0 + 60.0 * hash2(vec2f(fi, 7.0)) + 3.0 * sin(u.time * 0.4 + fi), xz.y);
    let radius = 9.0;
    // Envelope: a sphere stretched 1.25× vertically.
    let stretch = vec3f(1.0, 1.0 / 1.25, 1.0);
    let ro = (m.cam_pos - centre) * stretch;
    let rd = dir * stretch;
    let a = dot(rd, rd);
    let b = dot(ro, rd);
    let h = b * b - a * (dot(ro, ro) - radius * radius);
    if (h > 0.0) {
      let t = (-b - sqrt(h)) / a;
      if (t > 0.0 && t < best) {
        best = t;
        let n = normalize((ro + rd * t) * stretch);
        let gores = floor((atan2(n.x, n.z) / TAU + 0.5) * 12.0);
        var paints = array<vec3f, 4>(
          vec3f(0.95, 0.2, 0.1), vec3f(1.0, 0.8, 0.1), vec3f(0.1, 0.45, 0.9), vec3f(1.0, 0.97, 0.9));
        let k = u32(hash2(vec2f(fi, 11.0)) * 4.0);
        let paint = select(paints[k], paints[(k + 1u + u32(fi)) % 4u], gores % 2.0 < 0.5);
        let diff = clamp(dot(n, sun_dir()), 0.0, 1.0);
        let glow = pow(clamp(dot(-n, sun_dir()), 0.0, 1.0), 2.0) * 0.35; // sun through the fabric
        let lit = paint * (0.25 * zenith_col() + sun_col() * (diff + glow)) * 1.2;
        col = mix(lit, sky(dir), 1.0 - exp(-t * 0.0022));
      }
    }
    // Wicker basket hanging below.
    let bc = centre - vec3f(0.0, radius * 1.25 + 4.0, 0.0);
    let tb = hit_box(m.cam_pos, dir, bc - vec3f(1.4, 1.2, 1.4), bc + vec3f(1.4, 1.2, 1.4));
    if (tb > 0.0 && tb < best) {
      best = tb;
      col = mix(vec3f(0.35, 0.2, 0.08) * sun_col(), sky(dir), 1.0 - exp(-tb * 0.0022));
    }
  }
  return col;
}

// A loose V of birds circling slowly, wings flapping.
fn birds(dir: vec3f, behind: vec3f) -> vec3f {
  let p = vec2f(atan2(dir.x, dir.z), dir.y);
  let lead = vec2f(fract(m.wind * 0.004 + 0.1) * TAU - 3.14159, 0.3 + 0.05 * sin(u.time * 0.21));
  var ink = 0.0;
  for (var i = 0; i < 9; i++) {
    let fi = f32(i);
    let rank = floor((fi + 1.0) * 0.5);
    let side = select(-1.0, 1.0, i % 2 == 0);
    let pos = lead + vec2f(-rank * 0.028, side * rank * 0.018)
      + 0.004 * vec2f(sin(u.time * 1.3 + fi * 2.1), cos(u.time * 1.1 + fi));
    let q = (p - pos) / 0.012;
    let flap = sin(u.time * (8.0 + 6.0 * u.bands.w) + fi * 1.7);
    // Wings span horizontally; the tips rise and fall with the flap, making a shallow V.
    let wing = abs(q.x) * (0.1 + 0.45 * flap);
    let d = abs(q.y - wing);
    ink = max(ink, (1.0 - smoothstep(0.1, 0.25, d)) * step(abs(q.x), 1.0));
  }
  return mix(behind, vec3f(0.08, 0.06, 0.1), ink * 0.85);
}

struct SkyOut {
  @builtin(position) clip: vec4f,
  @location(0) uv: vec2f,
};

@vertex
fn vs_sky(@builtin(vertex_index) vertex_index: u32) -> SkyOut {
  let pos = fullscreen_pos(vertex_index);
  return SkyOut(vec4f(pos, 0.0, 1.0), pos);
}

@fragment
fn fs_sky(in: SkyOut) -> @location(0) vec4f {
  let dir = normalize(m.cam_fwd + m.cam_right * (in.uv.x * u.aspect / FOV_F) + m.cam_up * (in.uv.y / FOV_F));
  var col = sky(dir);
  let s = dot(dir, sun_dir());
  col += sun_col() * smoothstep(0.9988, 0.9993, s) * (2.5 + 1.5 * u.bands.x);
  col = clouds(dir, col);
  col = birds(dir, col);
  col = balloons(dir, col);
  return finish(col);
}

// ---------------------------------------------------------------- ground

struct GroundOut {
  @builtin(position) clip: vec4f,
  @location(0) world: vec3f,
  @location(1) normal: vec3f,
};

@vertex
fn vs_ground(@location(0) grid: vec2f) -> GroundOut {
  // Snap to whole grid steps so the land stays put as the camera flies over it.
  let snap = floor(m.cam_pos.xz / GROUND_STEP);
  let xz = (snap + grid - (GROUND_COLS - 1.0) * 0.5) * GROUND_STEP;
  let e = GROUND_STEP;
  let n = normalize(vec3f(
    ground_height(xz - vec2f(e, 0.0)) - ground_height(xz + vec2f(e, 0.0)),
    2.0 * e,
    ground_height(xz - vec2f(0.0, e)) - ground_height(xz + vec2f(0.0, e))));
  let world = vec3f(xz.x, ground_height(xz), xz.y);
  return GroundOut(project(world), world, n);
}

// Patchwork of crops with hedgerows between the fields.
fn fields(p: vec2f) -> vec3f {
  let size = 55.0;
  let warp = vec2f(fbm(p * 0.008), fbm(p * 0.008 + 7.3)) * 40.0;
  let q = (p + warp) / size;
  let cell = floor(q);
  let f = fract(q);
  let kind = hash2(cell + 3.0);
  let a = hash2(cell + 9.0) * 3.14159;
  let rows = 0.5 + 0.5 * sin(dot(p, vec2f(cos(a), sin(a))) * 1.8);
  let speck = hash2(floor(p * 1.3));
  let tint = 0.9 + 0.2 * noise(p * 0.08);

  var col: vec3f;
  if (kind < 0.24) {         // wheat
    col = mix(vec3f(0.78, 0.5, 0.1), vec3f(0.95, 0.7, 0.2), rows);
  } else if (kind < 0.38) {  // lavender rows
    col = mix(vec3f(0.12, 0.25, 0.06), vec3f(0.42, 0.2, 0.72), smoothstep(0.3, 0.6, rows));
  } else if (kind < 0.52) {  // sunflowers
    let head = fract(p / 1.6) - 0.5;
    col = mix(vec3f(0.98, 0.72, 0.02), vec3f(0.3, 0.14, 0.02), step(length(head), 0.14));
  } else if (kind < 0.66) {  // rapeseed
    col = mix(vec3f(0.85, 0.85, 0.05), vec3f(1.0, 0.92, 0.12), rows);
  } else if (kind < 0.86) {  // meadow with poppies
    col = mix(vec3f(0.2, 0.45, 0.06), vec3f(0.3, 0.55, 0.1), noise(p * 0.3));
    col = mix(col, vec3f(0.9, 0.06, 0.02), step(0.94, speck));
  } else {                   // mown hay in stripes
    col = mix(vec3f(0.75, 0.62, 0.3), vec3f(0.55, 0.52, 0.18), step(0.5, fract(dot(p, vec2f(cos(a), sin(a))) / 6.0)));
  }
  col *= tint;

  // Hedgerows, fattened into tree clumps here and there.
  let edge = min(min(f.x, 1.0 - f.x), min(f.y, 1.0 - f.y)) * size;
  let hedge = 1.0 - smoothstep(0.8, 1.6 + 2.5 * step(0.8, noise(p * 0.05)), edge);
  return mix(col, vec3f(0.06, 0.2, 0.04), hedge);
}

@fragment
fn fs_ground(in: GroundOut) -> @location(0) vec4f {
  let n = normalize(in.normal);
  var col = fields(in.world.xz);

  // Wind gusts rolling through the crops as bright bands; stronger with the mids.
  let wind_dir = vec2f(sin(WIND_YAW), cos(WIND_YAW));
  let gust = sin(dot(in.world.xz, wind_dir) * 0.06 - m.wind * 3.0 + noise(in.world.xz * 0.02) * 3.0);
  col *= 1.0 + smoothstep(0.6, 1.0, gust) * (0.15 + 0.45 * u.bands.y);

  let shadow = cloud_shadow(in.world);
  let sun = max(dot(n, sun_dir()), 0.0) * (1.0 - 0.6 * shadow);
  col *= 0.2 * zenith_col() + 0.15 * horizon_col() + sun_col() * sun * 1.35;
  return finish(aerial(col, in.world));
}

// ---------------------------------------------------------------- windmills

struct MillIn {
  @location(0) pos: vec3f,
  @location(1) normal: vec3f,
  @location(2) uv: vec2f,
  @location(3) part: f32, // 0 tower, 1 cap, 2 cloth, 3 spar, 4 hub
};

struct MillOut {
  @builtin(position) clip: vec4f,
  @location(0) world: vec3f,
  @location(1) normal: vec3f,
  @location(2) uv: vec2f,
  @location(3) @interpolate(flat) part: f32,
  @location(4) @interpolate(flat) seed: f32,
  @location(5) @interpolate(flat) level: f32,
};

@vertex
fn vs_mill(v: MillIn, @builtin(instance_index) instance: u32) -> MillOut {
  let offset = vec2f(f32(instance % MILL_GRID), f32(instance / MILL_GRID)) - f32(MILL_GRID) * 0.5;
  let cell = floor(m.cam_pos.xz / MILL_SPACING) + offset;
  let seed = hash2(cell);
  let jitter = (vec2f(hash2(cell + 17.0), hash2(cell + 29.0)) - 0.5) * MILL_SPACING * 0.55;
  let base_xz = (cell + 0.5) * MILL_SPACING + jitter;
  let present = seed < 0.5 && length(base_xz - m.cam_pos.xz) < FOG_END;

  // Each mill listens to one frequency: bass mills are big and stately, treble mills small.
  let bin = 4u + u32(hash2(cell + 41.0) * 56.0);
  let level = m.levels[bin / 4u][bin % 4u];
  let angle = m.angles[bin / 4u][bin % 4u] + seed * TAU * 7.0;
  let size = mix(1.3, 0.75, f32(bin) / 63.0) * (0.85 + 0.3 * hash2(cell + 53.0));
  let yaw = WIND_YAW + (hash2(cell + 61.0) - 0.5) * 0.7;

  var local = v.pos;
  var n = v.normal;
  if (v.part >= 1.5) {
    // Sails: spin about the windshaft and stretch with the level.
    let spin = rot_z(angle);
    local = HUB + spin * (v.pos * (0.72 + 0.65 * level));
    n = spin * n;
  }
  let turn = rot_y(yaw);
  let world = vec3f(base_xz.x, ground_height(base_xz) - 0.4, base_xz.y) + turn * (local * size);
  let clip = select(vec4f(2.0, 2.0, 2.0, 1.0), project(world), present);
  return MillOut(clip, world, turn * n, v.uv, v.part, seed, level);
}

@fragment
fn fs_mill(in: MillOut) -> @location(0) vec4f {
  let view = normalize(in.world - m.cam_pos);
  let n0 = normalize(in.normal);
  let n = select(n0, -n0, dot(n0, view) > 0.0);
  let pick = u32(in.seed * 1000.0);

  var towers = array<vec3f, 5>(vec3f(0.92, 0.86, 0.72), vec3f(0.96, 0.96, 0.93),
    vec3f(0.78, 0.3, 0.14), vec3f(0.35, 0.6, 0.85), vec3f(0.96, 0.8, 0.38));
  var caps = array<vec3f, 4>(vec3f(0.4, 0.24, 0.1), vec3f(0.18, 0.4, 0.24),
    vec3f(0.16, 0.16, 0.2), vec3f(0.62, 0.1, 0.07));
  var cloths = array<vec3f, 4>(vec3f(0.96, 0.9, 0.78), vec3f(0.88, 0.14, 0.07),
    vec3f(0.98, 0.5, 0.08), vec3f(1.0, 1.0, 0.96));
  let wood = vec3f(0.3, 0.18, 0.08);

  var col: vec3f;
  var glow = vec3f(0.0);
  if (in.part < 0.5) {
    // Tower: weatherboarding, a ring of small windows and a door facing the sails.
    col = towers[pick % 5u] * (0.9 + 0.1 * step(0.5, fract(in.uv.x * 40.0)));
    let window = step(abs(fract(in.uv.x * 4.0) - 0.5), 0.08) * step(abs(in.uv.y - 0.72), 0.04);
    let door = step(abs(in.uv.x - 0.06), 0.035) * step(in.uv.y, 0.14);
    col = mix(col, vec3f(0.08, 0.06, 0.05), max(window, door));
    glow = vec3f(1.0, 0.6, 0.2) * window * m.warmth * 0.8;
  } else if (in.part < 1.5) {
    col = caps[(pick / 5u) % 4u];
  } else if (in.part < 2.5) {
    // Sail cloth stretched over a wooden lattice; backlit cloth glows in the sun.
    let lattice = max(step(fract(in.uv.x * 9.0), 0.07), step(fract(in.uv.y * 3.0), 0.1));
    col = mix(cloths[(pick / 20u) % 4u], wood, lattice);
    glow = sun_col() * cloths[(pick / 20u) % 4u] * pow(max(dot(-n, sun_dir()), 0.0), 1.5) * 0.45 * (1.0 - lattice);
    glow += vec3f(1.0, 0.75, 0.3) * in.level * in.level * 0.35 * (1.0 - lattice);
  } else if (in.part < 3.5) {
    col = wood;
  } else {
    // Brass hub, flashing on bass hits.
    col = vec3f(0.85, 0.62, 0.2);
    glow = vec3f(1.0, 0.8, 0.4) * m.kick * 1.5;
  }

  let shadow = cloud_shadow(in.world);
  let sun = max(dot(n, sun_dir()), 0.0) * (1.0 - 0.6 * shadow);
  let rim = pow(1.0 - max(dot(n, -view), 0.0), 3.0) * 0.25;
  col = col * (0.22 * zenith_col() + 0.18 * horizon_col() + sun_col() * sun * 1.25) + glow + sun_col() * rim;
  return finish(aerial(col, in.world));
}

// ---------------------------------------------------------------- pollen

struct PollenOut {
  @builtin(position) clip: vec4f,
  @location(0) corner: vec2f,
  @location(1) @interpolate(flat) seed: f32,
  @location(2) @interpolate(flat) dist: f32,
};

@vertex
fn vs_pollen(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> PollenOut {
  var corners = array<vec2f, 6>(vec2f(-1.0, -1.0), vec2f(1.0, -1.0), vec2f(1.0, 1.0),
    vec2f(-1.0, -1.0), vec2f(1.0, 1.0), vec2f(-1.0, 1.0));
  let c = corners[vi];
  let fi = f32(ii);
  let seed = vec3f(hash2(vec2f(fi, 1.0)), hash2(vec2f(fi, 2.0)), hash2(vec2f(fi, 3.0)));
  let box = vec3f(140.0, 60.0, 140.0);
  // Drift downwind with a lazy bob, wrapped into a box that travels with the camera.
  let drift = vec3f(sin(WIND_YAW), 0.0, cos(WIND_YAW)) * m.wind * 4.0
    + vec3f(sin(u.time * 0.7 + fi), sin(u.time * 0.5 + fi * 1.3) * 1.5, cos(u.time * 0.6 + fi));
  let rel = (fract((seed * box + drift - m.cam_pos) / box) - 0.5) * box;
  let dist = length(rel);
  let size = (0.06 + 0.14 * u.bands.w) * (1.0 - smoothstep(45.0, 70.0, dist));
  let world = m.cam_pos + rel + (m.cam_right * c.x + m.cam_up * c.y) * size;
  return PollenOut(project(world), c, seed.x, dist);
}

@fragment
fn fs_pollen(in: PollenOut) -> @location(0) vec4f {
  let r = length(in.corner);
  if (r > 1.0) { discard; }
  let twinkle = 0.6 + 0.4 * sin(u.time * (4.0 + 6.0 * in.seed) + in.seed * TAU);
  let core = pow(1.0 - r, 1.5);
  let col = mix(vec3f(1.0, 0.85, 0.45), vec3f(1.0, 1.0, 0.9), core) * sun_col()
    * (0.8 + core * twinkle * (1.0 + 3.0 * u.bands.w));
  return finish(col);
}

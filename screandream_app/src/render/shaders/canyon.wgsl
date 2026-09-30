// Planetary canyon fly-through (prefixed with common.wgsl at load time). `vs_sky`/`fs_sky` paint the sky (sun, ringed planet, aurora,
// stars, shooting stars) as a full-screen triangle; `vs_terrain`/`fs_terrain` displace a
// camera-following grid into a winding canyon whose walls rise and fall with the spectrum.
struct Canyon {
  travel: f32, // camera distance along the canyon
  head: f32,   // texture v of the newest spectrum row
  hue: f32,
  kick: f32,   // decaying bass-hit impulse
};

@group(1) @binding(0) var<uniform> c: Canyon;
@group(1) @binding(1) var history: texture_2d<f32>;
@group(1) @binding(2) var history_sampler: sampler;

const COLS: f32 = 201.0;       // grid columns, matches canyon.rs
const STEP_U: f32 = 0.7;       // lateral grid spacing
const STEP_Z: f32 = 0.8;       // forward grid spacing
const ROWS_BEHIND: f32 = 12.0; // grid rows kept behind the camera
const REACH: f32 = 225.0;      // distance ahead over which the spectrum history is spread
const FOG_END: f32 = 215.0;
const FOV_F: f32 = 1.4281;     // 1 / tan(35°)
const NEAR: f32 = 0.2;
const FAR: f32 = 600.0;

// ---------------------------------------------------------------- helpers

// Sharp-crested ridges for rocky canyon walls.
fn ridged(p0: vec2f) -> f32 {
  var p = p0; var v = 0.0; var a = 0.55;
  for (var i = 0; i < 4; i++) {
    let n = 1.0 - abs(noise(p) * 2.0 - 1.0);
    v += a * n * n;
    p = p * 2.1 + vec2f(3.1, 7.7);
    a *= 0.5;
  }
  return v;
}

// Saturated cosine palette: squaring pushes the channels apart for bold, un-pastel colour.
fn bold(t: f32) -> vec3f {
  let p = 0.5 + 0.5 * cos(TAU * (vec3f(0.0, 0.33, 0.67) + t));
  return p * p;
}

// ---------------------------------------------------------------- canyon + camera

fn path_x(z: f32) -> f32 {
  return 22.0 * sin(z * 0.009) + 9.0 * sin(z * 0.023 + 1.3);
}

fn path_dx(z: f32) -> f32 {
  return 0.198 * cos(z * 0.009) + 0.207 * cos(z * 0.023 + 1.3);
}

struct Cam {
  pos: vec3f,
  right: vec3f,
  up: vec3f,
  fwd: vec3f,
};

fn camera() -> Cam {
  let z = c.travel;
  let pos = vec3f(path_x(z), 6.5 + 1.5 * sin(z * 0.013) + 0.8 * c.kick, z);
  let ahead = z + 28.0;
  let look_at = vec3f(path_x(ahead), 4.0, ahead);
  let fwd = normalize(look_at - pos);
  let flat_right = normalize(cross(vec3f(0.0, 1.0, 0.0), fwd));
  let flat_up = cross(fwd, flat_right);
  // Bank into the turns like a glider, plus a slow sway.
  let roll = -clamp((path_dx(z + 30.0) - path_dx(z)) * 2.5, -0.45, 0.45)
    + 0.035 * sin(u.time * 0.7);
  let right = flat_right * cos(roll) + flat_up * sin(roll);
  return Cam(pos, right, cross(fwd, right), fwd);
}

fn project(cam: Cam, world: vec3f) -> vec4f {
  let d = world - cam.pos;
  let v = vec3f(dot(d, cam.right), dot(d, cam.up), dot(d, cam.fwd));
  return vec4f(v.x * FOV_F / u.aspect, v.y * FOV_F, (v.z - NEAR) * FAR / (FAR - NEAR), v.z);
}

// Spectrum level for a lateral offset (floor = treble, outer peaks = bass) and a distance
// ahead (near = now, far = a few seconds ago).
fn spectrum_at(au: f32, dist: f32) -> f32 {
  let x = 1.0 - clamp((au - 2.0) / 58.0, 0.0, 1.0);
  let y = c.head - clamp(dist, 0.0, REACH) / REACH * 0.98;
  return textureSampleLevel(history, history_sampler, vec2f(x, y), 0.0).r;
}

// Returns (height, audio level) at lateral offset `lat` from the canyon centre line.
fn terrain(lat: f32, z: f32, dist: f32) -> vec2f {
  let au = abs(lat);
  let wall = smoothstep(3.5, 20.0, au);
  let side = select(0.0, 57.0, lat < 0.0); // left and right walls differ
  let ridges = ridged(vec2f(lat * 0.05 + side, z * 0.035));
  var base = wall * (7.0 + 18.0 * ridges) + smoothstep(38.0, 70.0, au) * 14.0;
  // Terraced strata give the walls a carved-mesa look.
  let tier = 3.0;
  let terraced = (floor(base / tier) + smoothstep(0.55, 1.0, fract(base / tier))) * tier;
  base = mix(base, terraced, 0.7 * wall);
  base += (1.0 - wall) * (noise(vec2f(lat * 0.3, z * 0.2)) * 0.8 - 0.4);

  let s = spectrum_at(au, dist);
  let audio = s * s * (1.2 + 22.0 * wall) * (0.55 + 0.45 * ridges) * (0.8 + 0.5 * u.amplitude);
  return vec2f(base + audio, s);
}

// ---------------------------------------------------------------- sky

fn sun_dir() -> vec3f {
  return normalize(vec3f(0.28, 0.07, 1.0));
}

// Sky gradient and sun glow, without the sun disk: also used as the terrain's haze colour.
fn sky_base(dir: vec3f) -> vec3f {
  let el = clamp(dir.y, 0.0, 1.0);
  let horizon = bold(c.hue) * 0.75;
  let zenith = bold(c.hue + 0.45) * 0.015;
  var col = mix(horizon, zenith, pow(el, 0.3));
  let s = max(dot(dir, sun_dir()), 0.0);
  col += bold(c.hue + 0.1) * (pow(s, 12.0) * 0.5 + pow(s, 120.0) * 1.0) * (0.7 + u.bands.x);
  // Bass hits flash the whole sky.
  col += horizon * c.kick * 0.25;
  return col;
}

fn sun_disk(dir: vec3f) -> vec3f {
  let s = dot(dir, sun_dir());
  return vec3f(1.0, 0.95, 0.85) * smoothstep(0.9993, 0.9996, s) * 2.0;
}

fn stars(dir: vec3f) -> vec3f {
  let p = vec2f(atan2(dir.x, dir.z), dir.y) * 220.0;
  let h = hash2(p);
  let jitter = vec2f(hash2(p + 5.0), hash2(p + 9.0)) * 0.5 + 0.25;
  let dot_ = 1.0 - smoothstep(0.03, 0.09, length(fract(p) - jitter));
  let twinkle = 0.5 + 0.5 * sin(u.time * (3.0 + 8.0 * hash2(p + 17.0)) + h * TAU);
  let star = step(0.993, h) * dot_ * (0.4 + twinkle * (0.5 + 3.0 * u.bands.w));
  return vec3f(star) * smoothstep(0.05, 0.3, dir.y);
}

fn aurora(dir: vec3f) -> vec3f {
  var col = vec3f(0.0);
  let az = atan2(dir.x, dir.z);
  for (var i = 0; i < 3; i++) {
    let fi = f32(i);
    let centre = 0.42 + 0.1 * fi + 0.06 * sin(az * (3.0 + fi) + u.time * (0.3 + 0.1 * fi))
      + 0.03 * fbm(vec2f(az * 4.0, u.time * 0.2 + fi));
    let band = exp(-abs(dir.y - centre) * (40.0 - 8.0 * u.bands.y));
    let curtain = 0.4 + 0.6 * fbm(vec2f(az * 9.0 + fi * 3.0, u.time * 0.4));
    col += bold(c.hue + 0.3 + fi * 0.12) * band * curtain;
  }
  return col * (0.15 + 1.4 * u.bands.y);
}

fn shooting_star(dir: vec3f) -> vec3f {
  let period = 2.6;
  let k = floor(u.time / period);
  let ph = fract(u.time / period);
  let seed = vec2f(k, 11.0);
  let start = vec2f(hash2(seed) * 1.4 - 0.7, 0.3 + hash2(seed + vec2f(1.0, 0.0)) * 0.35);
  let heading = normalize(vec2f(hash2(seed + vec2f(2.0, 0.0)) - 0.5, -0.35));
  let head = start + heading * ph * 0.9;
  let tail = head - heading * 0.18;
  let p = vec2f(atan2(dir.x, dir.z), dir.y);
  let pa = p - tail;
  let ba = head - tail;
  let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
  let d = length(pa - ba * h);
  let visible = 1.0 - smoothstep(0.35, 0.6, ph);
  return vec3f(1.0, 0.9, 0.8) * exp(-d * 900.0) * h * h * visible * (0.8 + 2.0 * u.bands.w);
}

// A ringed gas giant hanging above the canyon; blends over `behind`.
fn planet(dir: vec3f, behind: vec3f) -> vec3f {
  let centre = normalize(vec3f(-0.5, 0.32, 1.0)) * 1000.0;
  let radius = 120.0 * (1.0 + 0.08 * u.bands.x + 0.05 * c.kick);
  var col = behind;

  // Atmosphere halo, then the sphere itself.
  let b = dot(dir, centre);
  let closest = sqrt(max(dot(centre, centre) - b * b, 0.0));
  let halo = exp(-max(closest - radius, 0.0) / radius * 6.0) * step(0.0, b);
  col += bold(c.hue + 0.2) * halo * (0.25 + 0.6 * u.glow);
  let cover = (1.0 - smoothstep(radius * 0.994, radius * 1.006, closest)) * step(0.0, b);
  var planet_t = 1e9;
  if (cover > 0.0) {
    planet_t = b - sqrt(max(radius * radius - closest * closest, 0.0));
    let n = normalize(dir * planet_t - centre);
    let light = normalize(vec3f(0.7, 0.35, -0.4));
    let diff = clamp(dot(n, light), 0.0, 1.0);
    let swirl = fbm(vec2f(n.x * 3.0 + u.time * 0.03, n.y * 12.0));
    let stripes = sin(n.y * 24.0 + swirl * 5.0 + u.bands.y * 2.0);
    let surf = mix(bold(c.hue + 0.3), bold(c.hue + 0.72), 0.5 + 0.5 * stripes);
    let rim = pow(1.0 - clamp(dot(n, -dir), 0.0, 1.0), 3.0);
    let lit = surf * (0.06 + 1.1 * diff) + bold(c.hue + 0.2) * rim * (0.5 + u.glow);
    col = mix(col, lit, cover);
  }

  // Rings: intersect the tilted ring plane; drawn unless the planet is in front.
  let ring_n = normalize(vec3f(0.25, 1.0, -0.3));
  let denom = dot(dir, ring_n);
  if (abs(denom) > 1e-4) {
    let t = dot(centre, ring_n) / denom;
    let r = length(dir * t - centre) / radius;
    if (t > 0.0 && r > 1.3 && r < 2.5 && (t < planet_t || cover < 1.0)) {
      let grooves = 0.5 + 0.5 * sin(r * 40.0) * sin(r * 13.0 + 1.0);
      let a = smoothstep(1.3, 1.42, r) * (1.0 - smoothstep(2.3, 2.5, r)) * (0.3 + 0.7 * grooves);
      let ring = bold(c.hue + 0.55 + r * 0.15) * (0.7 + 1.8 * u.bands.z);
      col = mix(col, ring, a * 0.85);
    }
  }
  return col;
}

// Tone-map, then dim and desaturate while idle so silence never looks like a reaction.
fn finish(col0: vec3f) -> vec4f {
  return idle_fade(col0 / (1.0 + 0.3 * max(col0.r, max(col0.g, col0.b))));
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
  let cam = camera();
  let dir = normalize(cam.fwd + cam.right * (in.uv.x * u.aspect / FOV_F) + cam.up * (in.uv.y / FOV_F));
  var col = sky_base(dir) + sun_disk(dir) + stars(dir) + aurora(dir) + shooting_star(dir);
  col = planet(dir, col);
  return finish(col);
}

// ---------------------------------------------------------------- terrain

struct TerrainOut {
  @builtin(position) clip: vec4f,
  @location(0) world: vec3f,
  @location(1) normal: vec3f,
  @location(2) lateral: f32,
  @location(3) audio: f32,
  @location(4) dist: f32,
};

@vertex
fn vs_terrain(@location(0) grid: vec2f) -> TerrainOut {
  // Rows snap to whole grid steps so the terrain stays put as the camera glides over it.
  let lat = (grid.x - (COLS - 1.0) * 0.5) * STEP_U;
  let z = (floor(c.travel / STEP_Z) + grid.y - ROWS_BEHIND) * STEP_Z;
  let dist = z - c.travel;
  let t = terrain(lat, z, dist);
  let e = STEP_U;
  let hx = terrain(lat + e, z, dist).x - terrain(lat - e, z, dist).x;
  let hz = terrain(lat, z + e, dist + e).x - terrain(lat, z - e, dist - e).x;
  let n = normalize(vec3f(-hx, 2.0 * e, -hz));
  let world = vec3f(path_x(z) + lat, t.x, z);
  return TerrainOut(project(camera(), world), world, n, lat, t.y, dist);
}

@fragment
fn fs_terrain(in: TerrainOut) -> @location(0) vec4f {
  let cam = camera();
  let n = normalize(in.normal);
  let h = in.world.y;

  // Banded rock strata whose colours drift with the palette.
  let strata = 0.5 + 0.5 * sin(h * 1.1 + noise(vec2f(in.world.z * 0.05, h * 0.2)) * 2.5);
  let rock = mix(bold(c.hue + 0.62 + h * 0.012), bold(c.hue + 0.95 + h * 0.02), strata);
  let key = normalize(vec3f(-0.35, 0.75, -0.55));
  let diff = max(dot(n, key), 0.0);
  var col = rock * (0.06 + 1.0 * diff);
  col += bold(c.hue) * 0.12 * max(n.y, 0.0);                             // sky bounce
  col += bold(c.hue + 0.1) * pow(max(dot(n, sun_dir()), 0.0), 3.0) * 0.6; // sun rim

  // Crests the music is pushing up glow hot.
  col += bold(c.hue + 0.35) * pow(in.audio, 2.5) * (0.8 + 1.5 * u.glow) * smoothstep(3.0, 12.0, abs(in.lateral));

  // Topographic contour lines climbing the walls, brighter with the high mids.
  let hc = h / 2.5 - u.time * 0.25;
  let fw = fwidth(hc);
  let d = abs(fract(hc + 0.5) - 0.5);
  let line = (1.0 - smoothstep(0.5 * fw, 1.5 * fw, d)) * (1.0 - smoothstep(0.15, 0.4, fw));
  let steep = smoothstep(0.05, 0.3, 1.0 - n.y);
  col += bold(c.hue + 0.4 + hc * 0.03) * line * (0.2 + 1.3 * u.bands.z) * steep;

  // Neon survey grid across the canyon floor gives a sense of speed.
  let g = vec2f(in.lateral, in.world.z) / 4.0;
  let gw = fwidth(g);
  let gd = abs(fract(g + 0.5) - 0.5) / max(gw, vec2f(1e-4));
  let grid_line = (1.0 - min(min(gd.x, gd.y), 1.0)) * (1.0 - smoothstep(0.2, 0.5, max(gw.x, gw.y)));
  let floor_mask = 1.0 - smoothstep(4.0, 12.0, abs(in.lateral));
  col += bold(c.hue + 0.8) * grid_line * floor_mask * (0.25 + 1.2 * u.bands.w);

  // A river of light along the canyon floor, pulsing with the bass.
  let river = exp(-in.lateral * in.lateral * 0.3);
  let flow = 0.5 + 0.5 * sin(in.world.z * 0.5 - u.time * 10.0);
  col += bold(c.hue + 0.5) * river * (0.4 + 2.2 * u.bands.x + c.kick) * (0.55 + 0.45 * flow);

  // Atmospheric haze into the sky colour; fully hides the far edge of the grid.
  let view_dir = normalize(in.world - cam.pos);
  let fog = max(1.0 - exp(-max(in.dist, 0.0) * 0.006), smoothstep(FOG_END * 0.7, FOG_END, in.dist));
  col = mix(col, sky_base(view_dir), fog);
  return finish(col);
}

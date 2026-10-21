// Flights into the Mandelbrot and Julia sets (prefixed with common.wgsl).
//   vs_screen/fs_sky        night sky, stars and a companion set hanging like a moon: the Julia
//                           set of the point being dived into, or the Mandelbrot set marking
//                           the Julia parameter
//   vs_terrain/fs_terrain   the set as a landscape: ridges along its boundary, mirror-topped mesas inside,
//                           escape-time bands that glow and rise with their frequency, in a
//                           palette of the dive's own. One
//                           instance per dive; while one dive dissolves into the next, each
//                           pixel belongs to one of them, along a burning edge
//   vs_bolt/fs_bolt         lightning between the peaks, one glowing capsule per segment
//                           (additive)
struct Dive {
  view: vec4f, // centre.xy, span, rotation
  fractal: vec4f, // Julia parameter.xy, kind (0 Mandelbrot, 1 Julia), max iterations
  look: vec4f, // band phase, palette hue, range and saturation
};

struct Scene {
  cam_pos: vec4f,   // xyz, focal length
  cam_fwd: vec4f,   // xyz, dissolve progress (0 while only one dive shows)
  cam_right: vec4f, // xyz, lightning flash
  cam_up: vec4f,    // xyz, energy
  ring: vec4f,      // bass shockwave radius, strength, 0, 0
  dives: array<Dive, 2>,
  lights: array<vec4f, 6>, // per bolt light: (position, 0), (colour, intensity)
  spectrum: array<vec4f, 16>,
};

@group(1) @binding(0) var<uniform> sc: Scene;

// Must match mandel_sim.rs.
const WORLD_SPAN: f32 = 24.0;
const GRID_HALF: f32 = 48.0;
const GRID_CELLS: f32 = 360.0;
const PEAK_HEIGHT: f32 = 3.2;
const RIDGE_WIDTH: f32 = 1.4;
const SMOOTHING: f32 = 0.45 * 2.0 * GRID_HALF / GRID_CELLS;
const AUDIO_HEIGHT: f32 = 1.6;
const MESA_HEIGHT: f32 = 2.9;
const BAND_FREQ: f32 = 0.045;
const BAILOUT2: f32 = 1e4;
const LIGHTS: u32 = 3u;

const NEAR: f32 = 0.1;
const FAR: f32 = 200.0;
const FOG_FAR: f32 = 62.0;

// ---------------------------------------------------------------- camera

fn view_of(p: vec3f) -> vec3f {
  let rel = p - sc.cam_pos.xyz;
  return vec3f(dot(rel, sc.cam_right.xyz), dot(rel, sc.cam_up.xyz), dot(rel, sc.cam_fwd.xyz));
}

fn clip_of(p: vec3f) -> vec4f {
  let v = view_of(p);
  let f = sc.cam_pos.w;
  return vec4f(v.x * f / u.aspect, v.y * f, FAR * (v.z - NEAR) / (FAR - NEAR), v.z);
}

fn ndc_of(p: vec3f) -> vec2f {
  let v = view_of(p);
  let f = sc.cam_pos.w;
  return vec2f(v.x * f / u.aspect, v.y * f) / max(v.z, NEAR);
}

fn ray_of(ndc: vec2f) -> vec3f {
  let f = sc.cam_pos.w;
  return normalize(sc.cam_fwd.xyz + sc.cam_right.xyz * (ndc.x * u.aspect / f) + sc.cam_up.xyz * (ndc.y / f));
}

// ---------------------------------------------------------------- the fractal

struct Esc {
  inside: bool,
  nu: f32, // smooth escape count
  de: f32, // distance estimate to the set
};

fn in_main_bulbs(c: vec2f) -> bool {
  let x = c.x - 0.25;
  let q = x * x + c.y * c.y;
  let b = c + vec2f(1.0, 0.0);
  return q * (q + x) <= 0.25 * c.y * c.y || dot(b, b) <= 0.0625;
}

// Iterates z² + c from z0; for the Mandelbrot set dz/dc is tracked (`one` = 1), for Julia
// sets dz/dz0 (`one` = 0, dz starting at 1).
fn iterate(z0: vec2f, c: vec2f, dz0: vec2f, one: f32, max_iter: i32) -> Esc {
  var z = z0;
  var dz = dz0;
  var huge = false;
  for (var n = 0; n < max_iter; n++) {
    dz = 2.0 * vec2f(z.x * dz.x - z.y * dz.y, z.x * dz.y + z.y * dz.x) + vec2f(one, 0.0);
    // Keep the derivative finite; by then the distance is far below a pixel anyway.
    if (dot(dz, dz) > 1e30) { dz *= 1e-15; huge = true; }
    z = vec2f(z.x * z.x - z.y * z.y, 2.0 * z.x * z.y) + c;
    let r2 = dot(z, z);
    if (r2 > BAILOUT2) {
      let r = sqrt(r2);
      let nu = f32(n) + 1.0 - log2(log2(r));
      let de = select(0.5 * r * log(r) / max(length(dz), 1e-30), 0.0, huge);
      return Esc(false, nu, de);
    }
  }
  return Esc(true, 0.0, 0.0);
}

fn escape(d: Dive, q: vec2f) -> Esc {
  let max_iter = i32(d.fractal.w);
  if (d.fractal.z > 0.5) {
    return iterate(q, d.fractal.xy, vec2f(1.0, 0.0), 0.0, max_iter);
  }
  if (in_main_bulbs(q)) { return Esc(true, 0.0, 0.0); }
  return iterate(vec2f(0.0), q, vec2f(0.0), 1.0, max_iter);
}

fn to_plane(d: Dive, w: vec2f) -> vec2f {
  let cs = vec2f(cos(d.view.w), sin(d.view.w));
  let r = vec2f(cs.x * w.x - cs.y * w.y, cs.y * w.x + cs.x * w.y);
  return d.view.xy + r * (d.view.z / WORLD_SPAN);
}

fn bin(i: u32) -> f32 {
  return sc.spectrum[i / 4u][i % 4u];
}

fn spectrum_at(t: f32) -> f32 {
  let x = clamp(t, 0.0, 1.0) * 63.0;
  let i = u32(floor(x));
  return mix(bin(i), bin(min(i + 1u, 63u)), fract(x));
}

// 0 (bass) to 1 (treble) and back, banded by escape time.
fn band_of(nu: f32, phase: f32) -> f32 {
  let x = fract(nu * BAND_FREQ + phase);
  return 1.0 - abs(2.0 * x - 1.0);
}

// The dive's palette at t: a slice of the colour wheel, dark at the bass end. Matches
// `Palette::colour` in mandel_sim.rs.
fn band_colour(d: Dive, t: f32) -> vec3f {
  let h = fract(d.look.y + d.look.z * t);
  let pure = clamp(abs(fract(h + vec3f(0.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0) - 1.0, vec3f(0.0), vec3f(1.0));
  return mix(vec3f(1.0), pure, d.look.w) * (0.55 + 0.45 * t);
}

fn ridge(d: Dive, e: Esc) -> f32 {
  return exp(-e.de / d.view.z * WORLD_SPAN / RIDGE_WIDTH);
}

fn height(d: Dive, e: Esc) -> f32 {
  if (e.inside) { return MESA_HEIGHT; }
  let r = ridge(d, e);
  let level = spectrum_at(band_of(e.nu, d.look.x));
  return PEAK_HEIGHT * r + AUDIO_HEIGHT * level * (0.25 + 0.75 * r);
}

// ---------------------------------------------------------------- dissolve

// Each pixel shows one dive: x is which (0 or 1), y how near it is to the burning edge.
// The new dive burns in from the middle of the screen outwards.
fn owner(ndc: vec2f) -> vec2f {
  let t = sc.cam_fwd.w;
  if (t <= 0.0) { return vec2f(0.0); }
  let s = vec2f(ndc.x * u.aspect, ndc.y);
  let n = 0.6 * fbm(s * 2.2 + vec2f(u.time * 0.05, -u.time * 0.04)) + 0.4 * min(length(s) / 1.8, 1.0);
  let edge = mix(-0.1, 1.1, t);
  return vec2f(select(0.0, 1.0, n < edge), 1.0 - smoothstep(0.0, 0.035, abs(n - edge)));
}

fn ember(ndc: vec2f) -> vec3f {
  let edge = owner(ndc).y;
  return mix(vec3f(1.0, 0.35, 0.05), vec3f(1.0, 0.9, 0.6), edge * edge) * edge * 2.2;
}

// ---------------------------------------------------------------- sky

fn sky_base(dir: vec3f, d: Dive) -> vec3f {
  let horizon = band_colour(d, 0.5) * 0.025 + vec3f(0.006, 0.005, 0.012);
  let zenith = vec3f(0.001, 0.001, 0.003);
  var col = mix(horizon, zenith, smoothstep(-0.02, 0.55, dir.y));
  // Lightning lights the clouds.
  col += vec3f(0.55, 0.6, 1.0) * sc.cam_right.w * 0.06 * (1.0 - smoothstep(0.0, 0.7, dir.y));
  col += band_colour(d, 0.0) * u.bands.x * 0.012;
  return col;
}

fn stars(dir: vec3f) -> vec3f {
  let p = vec2f(atan2(dir.x, dir.z), dir.y) * 260.0;
  let h = hash2(p);
  let jitter = vec2f(hash2(p + 5.0), hash2(p + 9.0)) * 0.5 + 0.25;
  let spot = 1.0 - smoothstep(0.03, 0.09, length(fract(p) - jitter));
  let twinkle = 0.5 + 0.5 * sin(u.time * (2.0 + 7.0 * hash2(p + 17.0)) + h * TAU);
  let star = step(0.992, h) * spot * (0.3 + twinkle * (0.4 + 2.5 * u.bands.w));
  return vec3f(star) * smoothstep(0.02, 0.25, dir.y);
}

// The companion set, as (colour, cover): cover 0 means the colour is halo, to be added.
// For a Mandelbrot dive, the Julia set of the point being dived into
// (which looks like the landscape below); for a Julia dive, the Mandelbrot set with the
// Julia parameter marked.
fn companion(dir: vec3f, d: Dive) -> vec4f {
  // Hangs in the upper right of the view.
  let m_f = normalize(sc.cam_fwd.xyz + sc.cam_up.xyz * 0.4 + sc.cam_right.xyz * 0.5);
  let m_r = normalize(cross(vec3f(0.0, 1.0, 0.0), m_f));
  let m_u = cross(m_f, m_r);
  let k = dot(dir, m_f);
  if (k < 0.8) { return vec4f(0.0); }
  let radius = 0.1 * (1.0 + 0.06 * u.bands.x);
  let q = vec2f(dot(dir, m_r), dot(dir, m_u)) / k / radius;
  let r = length(q);
  let halo = exp(-max(r - 1.0, 0.0) * 7.0) * (0.12 + 0.5 * u.bands.x) * step(1.0, r);
  if (r > 1.0) { return vec4f(band_colour(d, 1.0) * halo, 0.0); }

  let spin = u.time * 0.03;
  let cs = vec2f(cos(spin), sin(spin));
  let s = vec2f(cs.x * q.x - cs.y * q.y, cs.y * q.x + cs.x * q.y);
  var e: Esc;
  var marker = 0.0;
  if (d.fractal.z > 0.5) {
    let c = s * 1.35 + vec2f(-0.6, 0.0);
    e = iterate(vec2f(0.0), c, vec2f(0.0), 1.0, 96);
    let m = length(c - d.fractal.xy);
    marker = exp(-m * 60.0) * (0.8 + 0.6 * sin(u.time * 5.0));
  } else {
    e = iterate(s * 1.6, d.view.xy, vec2f(1.0, 0.0), 0.0, 96);
  }
  var col = vec3f(0.01, 0.008, 0.02);
  if (!e.inside) {
    let t = fract(e.nu * 0.03 + d.look.x);
    let glow = exp(-e.de * 6.0);
    col = band_colour(d, 1.0 - abs(2.0 * t - 1.0)) * (0.02 + 0.8 * glow) * (0.5 + 0.7 * u.glow);
  }
  col += vec3f(1.0, 0.95, 0.85) * marker;
  let rim = smoothstep(0.85, 1.0, r) * 0.25;
  let cover = 1.0 - smoothstep(0.96, 1.0, r);
  return vec4f(col + band_colour(d, 1.0) * rim, max(cover * 0.9, 1e-3));
}

fn vignette(ndc: vec2f) -> f32 {
  return 1.0 - 0.35 * smoothstep(0.6, 1.6, length(ndc * vec2f(1.0, 0.8)));
}

fn finish(col0: vec3f, ndc: vec2f) -> vec4f {
  let col = col0 / (1.0 + 0.3 * max(col0.r, max(col0.g, col0.b)));
  return idle_fade(col * vignette(ndc));
}

struct ScreenOut {
  @builtin(position) clip: vec4f,
  @location(0) ndc: vec2f,
};

@vertex
fn vs_screen(@builtin(vertex_index) vi: u32) -> ScreenOut {
  let pos = fullscreen_pos(vi);
  return ScreenOut(vec4f(pos, 1.0, 1.0), pos);
}

@fragment
fn fs_sky(in: ScreenOut) -> @location(0) vec4f {
  let own = owner(in.ndc);
  let d = sc.dives[u32(own.x)];
  let dir = ray_of(in.ndc);
  var col = sky_base(dir, d) + stars(dir);
  let moon = companion(dir, d);
  col = mix(col, moon.rgb, moon.a) + select(moon.rgb, vec3f(0.0), moon.a > 0.0);
  col += ember(in.ndc);
  return finish(col, in.ndc);
}

// ---------------------------------------------------------------- terrain

struct TerrainOut {
  @builtin(position) clip: vec4f,
  @location(0) world: vec3f,
  @location(1) @interpolate(flat) dive: u32,
};

@vertex
fn vs_terrain(@location(0) grid: vec2f, @builtin(instance_index) ii: u32) -> TerrainOut {
  let d = sc.dives[ii];
  let w = (grid / GRID_CELLS * 2.0 - 1.0) * GRID_HALF;
  // Average four samples around the vertex, which rounds off spikes narrower than the grid.
  var h = 0.0;
  for (var i = 0; i < 4; i++) {
    let o = vec2f(select(-1.0, 1.0, (i & 1) == 0), select(-1.0, 1.0, (i & 2) == 0)) * SMOOTHING;
    h += height(d, escape(d, to_plane(d, w + o))) * 0.25;
  }
  let world = vec3f(w.x, h, w.y);
  return TerrainOut(clip_of(world), world, ii);
}

// Light from the bolts in the air.
fn bolt_light(p: vec3f, n: vec3f) -> vec3f {
  var col = vec3f(0.0);
  for (var i = 0u; i < LIGHTS; i++) {
    let pos = sc.lights[i * 2u].xyz;
    let lc = sc.lights[i * 2u + 1u];
    let l = pos - p;
    let att = lc.w / (1.0 + dot(l, l) * 0.12);
    col += lc.rgb * att * (0.25 + max(dot(n, normalize(l)), 0.0));
  }
  return col;
}

@fragment
fn fs_terrain(in: TerrainOut) -> @location(0) vec4f {
  let d = sc.dives[in.dive];
  let e = escape(d, to_plane(d, in.world.xz));
  // Mostly the smooth mesh's slope, with a touch of detail from this very pixel's height.
  let p = vec3f(in.world.x, height(d, e), in.world.z);
  var fine = normalize(cross(dpdy(p), dpdx(p)) + vec3f(0.0, 1e-6, 0.0));
  fine = select(fine, -fine, fine.y < 0.0);
  var n = normalize(cross(dpdy(in.world), dpdx(in.world)) + vec3f(0.0, 1e-6, 0.0));
  n = select(n, -n, n.y < 0.0);
  n = normalize(mix(n, fine, 0.3) + vec3f(0.0, 0.15, 0.0));
  let k = e.nu * BAND_FREQ * 5.0;
  let kw = fwidth(k);

  let ndc = ndc_of(in.world);
  let own = owner(ndc);
  if (u32(own.x) != in.dive) { discard; }

  let cam = sc.cam_pos.xyz;
  let to_p = in.world - cam;
  let dist = length(to_p);
  let view = to_p / dist;
  let key = normalize(vec3f(-0.4, 0.7, 0.35));
  var col: vec3f;
  var normal = n;
  if (e.inside) {
    // The mesa top: a still, dark pool that mirrors the sky; the bass sends ripples across it.
    let rp = in.world.xz;
    let wave = sin(length(rp) * 3.0 - u.time * 4.0) * (0.02 + 0.12 * u.bands.x)
      + (noise(rp * 1.7 + u.time * 0.3) - 0.5) * 0.06;
    normal = normalize(vec3f(wave * rp.x / max(length(rp), 1e-3), 1.0, wave * rp.y / max(length(rp), 1e-3)));
    let r = reflect(view, normal);
    let fres = 0.25 + 0.75 * pow(1.0 - max(dot(-view, normal), 0.0), 4.0);
    col = vec3f(0.004, 0.006, 0.014) + sky_base(r, d) * fres * 1.4 + stars(r) * fres;
    col += band_colour(d, 0.3) * u.bands.x * 0.05;
  } else {
    let t = band_of(e.nu, d.look.x);
    let base = band_colour(d, t);
    let level = spectrum_at(t);
    let r = ridge(d, e);
    let diff = max(dot(n, key), 0.0);
    col = base * (0.04 + 0.22 * diff) * (0.4 + 0.6 * r);
    // Bands glow when their frequency sounds; the boundary ridges burn brightest.
    col += base * level * (0.15 + 1.6 * r) * (0.6 + 0.8 * u.glow);
    col += base * pow(r, 6.0) * 0.35;
    // Thin contour lines between escape bands.
    let line = (1.0 - smoothstep(0.5 * kw, 1.5 * kw, abs(fract(k + 0.5) - 0.5)))
      * (1.0 - smoothstep(0.15, 0.4, kw));
    col += base * line * (0.1 + 0.9 * level);
    // Frosted peaks.
    col += vec3f(0.6, 0.65, 0.8) * smoothstep(0.8, 1.0, r) * pow(max(n.y, 0.0), 4.0) * 0.12;
  }
  col += bolt_light(in.world, normal) * select(1.0, 0.4, e.inside);

  // A shockwave of light rolling out from the dive's centre on bass onsets.
  let rd = abs(length(in.world.xz) - sc.ring.x);
  col += band_colour(d, 0.0) * exp(-rd * 1.5) * sc.ring.y * 0.9;

  let fog = max(1.0 - exp(-dist * 0.01), smoothstep(FOG_FAR * 0.7, FOG_FAR, dist));
  col = mix(col, sky_base(view, d), fog);
  col += ember(ndc);
  return finish(col, ndc);
}

// ---------------------------------------------------------------- lightning

struct BoltOut {
  @builtin(position) clip: vec4f,
  @location(0) local: vec2f,                      // along, across the segment (world units)
  @location(1) @interpolate(flat) shape: vec4f,   // length, core width, intensity, distance
  @location(2) @interpolate(flat) colour: vec3f,
};

@vertex
fn vs_bolt(
  @builtin(vertex_index) vi: u32,
  @location(0) a: vec4f,      // xyz, core width
  @location(1) b: vec4f,      // xyz, intensity
  @location(2) colour: vec4f,
) -> BoltOut {
  var o: BoltOut;
  let mid = 0.5 * (a.xyz + b.xyz);
  if (view_of(mid).z < 0.5 || b.w <= 0.001) {
    o.clip = vec4f(2.0, 2.0, 2.0, 1.0);
    return o;
  }
  let radius = a.w * 9.0 + 0.05;
  var axis = b.xyz - a.xyz;
  let len = length(axis);
  let to_cam = normalize(sc.cam_pos.xyz - mid);
  axis = select(sc.cam_right.xyz, axis / max(len, 1e-6), len > 1e-4);
  var side = cross(axis, to_cam);
  side = select(sc.cam_up.xyz, normalize(side), length(side) > 1e-4);
  var corners = array<vec2f, 6>(vec2f(0.0, -1.0), vec2f(1.0, -1.0), vec2f(0.0, 1.0),
                                vec2f(1.0, -1.0), vec2f(1.0, 1.0), vec2f(0.0, 1.0));
  let c = corners[vi];
  let s = select(-radius, len + radius, c.x > 0.5);
  let world = a.xyz + axis * s + side * c.y * radius;
  o.clip = clip_of(world);
  o.local = vec2f(s, c.y * radius);
  o.shape = vec4f(len, a.w, b.w, length(mid - sc.cam_pos.xyz));
  o.colour = colour.rgb;
  return o;
}

@fragment
fn fs_bolt(in: BoltOut) -> @location(0) vec4f {
  let len = in.shape.x;
  let w = in.shape.y;
  let along = max(max(-in.local.x, in.local.x - len), 0.0);
  let r = length(vec2f(along, in.local.y));
  let core = exp(-(r * r) / (w * w));
  let halo = exp(-r / (w * 3.0)) * 0.3;
  let haze = 1.0 - smoothstep(FOG_FAR * 0.5, FOG_FAR, in.shape.w);
  let col = mix(in.colour, vec3f(1.0), core * 0.85) * (core * 2.2 + halo) * in.shape.z * haze;
  return vec4f(col * mix(0.45, 1.0, u.live), 1.0);
}

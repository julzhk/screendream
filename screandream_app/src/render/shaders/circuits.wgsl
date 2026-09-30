// Fractal circuits (prefixed with common.wgsl at load time).
//   vs_surface/fs_surface  dark steel plate, warped in 3D by the bass and the pen tips
//   vs_trace/fs_trace      glowing trace segments and solder pads (additive)
//   vs_head/fs_head        reticles and glow around each pen tip (additive)
//   vs_overlay/fs_overlay  vignette, scanlines and flicker (multiplied over everything)
struct Circuit {
  cam_pos: vec3f,
  coverage: f32, // fraction of the screen recently drawn on
  cam_right: vec3f,
  kick: f32,
  cam_up: vec3f,
  sweep: f32,    // 0..1 position of the scan sweep
  cam_fwd: vec3f,
  bass: f32,     // slowly smoothed bass
  heads: array<vec4f, 8>, // tip xy, smoothed level, fast level
};

@group(1) @binding(0) var<uniform> c: Circuit;

const FOV_F: f32 = 1.4281; // 1 / tan(35°)
const NEAR: f32 = 0.05;
const FAR: f32 = 20.0;
const DOMAIN: vec2f = vec2f(1.55, 0.95);     // matches fractal_curves.rs
const LIFETIME: f32 = 30.0;                  // SEGMENT_LIFETIME in fractal_curves.rs
const FADE: f32 = 9.0;
const SURF_COLS: f32 = 241.0;                // matches circuits.rs
const SURF_ROWS: f32 = 161.0;
const SURF_HALF: vec2f = vec2f(3.4, 2.4);
const PANEL: f32 = 0.31;

fn pen_col(i: u32) -> vec3f {
  var cols = array<vec3f, 8>(
    vec3f(1.0, 0.16, 0.06),  // bass: warning red
    vec3f(1.0, 0.42, 0.04),  // hazard orange
    vec3f(1.0, 0.7, 0.14),   // sodium amber
    vec3f(0.55, 1.0, 0.22),  // phosphor green
    vec3f(0.12, 0.9, 0.72),  // coolant teal
    vec3f(0.2, 0.62, 1.0),   // arc blue
    vec3f(0.55, 0.42, 1.0),  // ion violet
    vec3f(0.85, 0.95, 1.0)); // treble: plasma white
  return cols[i % 8u];
}

// Height of the plate: a bass ripple, slow folds that deepen as the screen fills, and a
// bulge under each pen tip.
fn warp(p: vec2f) -> f32 {
  let r = length(p);
  var h = 0.05 * c.bass * sin(r * 8.0 - u.time * 2.0) * exp(-r * 0.9);
  let fold = 0.05 + 0.32 * c.coverage;
  h += fold * sin(p.x * 1.5 + u.time * 0.19) * cos(p.y * 1.8 - u.time * 0.15);
  h -= 0.12 * c.coverage * c.coverage * dot(p, p);
  for (var i = 0; i < 8; i++) {
    let d = p - c.heads[i].xy;
    h += 0.1 * c.heads[i].z * exp(-dot(d, d) * 25.0);
  }
  return h;
}

fn surface(p: vec2f) -> vec3f {
  return vec3f(p, warp(p));
}

fn project(world: vec3f) -> vec4f {
  let d = world - c.cam_pos;
  let v = vec3f(dot(d, c.cam_right), dot(d, c.cam_up), dot(d, c.cam_fwd));
  return vec4f(v.x * FOV_F / u.aspect, v.y * FOV_F, (v.z - NEAR) * FAR / (FAR - NEAR), v.z);
}

// ---------------------------------------------------------------- plate

struct SurfOut {
  @builtin(position) clip: vec4f,
  @location(0) world: vec3f,
  @location(1) normal: vec3f,
};

@vertex
fn vs_surface(@location(0) grid: vec2f) -> SurfOut {
  let p = (grid / vec2f(SURF_COLS - 1.0, SURF_ROWS - 1.0) * 2.0 - 1.0) * SURF_HALF;
  let e = 0.01;
  let n = normalize(vec3f(
    -(warp(p + vec2f(e, 0.0)) - warp(p - vec2f(e, 0.0))) / (2.0 * e),
    -(warp(p + vec2f(0.0, e)) - warp(p - vec2f(0.0, e))) / (2.0 * e),
    1.0));
  let world = surface(p);
  return SurfOut(project(world), world, n);
}

@fragment
fn fs_surface(in: SurfOut) -> @location(0) vec4f {
  let p = in.world.xy;
  let n = normalize(in.normal);
  let view = normalize(c.cam_pos - in.world);

  // Brushed gunmetal panels, some tinted, a few with hazard stripes.
  let cell = floor(p / PANEL);
  let f = fract(p / PANEL);
  let brushed = noise(vec2f(p.x * 300.0, p.y * 6.0 + cell.x * 13.0));
  var base = vec3f(0.024, 0.027, 0.032) * (0.75 + 0.5 * brushed) * (0.75 + 0.5 * hash2(cell));
  let hazard = step(hash2(cell + 7.0), 0.06) * step(0.5, fract((f.x + f.y) * 5.0));
  base = mix(base, vec3f(0.06, 0.04, 0.004), hazard * 0.5);

  let seam_dist = min(min(f.x, 1.0 - f.x), min(f.y, 1.0 - f.y)) * PANEL;
  let seam = 1.0 - smoothstep(0.0015, 0.004, seam_dist);
  let rivet_dist = length(abs(f - 0.5) - 0.43) * PANEL;
  let rivet = 1.0 - smoothstep(0.004, 0.007, rivet_dist);
  let etch_g = p / 0.05;
  let etch_w = fwidth(etch_g);
  let etch_d = abs(fract(etch_g + 0.5) - 0.5) / max(etch_w, vec2f(1e-4));
  let etch = (1.0 - min(min(etch_d.x, etch_d.y), 1.0)) * (1.0 - smoothstep(0.3, 0.6, max(etch_w.x, etch_w.y)));

  // Cold overhead key light with a brushed specular streak.
  let key = normalize(vec3f(-0.4, 0.6, 0.7));
  let diff = max(dot(n, key), 0.0);
  let spec = pow(max(dot(n, normalize(key + view)), 0.0), 60.0) * (0.1 + 0.2 * brushed);
  var col = base * (0.35 + 0.9 * diff) * (1.0 - 0.7 * seam) + vec3f(0.5, 0.6, 0.7) * spec;
  col += vec3f(0.05, 0.06, 0.07) * rivet * (0.5 + diff);
  col += vec3f(0.02, 0.05, 0.06) * etch;

  // Each pen tip is a hot light source skimming over the metal.
  for (var i = 0u; i < 8u; i++) {
    let h = c.heads[i];
    let light = vec3f(h.xy, warp(h.xy) + 0.08) - in.world;
    let d2 = dot(light, light);
    let lit = max(dot(n, normalize(light)), 0.0);
    col += pen_col(i) * h.w * 0.006 / (d2 + 0.004) * (0.25 + lit) * (0.6 + rivet);
  }

  // Scan sweep and bass-hit seam flash.
  let sweep_y = (c.sweep * 2.0 - 1.0) * 1.4;
  col += vec3f(0.05, 0.25, 0.3) * exp(-abs(p.y - sweep_y) * 60.0) * 0.4;
  col += vec3f(0.08, 0.2, 0.25) * exp(-max(p.y - sweep_y, 0.0) * 6.0) * step(p.y, sweep_y) * 0.03;
  col += vec3f(1.0, 0.45, 0.05) * seam * c.kick * 0.04;

  // Beyond the drawing area the plate falls away into darkness.
  let outside = max(abs(p.x) - DOMAIN.x, abs(p.y) - DOMAIN.y);
  col *= 1.0 - 0.9 * smoothstep(0.0, 0.8, outside);
  return idle_fade(col);
}

// ---------------------------------------------------------------- traces

struct TraceIn {
  @location(0) ends: vec4f, // a.xy, b.xy
  @location(1) info: vec4f, // birth, pen, width, via
};

struct TraceOut {
  @builtin(position) clip: vec4f,
  @location(0) local: vec2f, // along, across
  @location(1) @interpolate(flat) len: f32,
  @location(2) @interpolate(flat) width: f32,
  @location(3) @interpolate(flat) age: f32,
  @location(4) @interpolate(flat) pen: f32,
  @location(5) @interpolate(flat) via: f32,
};

@vertex
fn vs_trace(@builtin(vertex_index) vi: u32, t: TraceIn) -> TraceOut {
  var corners = array<vec2f, 6>(vec2f(0.0, -1.0), vec2f(1.0, -1.0), vec2f(1.0, 1.0),
    vec2f(0.0, -1.0), vec2f(1.0, 1.0), vec2f(0.0, 1.0));
  let corner = corners[vi];
  let a = t.ends.xy;
  let b = t.ends.zw;
  let age = u.time - t.info.x;
  let via = t.info.w;
  let width = t.info.z * select(1.0, 2.2, via > 0.5);
  let reach = width * 4.0; // quad half-size, leaving room for the glow
  let delta = b - a;
  let len = length(delta);
  let dir = select(vec2f(1.0, 0.0), delta / max(len, 1e-6), len > 1e-6);
  let side = vec2f(-dir.y, dir.x);
  let along = mix(-reach, len + reach, corner.x);
  let across = corner.y * reach;
  let p = a + dir * along + side * across;
  var clip = project(surface(p) + vec3f(0.0, 0.0, 0.004));
  if (age < 0.0 || age > LIFETIME || t.info.z <= 0.0) {
    clip = vec4f(2.0, 2.0, 2.0, 1.0);
  }
  return TraceOut(clip, vec2f(along, across), len, width, age, t.info.y, via);
}

@fragment
fn fs_trace(in: TraceOut) -> @location(0) vec4f {
  let x = in.local.x;
  let y = in.local.y;
  var dist: f32;
  if (in.via > 0.5) {
    dist = abs(length(vec2f(x, y)) - in.width); // solder pad ring
  } else {
    dist = length(vec2f(max(max(-x, x - in.len), 0.0), y));
  }
  let core = 1.0 - smoothstep(in.width * 0.45, in.width, dist);
  let halo = exp(-dist / in.width * 1.3) * 0.3;
  let fade = exp(-in.age / FADE) * (1.0 - smoothstep(LIFETIME * 0.75, LIFETIME, in.age));

  // White-hot when fresh, cooling to the pen colour, then to dull embers.
  let base = pen_col(u32(in.pen));
  let hot = mix(vec3f(1.0, 0.95, 0.85) * 1.8, base, smoothstep(0.0, 1.2, in.age));
  let col = mix(hot, base * vec3f(0.9, 0.45, 0.3) * 0.6, smoothstep(4.0, 16.0, in.age));
  return idle_fade(col * (core * 1.1 + halo) * fade * (1.0 + 0.4 * c.kick));
}

// ---------------------------------------------------------------- pen tips

struct HeadOut {
  @builtin(position) clip: vec4f,
  @location(0) corner: vec2f,
  @location(1) @interpolate(flat) pen: u32,
};

@vertex
fn vs_head(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> HeadOut {
  var corners = array<vec2f, 6>(vec2f(-1.0, -1.0), vec2f(1.0, -1.0), vec2f(1.0, 1.0),
    vec2f(-1.0, -1.0), vec2f(1.0, 1.0), vec2f(-1.0, 1.0));
  let corner = corners[vi];
  let h = c.heads[ii];
  let size = 0.05 + 0.06 * h.z;
  let centre = surface(h.xy) + vec3f(0.0, 0.0, 0.01);
  let world = centre + (c.cam_right * corner.x + c.cam_up * corner.y) * size;
  return HeadOut(project(world), corner, ii);
}

@fragment
fn fs_head(in: HeadOut) -> @location(0) vec4f {
  let h = c.heads[in.pen];
  let r = length(in.corner);
  let glow = exp(-r * 5.0) * (0.5 + 1.8 * h.w);
  // A broken targeting ring that spins with the pen's energy, plus crosshair ticks.
  let ang = atan2(in.corner.y, in.corner.x) / TAU;
  let spin = u.time * (0.2 + 1.2 * h.z) * select(-1.0, 1.0, in.pen % 2u == 0u);
  let ring = (1.0 - smoothstep(0.015, 0.035, abs(r - 0.72))) * step(0.18, fract(ang * 4.0 + spin));
  let ticks = (1.0 - smoothstep(0.01, 0.025, min(abs(in.corner.x), abs(in.corner.y))))
    * step(0.82, r) * step(r, 0.98);
  let col = pen_col(in.pen) * (glow + (ring * 0.5 + ticks * 0.4) * (0.3 + h.z));
  return idle_fade(col * (1.0 - step(1.0, r)));
}

// ---------------------------------------------------------------- monitor overlay

struct OverlayOut {
  @builtin(position) clip: vec4f,
  @location(0) uv: vec2f,
};

@vertex
fn vs_overlay(@builtin(vertex_index) vertex_index: u32) -> OverlayOut {
  let pos = fullscreen_pos(vertex_index);
  return OverlayOut(vec4f(pos, 0.0, 1.0), pos);
}

@fragment
fn fs_overlay(in: OverlayOut) -> @location(0) vec4f {
  let q = in.uv * vec2f(u.aspect, 1.0);
  let vignette = 1.0 - 0.6 * smoothstep(0.5, 1.9, length(q));
  let scan = 0.86 + 0.14 * sin(in.clip.y * 1.5);
  let flicker = 1.0 - 0.04 * u.bands.w * (0.5 + 0.5 * sin(u.time * 53.0));
  return vec4f(vec3f(vignette * scan * flicker), 1.0);
}

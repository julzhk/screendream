// A neural culture under a fluorescence microscope (prefixed with common.wgsl at load time).
//   vs_screen/fs_fluid     the culture medium: drifting out-of-focus haze, matrix fibres, bokeh
//   vs_tendril/fs_tendril  one capsule per rope segment, max-blended so joints don't double up
//   vs_sprite/fs_sprite    cell bodies, growth cones, pulses, debris, organelles and bacteria
//                          as procedural billboards (additive)
//   vs_screen/fs_optics    reticle and scale bar (additive)
//   vs_screen/fs_eyepiece  eyepiece vignette and sensor grain (multiply)
// Screen space: x ∈ [-aspect, aspect], y ∈ [-1, 1]. Everything is projected by hand from the
// camera basis, with depth of field: things blur and dim away from the focus distance.
struct Scope {
  cam_pos: vec4f,   // xyz, focal length
  cam_fwd: vec4f,   // xyz, focus distance
  cam_right: vec4f, // xyz, aperture
  cam_up: vec4f,    // xyz, stain phase (cycles 0..3 through three stain sets)
  beats: vec4f,     // per-band onset flash: bass, low-mid, high-mid, treble
};

@group(1) @binding(0) var<uniform> s: Scope;

const NEAR: f32 = 0.2;
const FOG_START: f32 = 17.0;
const FOG_END: f32 = 27.0;

fn to_view(p: vec3f) -> vec3f {
  let rel = p - s.cam_pos.xyz;
  return vec3f(dot(rel, s.cam_right.xyz), dot(rel, s.cam_up.xyz), dot(rel, s.cam_fwd.xyz));
}

fn project(v: vec3f) -> vec2f {
  return v.xy * s.cam_pos.w / v.z;
}

// Radius of the blur circle, in screen units, for something at view depth `z`.
fn blur_at(z: f32) -> f32 {
  return s.cam_right.w * abs(z - s.cam_fwd.w) / z;
}

fn depth_fade(z: f32) -> f32 {
  return (1.0 - smoothstep(FOG_START, FOG_END, z)) * smoothstep(NEAR, 1.0, z);
}

fn clip_of(p: vec2f) -> vec4f {
  return vec4f(p.x / u.aspect, p.y, 0.5, 1.0);
}

// Two fluorophores per stain set, crossfading between sets: GFP and tdTomato, cyan and amber,
// violet and gold. `t` picks between the two.
fn stain_color(t: f32) -> vec3f {
  var a = array<vec3f, 3>(vec3f(0.25, 1.0, 0.45), vec3f(0.2, 0.75, 1.0), vec3f(0.65, 0.4, 1.0));
  var b = array<vec3f, 3>(vec3f(1.0, 0.25, 0.55), vec3f(1.0, 0.6, 0.15), vec3f(1.0, 0.85, 0.3));
  let phase = s.cam_up.w;
  let i = u32(floor(phase)) % 3u;
  let j = (i + 1u) % 3u;
  let k = smoothstep(0.85, 1.0, fract(phase));
  return mix(mix(a[i], b[i], t), mix(a[j], b[j], t), k);
}

// ---------------------------------------------------------------- full-screen passes

struct ScreenOut {
  @builtin(position) pos: vec4f,
  @location(0) p: vec2f,
};

@vertex
fn vs_screen(@builtin(vertex_index) vi: u32) -> ScreenOut {
  let c = fullscreen_pos(vi);
  var o: ScreenOut;
  o.pos = vec4f(c, 0.0, 1.0);
  o.p = vec2f(c.x * u.aspect, c.y);
  return o;
}

@fragment
fn fs_fluid(in: ScreenOut) -> @location(0) vec4f {
  let dir = normalize(s.cam_fwd.xyz * s.cam_pos.w + s.cam_right.xyz * in.p.x + s.cam_up.xyz * in.p.y);
  // Coordinates fixed to the world's directions, so the haze turns as the camera rolls.
  let w = vec2f(atan2(dir.x, -dir.z), asin(clamp(dir.y, -1.0, 1.0)));
  let t = u.time;
  let haze = fbm(w * 2.5 + vec2f(t * 0.012, -t * 0.008));
  let haze2 = fbm(w * 5.3 + vec2f(haze * 1.8, t * 0.02));
  let tint = stain_color(0.15);
  var col = vec3f(0.006, 0.01, 0.02) + tint * 0.025 * haze + vec3f(0.02, 0.03, 0.05) * pow(haze2, 3.0) * 2.0;

  // Extracellular matrix: faint ridged fibres.
  let ridge = 1.0 - abs(noise(w * 9.0 + vec2f(haze2 * 3.0, t * 0.03)) * 2.0 - 1.0);
  col += tint * pow(ridge, 12.0) * 0.03;

  // Out-of-focus specks far behind the culture: soft bokeh discs.
  let g = w * 26.0 + vec2f(t * 0.05, 0.0);
  let cell = floor(g);
  let h = hash2(cell);
  if (h > 0.9) {
    let off = vec2f(hash2(cell + 7.0), hash2(cell + 13.0)) - 0.5;
    let d = length(fract(g) - 0.5 - off * 0.4);
    let disc = smoothstep(0.38, 0.3, d) * (0.6 + 0.4 * smoothstep(0.25, 0.36, d));
    col += mix(tint, vec3f(0.6, 0.7, 1.0), hash2(cell + 3.0)) * disc * (h - 0.9) * 0.35;
  }

  // The bass throbs through the whole medium.
  col *= 1.0 + 0.6 * s.beats.x;
  return idle_fade(col);
}

@fragment
fn fs_optics(in: ScreenOut) -> @location(0) vec4f {
  let p = in.p;
  var v = 0.0;
  // Reticle: a small crosshair of tick marks around the centre.
  let ax = abs(p);
  let along = max(ax.x, ax.y);
  let across = min(ax.x, ax.y);
  if (along > 0.03 && along < 0.09) {
    v += (1.0 - smoothstep(0.0012, 0.0024, across)) * 0.25;
  }
  // Scale bar with end ticks in the bottom-right corner.
  let bar = p - vec2f(u.aspect - 0.3, -0.86);
  let in_bar = step(abs(bar.x), 0.12) * (1.0 - smoothstep(0.003, 0.005, abs(bar.y)));
  let ticks = step(abs(abs(bar.x) - 0.12), 0.003) * step(abs(bar.y), 0.018);
  v += (in_bar + ticks) * 0.5;
  return vec4f(vec3f(0.8, 0.9, 1.0) * v * u.live, 1.0);
}

@fragment
fn fs_eyepiece(in: ScreenOut) -> @location(0) vec4f {
  // Round eyepiece: corners fall away into darkness.
  let r = length(in.p) / max(u.aspect, 1.0);
  var shade = 1.0 - 0.88 * smoothstep(0.72, 1.08, r);
  // Sensor grain.
  let grain = hash2(floor(in.pos.xy) + vec2f(floor(u.time * 24.0) * 17.0, 0.0));
  shade *= 1.0 - 0.07 * grain;
  return vec4f(vec3f(shade), 1.0);
}

// ---------------------------------------------------------------- tendrils

struct TendrilIn {
  @location(0) a: vec4f, // start point, radius
  @location(1) b: vec4f, // end point, radius
  @location(2) g: vec4f, // glow at start, glow at end, arc length at start, at end
  @location(3) c: vec4f, // stain at start, stain at end, fade, seed
};

struct TendrilOut {
  @builtin(position) pos: vec4f,
  @location(0) p: vec2f,
  @location(1) ends: vec4f,  // screen start, screen end
  @location(2) radii: vec4f, // radius at start, at end, blur, fade
  @location(3) g: vec4f,
  @location(4) stain: vec2f,
};

@vertex
fn vs_tendril(@builtin(vertex_index) vi: u32, in: TendrilIn) -> TendrilOut {
  var o: TendrilOut;
  var v0 = to_view(in.a.xyz);
  var v1 = to_view(in.b.xyz);
  if ((v0.z < NEAR && v1.z < NEAR) || in.c.z <= 0.001) {
    o.pos = vec4f(2.0, 2.0, 2.0, 1.0);
    return o;
  }
  // Clip the segment to the near plane.
  if (v0.z < NEAR) { v0 = mix(v0, v1, (NEAR - v0.z) / (v1.z - v0.z)); }
  if (v1.z < NEAR) { v1 = mix(v1, v0, (NEAR - v1.z) / (v0.z - v1.z)); }
  let s0 = project(v0);
  let s1 = project(v1);
  let r0 = in.a.w * s.cam_pos.w / v0.z;
  let r1 = in.b.w * s.cam_pos.w / v1.z;
  let z = 0.5 * (v0.z + v1.z);
  let blur = blur_at(z);
  // Room for varicosities, the halo and the blur.
  let reach = max(r0, r1) * 4.0 + blur * 2.0 + 0.004;
  let d = s1 - s0;
  let len = length(d);
  var dir = vec2f(1.0, 0.0);
  if (len > 1e-6) { dir = d / len; }
  let n = vec2f(-dir.y, dir.x);
  let corners = array<vec2f, 6>(vec2f(-1.0, -1.0), vec2f(1.0, -1.0), vec2f(-1.0, 1.0),
                                vec2f(1.0, -1.0), vec2f(1.0, 1.0), vec2f(-1.0, 1.0));
  let c = corners[vi];
  let base = select(s0, s1, c.x > 0.0);
  let p = base + dir * c.x * reach + n * c.y * reach;
  o.pos = clip_of(p);
  o.p = p;
  o.ends = vec4f(s0, s1);
  o.radii = vec4f(r0, r1, blur, in.c.z * depth_fade(z));
  o.g = in.g;
  o.stain = in.c.xy;
  return o;
}

@fragment
fn fs_tendril(in: TendrilOut) -> @location(0) vec4f {
  let aa = length(fwidth(in.p));
  let s0 = in.ends.xy;
  let s1 = in.ends.zw;
  let pa = in.p - s0;
  let ba = s1 - s0;
  let h = clamp(dot(pa, ba) / max(dot(ba, ba), 1e-10), 0.0, 1.0);
  let dist = length(pa - ba * h);
  let arc = mix(in.g.z, in.g.w, h);
  // Varicosities: beads of swelling along the neurite.
  let bead = pow(0.5 + 0.5 * sin(arc * 13.0), 8.0);
  let r = mix(in.radii.x, in.radii.y, h) * (1.0 + 0.6 * bead);
  let blur = in.radii.z;
  let soft = blur + aa;
  let x = dist / max(r, 1e-6);
  // Membrane-bound dye: the tube's edges glow a little brighter than its core.
  let body = 1.0 - smoothstep(r - soft, r + soft, dist);
  let fill = body * (0.55 + 0.45 * smoothstep(0.2, 1.0, x));
  let halo = exp(-pow(dist / (r * 3.0 + blur * 1.5), 2.0)) * 0.3;
  let glow = mix(in.g.x, in.g.y, h);
  let col = stain_color(mix(in.stain.x, in.stain.y, h));
  var rgb = col * (fill * 0.75 + halo) + (vec3f(0.7) + col * 0.6) * glow * (body + halo * 2.5);
  // Blurring spreads the same light over a wider area.
  rgb *= in.radii.w * r / (r + blur);
  return idle_fade(rgb);
}

// ---------------------------------------------------------------- sprites

const KIND_SOMA: u32 = 0u;
const KIND_FLOWER: u32 = 1u;
const KIND_PULSE: u32 = 2u;
const KIND_DEBRIS: u32 = 3u;
const KIND_VESICLE: u32 = 4u;
const KIND_MITOCHONDRION: u32 = 5u;
const KIND_BACILLUS: u32 = 6u;
const KIND_COCCUS: u32 = 7u;
const KIND_SPIRILLUM: u32 = 8u;
const KIND_VIBRIO: u32 = 9u;
const KIND_AMOEBA: u32 = 10u;
const KIND_LIPID: u32 = 11u;

struct SpriteIn {
  @location(0) a: vec4f, // centre, size (world units per local unit)
  @location(1) b: vec4f, // axis, kind
  @location(2) c: vec4f, // per-kind parameters
  @location(3) d: vec4f, // colour, fade
};

struct SpriteOut {
  @builtin(position) pos: vec4f,
  @location(0) q: vec2f, // local coordinates, x along the projected axis
  @location(1) @interpolate(flat) kind: u32,
  @location(2) c: vec4f,
  @location(3) d: vec4f,
  // How directly the axis points at the camera, foreshortening, blur in local units.
  @location(4) view: vec3f,
};

// Half-extent of each kind's quad in local units: room for tails, petals and halos.
fn extent(kind: u32) -> vec2f {
  switch (kind) {
    case 0u, 1u: { return vec2f(2.4, 2.4); }
    case 2u: { return vec2f(1.8, 1.8); }
    case 5u: { return vec2f(1.9, 1.0); }
    case 6u, 9u: { return vec2f(3.4, 1.4); }
    case 7u: { return vec2f(2.0, 1.0); }
    case 8u: { return vec2f(2.8, 1.1); }
    case 10u: { return vec2f(2.0, 2.0); }
    default: { return vec2f(1.4, 1.4); }
  }
}

@vertex
fn vs_sprite(@builtin(vertex_index) vi: u32, in: SpriteIn) -> SpriteOut {
  var o: SpriteOut;
  let v = to_view(in.a.xyz);
  if (v.z < NEAR * 2.0 || in.d.w <= 0.001) {
    o.pos = vec4f(2.0, 2.0, 2.0, 1.0);
    return o;
  }
  let kind = u32(in.b.w);
  let centre = project(v);
  let scale = in.a.w * s.cam_pos.w / v.z;
  let tip = to_view(in.a.xyz + in.b.xyz * in.a.w);
  var dir = project(vec3f(tip.xy, max(tip.z, NEAR))) - centre;
  if (length(dir) < 1e-6) { dir = vec2f(1.0, 0.0); }
  dir = normalize(dir);
  let n = vec2f(-dir.y, dir.x);
  let facing = abs(dot(normalize(in.b.xyz), normalize(in.a.xyz - s.cam_pos.xyz)));
  let blur = blur_at(v.z) / max(scale, 1e-6);
  let ext = extent(kind) + vec2f(blur * 1.5);
  let corners = array<vec2f, 6>(vec2f(-1.0, -1.0), vec2f(1.0, -1.0), vec2f(-1.0, 1.0),
                                vec2f(1.0, -1.0), vec2f(1.0, 1.0), vec2f(-1.0, 1.0));
  let q = corners[vi] * ext;
  o.pos = clip_of(centre + (dir * q.x + n * q.y) * scale);
  o.q = q;
  o.kind = kind;
  o.c = in.c;
  o.d = vec4f(in.d.rgb, in.d.w * depth_fade(v.z));
  o.view = vec3f(facing, clamp(sqrt(1.0 - facing * facing), 0.3, 1.0), blur);
  return o;
}

fn capsule(q: vec2f, half_len: f32, r: f32) -> f32 {
  return length(q - vec2f(clamp(q.x, -half_len, half_len), 0.0)) - r;
}

fn segment(q: vec2f, a: vec2f, b: vec2f) -> f32 {
  let pa = q - a;
  let ba = b - a;
  return length(pa - ba * clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0));
}

// Distance to a whipping flagellum attached at x = `root` and trailing back to x = `end`:
// its wave grows towards the free end and travels backwards along it.
fn flagellum(q: vec2f, root: vec2f, end: f32, amp: f32, k: f32, phase: f32) -> f32 {
  let t = (root.x - q.x) / (root.x - end);
  if (t < 0.0 || t > 1.0) { return 1e3; }
  let along = root.x - q.x;
  let a = amp * t;
  let y = root.y + a * sin(along * k - phase);
  let slope = a * k * cos(along * k - phase);
  return abs(q.y - y) / sqrt(1.0 + slope * slope) - 0.045 * (1.0 - 0.6 * t);
}

// Fluorescent body: a dim fill and a bright membrane edge.
fn shade(d: f32, aa: f32, rim: f32) -> f32 {
  let inside = 1.0 - smoothstep(-aa, aa, d);
  let edge = 1.0 - smoothstep(0.0, rim + aa, abs(d));
  return inside * 0.3 + edge * 0.8;
}

fn line(d: f32, aa: f32) -> f32 {
  return 1.0 - smoothstep(0.0, aa * 1.5 + 0.01, d);
}

@fragment
fn fs_sprite(in: SpriteOut) -> @location(0) vec4f {
  let aa0 = length(fwidth(in.q));
  let blur = in.view.z;
  let aa = aa0 + blur;
  let q = in.q;
  let rr = length(q);
  let theta = atan2(q.y, q.x);
  // Elongated bodies shorten when their axis tips towards the camera.
  let qb = vec2f(q.x / in.view.y, q.y);
  var rgb = vec3f(0.0);

  switch (in.kind) {
    case 0u: { // cell body: wobbling membrane, dendrite stubs, nucleus, calcium flash
      let glow = in.c.x;
      let ph = in.c.y;
      let seed = in.c.z;
      let col = stain_color(in.c.w);
      var radius = 1.0 + 0.07 * sin(3.0 * theta + ph) + 0.05 * sin(5.0 * theta - 1.3 * ph + seed * 6.0);
      radius += 0.45 * pow(max(cos(4.0 * theta + seed * 20.0), 0.0), 24.0);
      let d = rr - radius;
      let inside = 1.0 - smoothstep(-aa, aa, d);
      let granules = noise(q * 7.0 + seed * 40.0 + vec2f(ph * 0.2, 0.0));
      rgb += col * (shade(d, aa, 0.08) + inside * granules * 0.25);
      let nuc = length(q - vec2f(0.15, -0.1) * sin(seed * 9.0 + vec2f(0.0, 1.0))) - 0.46;
      let dapi = vec3f(0.3, 0.45, 1.0);
      rgb += dapi * shade(nuc, aa, 0.05) * 0.8;
      let nucleolus = length(q - vec2f(0.2, -0.05)) - 0.12;
      rgb += dapi * (1.0 - smoothstep(-aa, aa, nucleolus)) * 0.6;
      // Firing: the whole cell flashes and a shell of light spreads outwards.
      let flash = in.d.rgb; // colour of the band that fired it
      let shell = rr - (1.0 + (1.0 - glow) * 1.3);
      rgb += (vec3f(0.8) + flash) * glow * (inside * 1.2 + exp(-rr * rr * 0.5) * 0.4);
      rgb += flash * glow * exp(-shell * shell * 30.0) * 0.6;
      rgb += col * exp(-rr * rr * 0.6) * 0.06;
    }
    case 1u: { // growth cone: a filopodia-fringed bud that blooms into a flower near contact
      let open = in.c.x;
      let ph = in.c.y;
      let flash = in.c.z;
      let col = stain_color(in.c.w);
      // Face-on: radial petals.
      let petal = 0.32 + open * 1.25 * pow(abs(cos(3.0 * theta + ph * 0.15)), 0.6 + (1.0 - open) * 2.0);
      let d_face = rr - petal;
      // Side-on: a closed teardrop that splays into three petals from its base.
      let bud = length(qb / vec2f(0.75, 0.42)) - 1.0;
      var d_side = bud * 0.42;
      let spread = open * 1.1;
      for (var k = -1; k <= 1; k++) {
        let ang = f32(k) * spread;
        let cs = vec2f(cos(ang), sin(ang));
        let rel = qb - vec2f(-0.5, 0.0);
        let local = vec2f(dot(rel, cs), dot(rel, vec2f(-cs.y, cs.x))) - vec2f(0.6 + 0.3 * open, 0.0);
        let lobe = (length(local / vec2f(0.65 + 0.35 * open, 0.26)) - 1.0) * 0.26;
        d_side = mix(d_side, min(d_side, lobe), open);
      }
      let d = mix(d_side, d_face, in.view.x);
      let inside = 1.0 - smoothstep(-aa, aa, d);
      let veins = pow(abs(cos(3.0 * theta + ph * 0.15)), 10.0) * inside * open;
      rgb += col * (shade(d, aa, 0.06) * 1.2 + veins * 0.4);
      // Filopodia: fine feelers that sweep about while it is still a bud.
      var feelers = 0.0;
      for (var k = 0; k < 5; k++) {
        let ang = in.c.w * 30.0 + f32(k) * 1.26 + 0.25 * sin(ph + f32(k) * 2.0);
        let len = 1.5 + 0.5 * sin(ph * 0.7 + f32(k));
        feelers += line(segment(q, vec2f(0.0), vec2f(cos(ang), sin(ang)) * len) - 0.03, aa);
      }
      rgb += col * min(feelers, 1.0) * (1.0 - open) * 0.6;
      // A glowing pistil, and a burst of light on contact.
      rgb += mix(vec3f(1.0, 0.9, 0.5), col, 0.3) * exp(-rr * rr * 18.0) * (0.3 + open);
      rgb += vec3f(1.0) * flash * (inside + exp(-rr * rr * 0.8) * 0.8);
    }
    case 2u: { // impulse travelling the network
      let e = in.c.x;
      rgb = in.d.rgb * exp(-rr * rr * 2.5) * e * 1.3 + vec3f(1.0) * exp(-rr * rr * 22.0) * e;
    }
    case 3u: { // speck of debris or transmitter
      rgb = mix(stain_color(in.c.w), vec3f(1.0), 0.4) * exp(-rr * rr * 3.0) * in.c.x;
    }
    case 4u: { // vesicle: a membrane bubble with cargo specks inside
      let d = rr - 1.0;
      rgb = in.d.rgb * shade(d, aa, 0.07) * 0.8;
      let speck = noise(q * 5.0 + in.c.y * 30.0);
      rgb += in.d.rgb * smoothstep(0.7, 0.8, speck) * step(rr, 0.85) * 0.4;
    }
    case 5u: { // mitochondrion: a capsule with folded cristae
      let d = capsule(qb, 0.95, 0.5);
      let inner = 1.0 - smoothstep(-0.12 - aa, -0.12 + aa, d);
      let cristae = smoothstep(0.7, 0.95, abs(sin(qb.x * 6.0 + sin(qb.y * 5.0 + in.c.x * 0.5) * 0.9)));
      rgb = in.d.rgb * (shade(d, aa, 0.06) + cristae * inner * 0.5) * 0.8;
    }
    case 6u: { // bacillus: a rod pushed along by a bundle of flagella
      let body = capsule(qb - vec2f(0.6, 0.0), 0.75, 0.36);
      rgb = in.d.rgb * shade(body, aa, 0.06);
      var tails = 0.0;
      for (var k = 0; k < 3; k++) {
        let off = (f32(k) - 1.0) * 0.18;
        tails += line(flagellum(qb, vec2f(-0.4, off), -3.2, 0.35, 3.0, in.c.x + f32(k) * 2.1), aa);
      }
      rgb += in.d.rgb * min(tails, 1.0) * 0.5;
    }
    case 7u: { // coccus chain
      var d = 1e3;
      for (var k = 0; k < 4; k++) {
        let x = (f32(k) - 1.5) * 0.78;
        let y = 0.1 * sin(in.c.x * 0.7 + f32(k) * 1.7);
        d = min(d, length(qb - vec2f(x, y)) - 0.38);
      }
      rgb = in.d.rgb * shade(d, aa, 0.06);
    }
    case 8u: { // spirillum: a corkscrew with flagella tufts at both ends
      let xc = clamp(qb.x, -1.8, 1.8);
      let y = 0.35 * sin(xc * 3.2 + in.c.x * 0.5);
      let slope = 0.35 * 3.2 * cos(xc * 3.2 + in.c.x * 0.5);
      let d = length(vec2f(qb.x - xc, (qb.y - y) / sqrt(1.0 + slope * slope))) - 0.15;
      rgb = in.d.rgb * shade(d, aa, 0.05);
      let tuft_l = flagellum(qb, vec2f(-1.8, 0.35 * sin(-1.8 * 3.2 + in.c.x * 0.5)), -2.7, 0.25, 6.0, in.c.x);
      let tuft_r = flagellum(vec2f(-qb.x, qb.y), vec2f(-1.8, 0.35 * sin(1.8 * 3.2 + in.c.x * 0.5)), -2.7, 0.25, 6.0, in.c.x + 1.0);
      rgb += in.d.rgb * (line(tuft_l, aa) + line(tuft_r, aa)) * 0.5;
    }
    case 9u: { // vibrio: a comma with one long whipping flagellum
      let bent = vec2f(qb.x - 0.5, qb.y + 0.32 * (qb.x - 0.5) * (qb.x - 0.5) - 0.15);
      let d = capsule(bent, 0.7, 0.3);
      rgb = in.d.rgb * shade(d, aa, 0.05);
      rgb += in.d.rgb * line(flagellum(qb, vec2f(-0.2, 0.0), -3.3, 0.4, 2.6, in.c.x), aa) * 0.55;
    }
    case 10u: { // amoeba: a restless blob reaching out with pseudopods
      let ph = in.c.x;
      let seed = in.c.y;
      var radius = 1.0 + 0.2 * sin(3.0 * theta + ph) + 0.12 * sin(5.0 * theta - ph * 1.7 + seed * 5.0);
      radius += 0.5 * pow(max(cos(2.0 * theta + seed * 9.0 + ph * 0.3), 0.0), 6.0);
      let d = (rr - radius) * 0.8;
      let inside = 1.0 - smoothstep(-aa, aa, d);
      let grain = noise(q * 6.0 + vec2f(ph * 0.3, seed * 20.0));
      rgb = in.d.rgb * (shade(d, aa, 0.06) * 0.7 + inside * grain * 0.2);
      // Contractile vacuole, swelling and emptying.
      let vac = length(q - vec2f(-0.3, 0.25)) - (0.18 + 0.08 * sin(ph * 2.0));
      rgb += in.d.rgb * shade(vac, aa, 0.03) * 0.5;
    }
    default: { // lipid droplet: bright and glassy
      let d = rr - 1.0;
      let inside = 1.0 - smoothstep(-aa, aa, d);
      let spec = exp(-dot(q - vec2f(-0.35, 0.35), q - vec2f(-0.35, 0.35)) * 12.0);
      rgb = in.d.rgb * (inside * 0.45 + shade(d, aa, 0.05) * 0.4) + vec3f(1.0) * spec * inside * 0.5;
    }
  }
  // Blur spreads the light; fog and fades dim it.
  rgb *= in.d.w / (1.0 + 1.5 * blur);
  return idle_fade(rgb);
}

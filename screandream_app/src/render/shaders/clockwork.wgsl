// A clockwork mechanism under moving lamps (prefixed with common.wgsl at load time).
//   vs_screen/fs_plate    the base plate: perlage and Geneva stripes, blued screws
//   vs_jewel/fs_jewel     a ruby bearing on the plate under every axle
//   vs_shadow/fs_shadow   each gear's soft shadow on the layer beneath it (multiply)
//   vs_gear/fs_gear       procedural gears: teeth, spokes, bevels, screws, rivets, brushing,
//                         scratches, oil, lit by lamps, spotlights and a studio environment;
//                         gears fading in or out use the same shader with alpha blending
//   vs_bar/fs_bar         linkage parts: rods, riveted crossbeams, ribbed cylinders, blocks,
//                         bosses and yokes; vs_bar_shadow/fs_bar_shadow cast them (multiply)
//   vs_puff/fs_puff       billows of steam (alpha)
//   vs_screen/fs_steam    drifting steam over the whole mechanism (alpha)
//   vs_spark/fs_spark     sparks, embers and dust motes (additive)
//   vs_screen/fs_glow     lamp glows, anamorphic streaks and lit haze (additive)
//   vs_screen/fs_vignette (multiply)
// World: the camera sits at z = 0 looking towards +z; gears lie in planes of constant z.
// Every quad is parallel to the screen, so it is projected with w = 1 and depth is linear in z.
struct Works {
  cam: vec4f,                    // x, y, roll, focal length
  light_pos: array<vec4f, 5>,    // xyz, spotlight cone cosine (below -1 for lamps)
  light_col: array<vec4f, 5>,    // rgb
  light_dir: array<vec4f, 5>,    // spotlight direction
  misc: vec4f,                   // beat pulse, plate z, top layer z, -
};

@group(1) @binding(0) var<uniform> w: Works;

const LIGHTS: i32 = 5;
const POINT_LAMPS: i32 = 3; // the rest are spotlights
const NEAR: f32 = 0.5;
const FAR: f32 = 10.0;

const KIND_SPOKED: u32 = 0u;
const KIND_HOLED: u32 = 1u;
const KIND_DISC: u32 = 2u;
const KIND_FLYWHEEL: u32 = 3u;
const KIND_PINION: u32 = 4u;
const KIND_BALANCE: u32 = 5u;
const KIND_RATCHET: u32 = 6u;

fn rot(p: vec2f, a: f32) -> vec2f {
  let c = cos(a);
  let s = sin(a);
  return vec2f(c * p.x - s * p.y, s * p.x + c * p.y);
}

fn to_clip(p: vec3f) -> vec4f {
  let rel = rot(p.xy - w.cam.xy, -w.cam.z);
  let s = rel * w.cam.w / p.z;
  return vec4f(s.x / u.aspect, s.y, (p.z - NEAR) / (FAR - NEAR), 1.0);
}

// World point on the plane at depth `z` seen at screen point `s`.
fn unproject(s: vec2f, z: f32) -> vec2f {
  return w.cam.xy + rot(s * z / w.cam.w, w.cam.z);
}

fn camera() -> vec3f {
  return vec3f(w.cam.xy, 0.0);
}

// Corner `i` of a quad drawn as two triangles.
fn corner(i: u32) -> vec2f {
  var c = array<vec2f, 6>(vec2f(-1.0, -1.0), vec2f(1.0, -1.0), vec2f(-1.0, 1.0),
                          vec2f(1.0, -1.0), vec2f(1.0, 1.0), vec2f(-1.0, 1.0));
  return c[i];
}

// ---------------------------------------------------------------- lighting

struct Metal {
  f0: vec3f,
  rough: f32,
  brush: u32, // 0 none, 1 circular, 2 radial
};

fn metal(i: u32) -> Metal {
  switch (i) {
    case 0u: { return Metal(vec3f(0.95, 0.72, 0.38), 0.32, 1u); } // brass
    case 1u: { return Metal(vec3f(1.0, 0.78, 0.34), 0.16, 1u); }  // gold
    case 2u: { return Metal(vec3f(0.96, 0.95, 0.93), 0.14, 1u); } // silver
    case 3u: { return Metal(vec3f(0.6, 0.62, 0.64), 0.42, 2u); }  // brushed steel
    case 4u: { return Metal(vec3f(0.95, 0.58, 0.42), 0.28, 1u); } // copper
    case 5u: { return Metal(vec3f(0.3, 0.31, 0.34), 0.38, 2u); }  // gunmetal
    case 6u: { return Metal(vec3f(0.75, 0.5, 0.28), 0.5, 0u); }   // bronze
    case 7u: { return Metal(vec3f(0.98, 0.68, 0.58), 0.22, 1u); } // rose gold
    case 8u: { return Metal(vec3f(0.78, 0.75, 0.68), 0.3, 2u); }  // nickel
    default: { return Metal(vec3f(0.18, 0.28, 0.75), 0.15, 0u); } // heat-blued steel
  }
}

// A dim workshop with two softboxes drifting overhead, for the metal to reflect.
fn environment(r: vec3f, rough: f32) -> vec3f {
  let t = u.time * 0.05;
  let soft = 0.12 + rough * 0.5;
  let box1 = smoothstep(soft, 0.0, abs(r.x - 0.45 * sin(t)) - 0.1) * smoothstep(-0.3, 0.4, r.y);
  let box2 = smoothstep(soft, 0.0, abs(r.y + 0.35 * cos(t * 1.3)) - 0.05);
  let base = mix(vec3f(0.025, 0.02, 0.018), vec3f(0.1, 0.085, 0.07), r.y * 0.5 + 0.5);
  return base + (vec3f(1.0, 0.88, 0.72) * box1 * 0.9 + vec3f(0.55, 0.7, 1.0) * box2 * 0.5)
    * (1.0 - rough * 0.6);
}

// Light from lamp `i` arriving at `p`: falls off with distance, and spotlights only light
// what lies inside their cone.
fn lamp(i: i32, p: vec3f) -> vec3f {
  let lv = w.light_pos[i].xyz - p;
  var c = w.light_col[i].rgb / (1.0 + dot(lv, lv) * 0.08);
  let cone = w.light_pos[i].w;
  if (cone > -1.5) {
    let along = dot(normalize(-lv), w.light_dir[i].xyz);
    c *= smoothstep(cone - 0.012, cone + 0.006, along);
  }
  return c;
}

// Anisotropic (Ward-style) metal under the lamps and spotlights. `t` is the brushing direction.
fn shade(p: vec3f, n: vec3f, t: vec3f, m: Metal) -> vec3f {
  let v = normalize(camera() - p);
  let b = normalize(cross(n, t));
  var along = m.rough;
  var across = m.rough;
  if (m.brush != 0u) {
    along = m.rough * 0.3;
    across = m.rough * 1.6;
  }
  var col = vec3f(0.0);
  for (var i = 0; i < LIGHTS; i++) {
    let lv = w.light_pos[i].xyz - p;
    let l = normalize(lv);
    let h = normalize(l + v);
    let ndl = max(dot(n, l), 0.0);
    let ndh = max(dot(n, h), 1e-3);
    let ht = dot(h, t) / along;
    let hb = dot(h, b) / across;
    let spec = exp(-(ht * ht + hb * hb) / (ndh * ndh)) / (12.566 * along * across);
    let fresnel = m.f0 + (1.0 - m.f0) * pow(1.0 - max(dot(v, h), 0.0), 5.0);
    col += lamp(i, p) * ndl * (fresnel * spec * 0.22 + m.f0 * 0.05);
  }
  col += m.f0 * environment(reflect(-v, n), m.rough) * 0.7;
  return col;
}

// ---------------------------------------------------------------- gear geometry

struct GearDesc {
  teeth: f32,
  radius: f32,
  kind: u32,
  spokes: f32,
  seed: f32,
  blur: f32, // tooth softening from motion
};

fn module_of(g: GearDesc) -> f32 {
  return select(0.0, 2.0 * g.radius / max(g.teeth, 1.0), g.teeth > 0.5);
}

fn hub_radius(g: GearDesc) -> f32 {
  return clamp(g.radius * 0.2, 0.03, 0.11);
}

fn rim_inner(g: GearDesc) -> f32 {
  let m = module_of(g);
  if (g.kind == KIND_FLYWHEEL) { return g.radius * 0.76; }
  if (g.kind == KIND_BALANCE) { return g.radius * 0.88; }
  return g.radius - 1.25 * m - max(g.radius * 0.12, 0.025);
}

// Signed distance to the gear's metal (negative inside), in its own rotating frame.
fn gear_sdf(q: vec2f, g: GearDesc) -> f32 {
  let r = length(q);
  let a = atan2(q.y, q.x);
  let m = module_of(g);
  var d_out = r - g.radius;
  if (g.kind == KIND_RATCHET) {
    let f = fract(a * g.teeth / TAU);
    let rise = select(mix(1.0, 0.0, (f - 0.88) / 0.12), f / 0.88, f < 0.88);
    d_out = (r - mix(g.radius - m, g.radius + m, rise)) * 0.6;
  } else if (g.teeth > 0.5) {
    let f = abs(fract(a * g.teeth / TAU + 0.5) - 0.5);
    let tooth = smoothstep(0.3 + g.blur, 0.17 - g.blur, f);
    let flank = tooth * (1.0 - tooth) * 4.0;
    d_out = (r - mix(g.radius - 1.25 * m, g.radius + m, tooth)) * mix(1.0, 0.45, flank);
  }

  let rh = hub_radius(g);
  let ri = rim_inner(g);
  var hole = 1e3;
  switch (g.kind) {
    case 0u, 5u, 3u: { // spokes between hub and rim, curved on flywheels
      if (ri > rh * 1.8) {
        let annulus = max(rh * 1.6 - r, r - ri);
        let sector = TAU / g.spokes;
        var b = a;
        var width = max(g.radius * 0.06, 0.012);
        if (g.kind == KIND_FLYWHEEL) {
          b = a - 0.7 * r / ri;
          width = mix(g.radius * 0.1, g.radius * 0.05, r / ri);
        } else if (g.kind == KIND_BALANCE) {
          width = max(g.radius * 0.035, 0.007);
        }
        let ang = b - round(b / sector) * sector;
        let spoke = abs(r * sin(ang)) - width;
        hole = max(annulus, -spoke) + 0.006;
      }
    }
    case 1u, 6u: { // ring of lightening holes
      let k = select(g.spokes + 2.0, 3.0, g.kind == KIND_RATCHET);
      let ring = 0.5 * (rh * 1.5 + ri);
      let sector = TAU / k;
      let hr = min(0.5 * (ri - rh * 1.5) * 0.85, ring * sin(3.14159 / k) * 0.8);
      if (hr > 0.006) {
        let c = round(a / sector) * sector;
        hole = length(q - ring * vec2f(cos(c), sin(c))) - hr;
      }
    }
    default: {}
  }
  return max(d_out, -hole);
}

fn screw_slot(q: vec2f, rs: f32, angle: f32) -> f32 {
  let s = rot(q, angle);
  return step(abs(s.y), rs * 0.16) * step(abs(s.x), rs * 0.8);
}

// Surface height (towards the camera) in world units: bevels, hub boss, recesses, screws.
fn gear_height(q: vec2f, g: GearDesc) -> f32 {
  let d = gear_sdf(q, g);
  let r = length(q);
  let m = module_of(g);
  let bev = select(0.014, clamp(m * 0.35, 0.006, 0.02), g.teeth > 0.5);
  var h = bev * smoothstep(0.0, bev, -d);
  let rh = hub_radius(g);
  let ri = rim_inner(g);
  h += 0.012 * smoothstep(rh + 0.003, rh - 0.003, r);
  if (g.kind == KIND_DISC || g.kind == KIND_PINION) {
    h -= 0.008 * smoothstep(rh * 1.4, rh * 1.4 + 0.004, r) * smoothstep(ri + 0.004, ri, r);
  }
  if (g.kind == KIND_FLYWHEEL) {
    h += 0.01 * smoothstep(ri - 0.004, ri, r);
  }
  // Domed centre screw with its slot.
  let rs = rh * 0.5;
  if (r < rs) {
    h += rs * 0.5 * sqrt(1.0 - (r / rs) * (r / rs)) - 0.006 * screw_slot(q, rs, g.seed * 6.0);
  }
  // Rivets around a flywheel rim.
  if (g.kind == KIND_FLYWHEEL) {
    let k = g.spokes * 2.0;
    let sector = TAU / k;
    let c = round(atan2(q.y, q.x) / sector) * sector;
    let rv = length(q - 0.5 * (ri + g.radius) * vec2f(cos(c), sin(c)));
    let rr = (g.radius - ri) * 0.18;
    h += select(0.0, rr * 0.7 * sqrt(max(1.0 - (rv / rr) * (rv / rr), 0.0)), rv < rr);
  }
  return h;
}

// ---------------------------------------------------------------- gears

struct GearIn {
  @location(0) a: vec4f, // centre, outer radius
  @location(1) b: vec4f, // angle, teeth, pitch radius, kind
  @location(2) c: vec4f, // material, seed, spokes, spin
  @location(3) d: vec4f, // fade, shadow receiver z, -, -
};

struct GearOut {
  @builtin(position) pos: vec4f,
  @location(0) world: vec2f,
  @location(1) centre: vec3f,
  @location(2) b: vec4f,
  @location(3) c: vec4f,
  @location(4) d: vec4f,
};

fn desc(b: vec4f, c: vec4f) -> GearDesc {
  // Fast gears smear their teeth a little rather than strobing.
  let blur = clamp(abs(c.w) * b.y / TAU * 0.012, 0.0, 0.12);
  return GearDesc(b.y, b.z, u32(b.w), c.z, c.y, blur);
}

@vertex
fn vs_gear(@builtin(vertex_index) vi: u32, in: GearIn) -> GearOut {
  var o: GearOut;
  let world = in.a.xy + corner(vi) * in.a.w * 1.04;
  o.pos = to_clip(vec3f(world, in.a.z));
  o.world = world;
  o.centre = in.a.xyz;
  o.b = in.b;
  o.c = in.c;
  o.d = in.d;
  return o;
}

@fragment
fn fs_gear(in: GearOut) -> @location(0) vec4f {
  let px = length(fwidth(in.world));
  let g = desc(in.b, in.c);
  let angle = in.b.x;
  let rel = in.world - in.centre.xy;
  let q = rot(rel, -angle);
  let d = gear_sdf(q, g);
  if (d > 0.0) { discard; }

  // Normal from the height field, sampled in world space.
  let e = max(px * 0.75, 0.0008);
  let h0 = gear_height(q, g);
  let hx = gear_height(rot(rel + vec2f(e, 0.0), -angle), g);
  let hy = gear_height(rot(rel + vec2f(0.0, e), -angle), g);
  let n = normalize(vec3f(-(hx - h0) / e, -(hy - h0) / e, -1.0));
  let p = vec3f(in.world, in.centre.z - h0);

  let r = length(q);
  let a = atan2(q.y, q.x);
  let rh = hub_radius(g);
  let ri = rim_inner(g);
  let m = module_of(g);
  var mat = metal(u32(in.c.x));

  // Brushing: concentric (circular graining) or sunburst (radial).
  var t = normalize(vec3f(-rel.y, rel.x, 0.0) + vec3f(1e-5, 0.0, 0.0));
  var grain = noise(vec2f(r * 900.0, g.seed * 50.0));
  if (mat.brush == 2u) {
    t = normalize(vec3f(rel, 0.0) + vec3f(1e-5, 0.0, 0.0));
    grain = noise(vec2f(a * 600.0 / TAU * 3.0, g.seed * 50.0 + r * 4.0));
  }
  mat.rough *= 0.8 + 0.4 * grain;

  // Tooth tips are polished bright by contact.
  let tip = select(0.0, smoothstep(g.radius - 0.2 * m, g.radius + 0.6 * m, r), g.teeth > 0.5);
  mat.rough *= 1.0 - 0.5 * tip;

  // Scratches: fine straight marks across the face.
  var scratch = 0.0;
  for (var k = 0; k < 5; k++) {
    let hk = vec2f(g.seed * 91.7 + f32(k) * 13.1, f32(k) * 7.3);
    let start = (vec2f(hash2(hk), hash2(hk + 1.0)) - 0.5) * 2.0 * g.radius;
    let dir_a = hash2(hk + 2.0) * TAU;
    let len = g.radius * (0.2 + 0.6 * hash2(hk + 3.0));
    let dir = vec2f(cos(dir_a), sin(dir_a));
    let along = clamp(dot(q - start, dir), 0.0, len);
    let dist = length(q - start - dir * along);
    scratch += exp(-pow(dist / max(0.0012, px * 0.6), 2.0)) * 0.7;
  }
  scratch = min(scratch, 1.0);

  // Screws are heat-blued steel.
  let rs = rh * 0.5;
  if (r < rs) {
    mat = metal(99u);
  }

  var col = shade(p, n, t, mat) * (0.85 + 0.3 * grain);
  col += mat.f0 * scratch * 0.12;
  col *= 1.0 + 0.4 * tip;
  if (r < rs) {
    col *= 1.0 - 0.8 * screw_slot(q, rs, g.seed * 6.0);
  }

  // Engraved graduations on big wheels.
  if (g.radius > 0.42 && g.kind != KIND_FLYWHEEL) {
    let ring = ri - 0.02;
    let tick = abs(fract(a * 60.0 / TAU) - 0.5);
    let long = select(0.008, 0.016, abs(fract(a * 12.0 / TAU) - 0.5) > 0.47);
    let mark = step(0.45, tick) * step(abs(r - ring), long);
    col *= 1.0 - 0.6 * mark;
  }
  // A ring of oil round the hub, with a thin-film sheen.
  let oil = smoothstep(rh * 1.0, rh * 1.15, r) * smoothstep(rh * 1.6, rh * 1.25, r);
  let film = 0.5 + 0.5 * cos(TAU * (vec3f(0.0, 0.33, 0.67) + r * 80.0 + noise(q * 40.0)));
  col = col * (1.0 - 0.3 * oil) + film * oil * 0.05;
  // Verdigris in bronze's flat recesses.
  if (u32(in.c.x) == 6u) {
    let patina = smoothstep(0.55, 0.75, noise(q * 30.0 + g.seed * 20.0)) * smoothstep(0.004, 0.02, -d);
    col = mix(col, vec3f(0.12, 0.28, 0.22), patina * 0.6);
  }
  // Shadowed edges give the silhouette its weight.
  col *= 0.55 + 0.45 * smoothstep(0.0, 0.025, -d);
  // Opacity: only arriving and departing gears are drawn blended, fading in and out.
  return vec4f(idle_fade(col).rgb, in.d.x);
}

// ---------------------------------------------------------------- shadows

@vertex
fn vs_shadow(@builtin(vertex_index) vi: u32, in: GearIn) -> GearOut {
  var o: GearOut;
  // Cast from the warm key lamp onto the plane below.
  let lamp = w.light_pos[0].xyz;
  let receiver = in.d.y;
  let s = (receiver - lamp.z) / max(in.a.z - lamp.z, 0.05);
  let centre = lamp.xy + (in.a.xy - lamp.xy) * s;
  let soft = (receiver - in.a.z) * 0.35;
  let world = centre + corner(vi) * (in.a.w * s + soft * 2.0);
  o.pos = to_clip(vec3f(world, receiver - 0.01));
  o.world = world;
  o.centre = vec3f(centre, s);
  o.b = in.b;
  o.c = in.c;
  o.d = vec4f(in.d.x, soft, receiver - in.a.z, 0.0);
  return o;
}

@fragment
fn fs_shadow(in: GearOut) -> @location(0) vec4f {
  let g = desc(in.b, in.c);
  let s = in.centre.z;
  let q = rot((in.world - in.centre.xy) / s, -in.b.x);
  let d = gear_sdf(q, g);
  let soft = max(in.d.y / s, 0.004);
  let height = in.d.z;
  let strength = (0.15 + 0.55 * (1.0 - smoothstep(0.0, 2.5, height))) * in.d.x;
  let shadow = smoothstep(soft, -soft, d) * strength;
  return vec4f(vec3f(1.0 - shadow), 1.0);
}

// ---------------------------------------------------------------- base plate and jewels

struct ScreenOut {
  @builtin(position) pos: vec4f,
  @location(0) p: vec2f,
};

@vertex
fn vs_screen(@builtin(vertex_index) vi: u32) -> ScreenOut {
  let c = fullscreen_pos(vi);
  var o: ScreenOut;
  o.pos = vec4f(c, (w.misc.y - NEAR) / (FAR - NEAR), 1.0);
  o.p = vec2f(c.x * u.aspect, c.y);
  return o;
}

@fragment
fn fs_plate(in: ScreenOut) -> @location(0) vec4f {
  let z = w.misc.y;
  let world = unproject(in.p, z);
  var mat = Metal(vec3f(0.5, 0.48, 0.46), 0.35, 1u);

  // Perlage: overlapping circular graining spots.
  let spacing = 0.09;
  let cell = floor(world / spacing);
  var best = -1.0;
  var centre = world;
  for (var j = -1; j <= 1; j++) {
    for (var i = -1; i <= 1; i++) {
      let c = cell + vec2f(f32(i), f32(j));
      let cp = (c + 0.5 + (vec2f(hash2(c), hash2(c + 3.1)) - 0.5) * 0.3) * spacing;
      let prio = hash2(c + 7.7);
      if (length(world - cp) < spacing * 0.8 && prio > best) {
        best = prio;
        centre = cp;
      }
    }
  }
  var rel = world - centre;
  var t = normalize(vec3f(-rel.y, rel.x, 0.0) + vec3f(1e-5, 0.0, 0.0));
  var grain = noise(vec2f(length(rel) * 600.0, best * 40.0));
  // Bands of Geneva stripes across parts of the plate.
  let stripes = smoothstep(0.55, 0.6, noise(world * 0.5 + 3.0));
  if (stripes > 0.5) {
    let dir = normalize(vec2f(1.0, 0.35));
    let across = dot(world, vec2f(-dir.y, dir.x));
    t = vec3f(dir, 0.0);
    grain = 0.5 + 0.5 * sin(across * 70.0);
    mat.f0 = vec3f(0.62, 0.56, 0.45);
  }
  var n = vec3f(0.0, 0.0, -1.0);
  var col = vec3f(0.0);

  // Blued screws, countersunk into the plate.
  let screw_cell = floor(world / 0.55);
  let screw_at = (screw_cell + 0.5) * 0.55;
  let sr = length(world - screw_at);
  let screw = hash2(screw_cell + 11.0) > 0.55 && sr < 0.04;
  if (screw) {
    let dome = sr / 0.032;
    n = normalize(vec3f((world - screw_at) * 2.0 * step(dome, 1.0) / 0.032, -1.0));
    mat = metal(99u);
  }
  col = shade(vec3f(world, z), n, t, mat) * (0.8 + 0.4 * grain);
  if (screw) {
    col *= 1.0 - 0.8 * screw_slot(world - screw_at, 0.032, hash2(screw_cell) * TAU);
    col *= smoothstep(0.04, 0.034, sr) * 0.7 + 0.3 * step(sr, 0.033);
  }
  return idle_fade(col * 0.6);
}

@vertex
fn vs_jewel(@builtin(vertex_index) vi: u32, in: GearIn) -> GearOut {
  var o: GearOut;
  let size = clamp(in.b.z * 0.08, 0.012, 0.03);
  let world = in.a.xy + corner(vi) * size;
  o.pos = to_clip(vec3f(world, w.misc.y - 0.005));
  o.world = world;
  o.centre = vec3f(in.a.xy, size);
  return o;
}

@fragment
fn fs_jewel(in: GearOut) -> @location(0) vec4f {
  let q = (in.world - in.centre.xy) / in.centre.z;
  let r = length(q);
  if (r > 1.0) { discard; }
  let n = normalize(vec3f(q * 0.9, -1.0));
  let ruby = Metal(vec3f(0.85, 0.05, 0.12), 0.1, 0u);
  var col = shade(vec3f(in.world, w.misc.y), n, vec3f(1.0, 0.0, 0.0), ruby) * 1.5;
  col += vec3f(0.5, 0.02, 0.06) * (1.0 - r) * 0.4; // light caught inside the stone
  col *= smoothstep(0.35, 0.45, r) * 0.6 + 0.4;     // the pivot hole
  return idle_fade(col);
}


// ---------------------------------------------------------------- linkages

const BAR_ROD: u32 = 0u;
const BAR_TRUSS: u32 = 1u;
const BAR_CYLINDER: u32 = 2u;
const BAR_BLOCK: u32 = 3u;
const BAR_BOSS: u32 = 4u;
const BAR_YOKE: u32 = 5u;

struct BarIn {
  @location(0) a: vec4f, // start, z, width
  @location(1) b: vec4f, // end, style, material
  @location(2) c: vec4f, // seed, -, -, -
};

struct BarOut {
  @builtin(position) pos: vec4f,
  @location(0) world: vec2f,
  @location(1) centre: vec2f,
  @location(2) dir: vec2f,
  // half length, width, z, seed
  @location(3) shape: vec4f,
  @location(4) @interpolate(flat) style: vec2u, // style, material
  @location(5) shadow: vec4f, // shadow projection: lamp xy, scale, softness
};

fn rounded_box(q: vec2f, half: vec2f, r: f32) -> f32 {
  let d = abs(q) - half + vec2f(r);
  return length(max(d, vec2f(0.0))) + min(max(d.x, d.y), 0.0) - r;
}

// Signed distance to a linkage part, in its own frame (x along it, centred).
fn bar_sdf(q: vec2f, half: f32, width: f32, style: u32) -> f32 {
  switch (style) {
    case 0u: { // dog-bone rod with eyes at both ends
      let eye = smoothstep(half - width * 1.6, half - width * 0.3, abs(q.x));
      let r = width * (0.5 + 0.5 * eye);
      return length(q - vec2f(clamp(q.x, -half, half), 0.0)) - r;
    }
    case 1u: { // riveted truss with diamond cut-outs
      var d = rounded_box(q, vec2f(half + width * 0.6, width), width * 0.4);
      let cell = width * 2.4;
      let x = q.x - (round(q.x / cell)) * cell;
      let diamond = abs(x) / (cell * 0.36) + abs(q.y) / (width * 0.58) - 1.0;
      let inner = step(abs(q.x), half - width * 0.6) * step(width * 1.3, abs(q.x));
      if (inner > 0.5) { d = max(d, -diamond * width * 0.5); }
      return d;
    }
    case 2u: { return rounded_box(q, vec2f(half, width), width * 0.3); }
    case 3u: { return rounded_box(q, vec2f(half + width * 0.3, width * 0.75), 0.008); }
    case 4u: { return length(q) - width; }
    default: { // slotted yoke
      let body = rounded_box(q, vec2f(half + width * 0.3, width * 0.6), width * 0.3);
      let slot = rounded_box(q, vec2f(half - width * 0.2, width * 0.22), width * 0.2);
      return max(body, -slot);
    }
  }
}

fn dome(q: vec2f, c: vec2f, r: f32) -> f32 {
  let d = length(q - c) / r;
  return select(0.0, r * 0.6 * sqrt(1.0 - d * d), d < 1.0);
}

// Height towards the camera: tubes for rods and cylinders, bevels, ribs, rivets and bolts.
fn bar_height(q: vec2f, half: f32, width: f32, style: u32) -> f32 {
  let d = bar_sdf(q, half, width, style);
  let bev = clamp(width * 0.25, 0.004, 0.012);
  var h = bev * smoothstep(0.0, bev, -d);
  switch (style) {
    case 0u: {
      // A round rod, with domed pins through its eyes.
      let r = width * (0.5 + 0.5 * smoothstep(half - width * 1.6, half - width * 0.3, abs(q.x)));
      h = r * 0.5 * sqrt(max(1.0 - (q.y / r) * (q.y / r), 0.0)) * smoothstep(0.0, 0.003, -d);
      h += dome(q, vec2f(half, 0.0), width * 0.5) + dome(q, vec2f(-half, 0.0), width * 0.5);
    }
    case 1u: {
      let cell = width * 2.4;
      let x = q.x - round(q.x / (cell * 0.5)) * cell * 0.5;
      h += dome(vec2f(x, abs(q.y)), vec2f(0.0, width * 0.8), width * 0.12);
    }
    case 2u: {
      // Ribbed tube with bolted flanges at each end.
      let tube = width * 0.5 * sqrt(max(1.0 - (q.y / width) * (q.y / width), 0.0));
      let rib = 0.004 * smoothstep(0.6, 0.9, abs(sin(q.x * 110.0)));
      let flange = step(half - 0.025, abs(q.x));
      h = (tube + rib * (1.0 - flange) + flange * 0.008) * smoothstep(0.0, 0.004, -d);
      let bolt_x = sign(q.x) * (half - 0.0125);
      h += dome(vec2f(q.x, abs(q.y)), vec2f(bolt_x, width * 0.75), 0.008);
    }
    case 3u: {
      let c = vec2f(half + width * 0.3 - 0.012, width * 0.75 - 0.012);
      h += dome(abs(q), c, 0.007);
    }
    case 4u: {
      h += dome(q, vec2f(0.0), width * 0.45);
    }
    default: {}
  }
  return h;
}

fn bar_local(world: vec2f, centre: vec2f, dir: vec2f) -> vec2f {
  let rel = world - centre;
  return vec2f(dot(rel, dir), dot(rel, vec2f(-dir.y, dir.x)));
}

fn bar_frame(in: BarIn) -> vec4f {
  let d = in.b.xy - in.a.xy;
  let len = length(d);
  var dir = vec2f(1.0, 0.0);
  if (len > 1e-5) { dir = d / len; }
  return vec4f(dir, len * 0.5, 0.0);
}

fn bar_vertex(vi: u32, in: BarIn) -> BarOut {
  var o: BarOut;
  let f = bar_frame(in);
  let dir = f.xy;
  let half = f.z;
  let width = in.a.w;
  let centre = 0.5 * (in.a.xy + in.b.xy);
  let c = corner(vi);
  let world = centre + dir * c.x * (half + width * 1.3) + vec2f(-dir.y, dir.x) * c.y * width * 1.3;
  o.pos = to_clip(vec3f(world, in.a.z));
  o.world = world;
  o.centre = centre;
  o.dir = dir;
  o.shape = vec4f(half, width, in.a.z, in.c.x);
  o.style = vec2u(u32(in.b.z), u32(in.b.w));
  return o;
}

@vertex
fn vs_bar(@builtin(vertex_index) vi: u32, in: BarIn) -> BarOut {
  return bar_vertex(vi, in);
}

@fragment
fn fs_bar(in: BarOut) -> @location(0) vec4f {
  let px = length(fwidth(in.world));
  let half = in.shape.x;
  let width = in.shape.y;
  let style = in.style.x;
  let q = bar_local(in.world, in.centre, in.dir);
  let d = bar_sdf(q, half, width, style);
  if (d > 0.0) { discard; }
  let e = max(px * 0.75, 0.0006);
  let h0 = bar_height(q, half, width, style);
  let hx = bar_height(bar_local(in.world + vec2f(e, 0.0), in.centre, in.dir), half, width, style);
  let hy = bar_height(bar_local(in.world + vec2f(0.0, e), in.centre, in.dir), half, width, style);
  let n = normalize(vec3f(-(hx - h0) / e, -(hy - h0) / e, -1.0));
  var mat = metal(in.style.y);
  mat.brush = 2u;
  let grain = noise(vec2f(q.x * 40.0, q.y * 900.0 + in.shape.w * 30.0));
  mat.rough *= 0.8 + 0.4 * grain;
  var col = shade(vec3f(in.world, in.shape.z - h0), n, vec3f(in.dir, 0.0), mat) * (0.85 + 0.3 * grain);
  // Dark bores: the yoke's slot edge and the pin holes catch shadow.
  col *= 0.55 + 0.45 * smoothstep(0.0, 0.012, -d);
  if (style == BAR_BOSS || style == BAR_ROD) {
    let slot_c = select(vec2f(0.0), vec2f(sign(q.x) * half, 0.0), style == BAR_ROD);
    let slot_r = select(width * 0.45, width * 0.5, style == BAR_ROD);
    if (length(q - slot_c) < slot_r) {
      col *= 1.0 - 0.75 * screw_slot(q - slot_c, slot_r, in.shape.w * 9.0);
    }
  }
  return idle_fade(col);
}

@vertex
fn vs_bar_shadow(@builtin(vertex_index) vi: u32, in: BarIn) -> BarOut {
  var o = bar_vertex(vi, in);
  let lamp = w.light_pos[0].xyz;
  let receiver = w.misc.z;
  let s = (receiver - lamp.z) / max(in.a.z - lamp.z, 0.05);
  let soft = (receiver - in.a.z) * 0.4;
  let world = lamp.xy + (o.world - lamp.xy) * s + (o.world - o.centre) * soft * 4.0;
  o.pos = to_clip(vec3f(world, receiver - 0.01));
  o.shadow = vec4f(lamp.xy, s, soft);
  o.world = world;
  return o;
}

@fragment
fn fs_bar_shadow(in: BarOut) -> @location(0) vec4f {
  let lamp = in.shadow.xy;
  let s = in.shadow.z;
  let soft = max(in.shadow.w / s, 0.004);
  let caster = lamp + (in.world - lamp) / s;
  let q = bar_local(caster, in.centre, in.dir);
  let d = bar_sdf(q, in.shape.x, in.shape.y, in.style.x);
  let shadow = smoothstep(soft, -soft, d) * 0.5;
  return vec4f(vec3f(1.0 - shadow), 1.0);
}

// ---------------------------------------------------------------- steam

// Steam is lit by the lamps it drifts past.
fn steam_light(p: vec3f) -> vec3f {
  var col = vec3f(0.3);
  for (var i = 0; i < LIGHTS; i++) {
    let d = w.light_pos[i].xyz - p;
    var c = lamp(i, p) * 0.25;
    if (i < POINT_LAMPS) { c = w.light_col[i].rgb * 0.22 / (1.0 + dot(d, d) * 0.6); }
    col += c;
  }
  return col;
}

struct PuffIn {
  @location(0) a: vec4f, // position, size
  @location(1) b: vec4f, // age / life, seed, -, -
  @location(2) c: vec4f, // smoke colour
};

struct PuffOut {
  @builtin(position) pos: vec4f,
  @location(0) q: vec2f,
  @location(1) world: vec3f,
  @location(2) info: vec2f,
  @location(3) tint: vec3f,
};

@vertex
fn vs_puff(@builtin(vertex_index) vi: u32, in: PuffIn) -> PuffOut {
  var o: PuffOut;
  let c = corner(vi);
  o.pos = to_clip(vec3f(in.a.xy + c * in.a.w, in.a.z));
  o.q = c;
  o.world = in.a.xyz;
  o.info = in.b.xy;
  o.tint = in.c.rgb;
  return o;
}

@fragment
fn fs_puff(in: PuffOut) -> @location(0) vec4f {
  let life = in.info.x;
  let billow = fbm(in.q * 2.2 + vec2f(in.info.y * 17.0, life * 1.5));
  let d = length(in.q) + (billow - 0.5) * 0.7;
  let density = smoothstep(1.0, 0.25, d);
  let alpha = min(density * pow(1.0 - life, 1.5) * min(life * 6.0, 1.0) * 0.9, 0.95);
  let col = in.tint * steam_light(in.world) * (0.9 + 0.3 * billow);
  return vec4f(col * mix(0.45, 1.0, u.live), alpha);
}

@fragment
fn fs_steam(in: ScreenOut) -> @location(0) vec4f {
  let t = u.time;
  let q = in.p * 1.3 + vec2f(t * 0.03, -t * 0.07);
  let warp = fbm(q * 0.7 + vec2f(t * 0.02, 0.0));
  let f = fbm(q + warp * 1.4);
  // Thicker low down, where it pools, and swelling a little on the beat.
  let pool = 0.6 + 0.4 * smoothstep(0.8, -1.0, in.p.y);
  let density = smoothstep(0.36, 0.8, f) * pool * (1.0 + 0.4 * w.misc.x);
  let world = vec3f(unproject(in.p, 3.0), 3.0);
  // Drifts of grey, verdigris and cobalt smoke.
  let hue = fbm(in.p * 0.6 + vec2f(-t * 0.015, t * 0.01) + warp);
  var tint = mix(vec3f(0.5, 0.51, 0.54), vec3f(0.33, 0.6, 0.5), smoothstep(0.5, 0.68, hue));
  tint = mix(tint, vec3f(0.24, 0.36, 0.82), smoothstep(0.42, 0.28, hue));
  let col = tint * steam_light(world) * mix(0.45, 1.0, u.live);
  return vec4f(col, min(density * 0.57, 0.8));
}

// ---------------------------------------------------------------- sparks and dust

struct SparkIn {
  @location(0) a: vec4f, // position, size
  @location(1) b: vec4f, // velocity, kind (0 spark, 1 mote, 2 ember)
  @location(2) c: vec4f, // life left (0..1), -, -, -
};

struct SparkOut {
  @builtin(position) pos: vec4f,
  @location(0) q: vec2f,
  @location(1) world: vec3f,
  @location(2) info: vec2f, // kind, life
};

@vertex
fn vs_spark(@builtin(vertex_index) vi: u32, in: SparkIn) -> SparkOut {
  var o: SparkOut;
  let c = corner(vi);
  var along = vec2f(1.0, 0.0);
  var stretch = 1.0;
  if (in.b.w < 0.5) {
    let v = in.b.xy;
    let speed = length(v);
    if (speed > 1e-4) { along = v / speed; }
    stretch = 1.0 + speed * 6.0;
  }
  let side = vec2f(-along.y, along.x);
  let world = in.a.xy + (along * c.x * stretch + side * c.y) * in.a.w;
  o.pos = to_clip(vec3f(world, in.a.z));
  o.q = vec2f(c.x, c.y);
  o.world = in.a.xyz;
  o.info = vec2f(in.b.w, in.c.x);
  return o;
}

@fragment
fn fs_spark(in: SparkOut) -> @location(0) vec4f {
  let r2 = dot(in.q, in.q);
  if (in.info.x > 1.5) {
    // An ember: a dull orange glow that flickers as it floats up.
    let life = in.info.y;
    let flicker = 0.7 + 0.3 * sin(u.time * 23.0 + in.world.x * 300.0);
    return vec4f(vec3f(1.0, 0.4, 0.08) * exp(-r2 * 2.5) * life * flicker * 0.9 * u.live, 1.0);
  }
  if (in.info.x < 0.5) {
    // White-hot core cooling to orange as it dies.
    let life = in.info.y;
    let heat = mix(vec3f(1.0, 0.35, 0.08), vec3f(1.0, 0.9, 0.7), life);
    let core = exp(-r2 * 3.0);
    return vec4f(heat * core * (0.4 + 1.6 * life) * u.live, 1.0);
  }
  // A mote is only seen when a lamp catches it.
  var lit = 0.0;
  for (var i = 0; i < LIGHTS; i++) {
    let d = w.light_pos[i].xyz - in.world;
    var c = length(lamp(i, in.world)) * 0.4;
    if (i < POINT_LAMPS) { c = length(w.light_col[i].rgb) / (1.0 + dot(d, d) * 3.0); }
    lit += c;
  }
  let fade = sin(3.14159 * clamp(in.info.y, 0.0, 1.0));
  return vec4f(vec3f(1.0, 0.95, 0.85) * exp(-r2 * 4.0) * lit * 0.12 * fade * u.live, 1.0);
}

// ---------------------------------------------------------------- lamps and vignette

fn screen_of(p: vec3f) -> vec2f {
  return rot(p.xy - w.cam.xy, -w.cam.z) * w.cam.w / p.z;
}

@fragment
fn fs_glow(in: ScreenOut) -> @location(0) vec4f {
  var col = vec3f(0.0);
  var lit = 0.0;
  for (var i = 0; i < POINT_LAMPS; i++) {
    let s = screen_of(w.light_pos[i].xyz);
    let d = in.p - s;
    let r = length(d);
    let lamp = w.light_col[i].rgb;
    col += lamp * (exp(-r * r * 60.0) * 0.25 + exp(-r * 5.0) * 0.03);
    // Anamorphic streak.
    col += lamp * exp(-abs(d.y) * 120.0) * exp(-abs(d.x) * 2.2) * 0.035;
    lit += exp(-r * r * 2.0);
  }
  // Haze drifting up through the lamplight, and the spotlights' pools sweeping through it.
  let haze = fbm(in.p * 1.6 + vec2f(u.time * 0.02, -u.time * 0.06));
  col += vec3f(1.0, 0.85, 0.65) * haze * haze * lit * 0.05;
  for (var i = POINT_LAMPS; i < LIGHTS; i++) {
    for (var k = 0; k < 3; k++) {
      // Sample the beam at a few depths to suggest a shaft through the air.
      let z = 1.6 + f32(k) * 0.7;
      let pool = lamp(i, vec3f(unproject(in.p, z), z));
      col += pool * (0.3 + haze) * 0.012;
    }
  }
  return vec4f(col * u.live, 1.0);
}

@fragment
fn fs_vignette(in: ScreenOut) -> @location(0) vec4f {
  let q = in.p / vec2f(u.aspect, 1.0);
  let v = 1.0 - 0.55 * smoothstep(0.4, 1.5, length(q));
  return vec4f(vec3f(v), 1.0);
}

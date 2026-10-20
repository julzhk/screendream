// Platonic solids throwing coloured beams through black fog (prefixed with common.wgsl).
//   vs_screen/fs_fog     the fog itself, and a wet floor far below where beams pool
//   vs_shape/fs_shape    opaque solids: obsidian faces, faces that glow edge to edge, neon edges, lit where
//                        other shapes' beams sweep across them
//   vs_shape/fs_crystal  glass solids with dispersive rainbow sheen (additive)
//   vs_beam/fs_beam      one camera-facing quad per beam; the fragment integrates the light the
//                        fog scatters towards the eye and writes the depth of the brightest
//                        point, so shapes in front hide it (additive)
//   vs_mote/fs_mote      dust drifting in the fog, seen only inside a beam (additive)
//   vs_screen/fs_grade   vignette and film grain (multiply)
struct Scene {
  cam_pos: vec4f,   // xyz, focal length
  cam_fwd: vec4f,   // xyz, beam count
  cam_right: vec4f, // xyz, bass flash
  cam_up: vec4f,    // xyz, energy
  // Per beam: (origin, spread), (direction, aperture), (colour, intensity).
  beams: array<vec4f, 720>,
};

@group(1) @binding(0) var<uniform> sc: Scene;

const NEAR: f32 = 0.1;
const FAR: f32 = 60.0;
const FLOOR_Y: f32 = -3.4;
const FOG_COLOR: vec3f = vec3f(0.004, 0.005, 0.009);
const BEAM_LENGTH: f32 = 20.0; // matches beacon_sim.rs

fn view_of(p: vec3f) -> vec3f {
  let rel = p - sc.cam_pos.xyz;
  return vec3f(dot(rel, sc.cam_right.xyz), dot(rel, sc.cam_up.xyz), dot(rel, sc.cam_fwd.xyz));
}

fn clip_of(p: vec3f) -> vec4f {
  let v = view_of(p);
  let f = sc.cam_pos.w;
  return vec4f(v.x * f / u.aspect, v.y * f, FAR * (v.z - NEAR) / (FAR - NEAR), v.z);
}

fn depth_of(p: vec3f) -> f32 {
  let z = max(view_of(p).z, NEAR);
  return clamp(FAR * (z - NEAR) / ((FAR - NEAR) * z), 0.0, 1.0);
}

// How much survives the fog over distance `d`: very little, very soon.
fn visibility(d: f32) -> f32 {
  return exp(-d * 0.085);
}

// Drifting thickness of the fog at `p`, which gives the beams their texture.
fn fog_density(p: vec3f) -> f32 {
  let t = u.time;
  let q = vec2f(p.x * 0.35 + p.y * 0.2 + t * 0.04, p.z * 0.35 - p.y * 0.25 - t * 0.03);
  return 0.4 + 1.0 * fbm(q + vec2f(fbm(q * 0.6 + t * 0.02), 0.0));
}

fn beam_count() -> u32 {
  return u32(sc.cam_fwd.w);
}

struct BeamData {
  origin: vec3f,
  spread: f32,
  dir: vec3f,
  aperture: f32, // radius where the beam starts
  color: vec3f,
  intensity: f32,
};

fn beam(i: u32) -> BeamData {
  let a = sc.beams[i * 3u];
  let b = sc.beams[i * 3u + 1u];
  let c = sc.beams[i * 3u + 2u];
  return BeamData(a.xyz, a.w, b.xyz, b.w, c.rgb, c.w);
}

fn aperture(b: BeamData) -> f32 {
  return b.aperture;
}

// Light from beam `b` falling on point `p`.
fn beam_light(b: BeamData, p: vec3f) -> f32 {
  let v = p - b.origin;
  let t = dot(v, b.dir);
  if (t < 0.05 || t > BEAM_LENGTH) { return 0.0; }
  let r = length(v - b.dir * t);
  let radius = aperture(b) + b.spread * t;
  return b.intensity * exp(-2.0 * (r / radius) * (r / radius)) * 0.25 / (radius * radius + 0.25)
    * exp(-t * 0.07);
}

fn spectral(x: f32) -> vec3f {
  return 0.5 + 0.5 * cos(TAU * (x + vec3f(0.0, 0.33, 0.67)));
}

// ---------------------------------------------------------------- fog and floor

struct ScreenOut {
  @builtin(position) pos: vec4f,
  @location(0) p: vec2f,
};

@vertex
fn vs_screen(@builtin(vertex_index) vi: u32) -> ScreenOut {
  let c = fullscreen_pos(vi);
  var o: ScreenOut;
  o.pos = vec4f(c, 0.999, 1.0);
  o.p = vec2f(c.x * u.aspect, c.y);
  return o;
}

@fragment
fn fs_fog(in: ScreenOut) -> @location(0) vec4f {
  let f = sc.cam_pos.w;
  let rd = normalize(sc.cam_fwd.xyz * f + sc.cam_right.xyz * in.p.x + sc.cam_up.xyz * in.p.y);
  let cam = sc.cam_pos.xyz;
  let t = u.time;
  // The fog never quite goes black: a faint, slow churn, lifted a touch by the bass.
  let churn = fbm(in.p * 1.4 + vec2f(t * 0.015, -t * 0.01));
  var col = FOG_COLOR * (0.6 + churn) * (1.0 + 0.8 * sc.cam_right.w);

  if (rd.y < -0.01) {
    let dist = (FLOOR_Y - cam.y) / rd.y;
    if (dist < FAR) {
      let fp = cam + rd * dist;
      // Wet black stone: puddles take the light sharper and brighter.
      let wet = smoothstep(0.45, 0.65, fbm(fp.xz * 0.5));
      let stone = 0.004 + 0.004 * noise(fp.xz * 6.0);
      var lit = vec3f(stone);
      for (var i = 0u; i < beam_count(); i++) {
        let b = beam(i);
        let incidence = max(-b.dir.y, 0.0);
        lit += b.color * beam_light(b, fp) * incidence * (0.6 + 1.4 * wet);
      }
      col = mix(col, lit, visibility(dist));
    }
  }
  return idle_fade(col);
}

@fragment
fn fs_grade(in: ScreenOut) -> @location(0) vec4f {
  let q = in.p / vec2f(u.aspect, 1.0);
  var v = 1.0 - 0.7 * smoothstep(0.35, 1.45, length(q));
  let grain = hash2(floor(in.pos.xy) + vec2f(floor(u.time * 24.0) * 13.0, 0.0));
  v *= 1.0 - 0.08 * grain;
  return vec4f(vec3f(v), 1.0);
}

// ---------------------------------------------------------------- shapes

struct ShapeIn {
  @location(0) pos: vec3f,
  @location(1) normal: vec3f,
  @location(2) extra: vec2f, // 1 at the face centre falling to 0 at its rim; face index
  @location(3) m0: vec4f,    // rotation × scale columns; w: crystal, emitter bits, fade
  @location(4) m1: vec4f,
  @location(5) m2: vec4f,
  @location(6) centre: vec4f, // position, beam intensity
  @location(7) c0: vec4f,
  @location(8) c1: vec4f,
  @location(9) c2: vec4f,
};

struct ShapeOut {
  @builtin(position) pos: vec4f,
  @location(0) world: vec3f,
  @location(1) normal: vec3f,
  @location(2) edge: f32,
  @location(3) @interpolate(flat) face: u32,
  @location(4) @interpolate(flat) emitters: u32,
  @location(5) info: vec2f, // fade, intensity
  @location(6) c0: vec3f,
  @location(7) c1: vec3f,
  @location(8) c2: vec3f,
};

@vertex
fn vs_shape(in: ShapeIn) -> ShapeOut {
  var o: ShapeOut;
  let world = in.centre.xyz + in.m0.xyz * in.pos.x + in.m1.xyz * in.pos.y + in.m2.xyz * in.pos.z;
  o.pos = clip_of(world);
  o.world = world;
  o.normal = normalize(in.m0.xyz * in.normal.x + in.m1.xyz * in.normal.y + in.m2.xyz * in.normal.z);
  o.edge = in.extra.x;
  o.face = u32(in.extra.y);
  o.emitters = u32(in.m1.w);
  o.info = vec2f(in.m2.w, in.centre.w);
  o.c0 = in.c0.rgb;
  o.c1 = in.c1.rgb;
  o.c2 = in.c2.rgb;
  return o;
}

fn face_color(in: ShapeOut) -> vec3f {
  switch (in.face % 3u) {
    case 0u: { return in.c0; }
    case 1u: { return in.c1; }
    default: { return in.c2; }
  }
}

@fragment
fn fs_shape(in: ShapeOut) -> @location(0) vec4f {
  let cam = sc.cam_pos.xyz;
  let to_cam = cam - in.world;
  let dist = length(to_cam);
  let v = to_cam / dist;
  let n = normalize(in.normal);
  let intensity = in.info.y;

  // Obsidian, catching other shapes' beams as they sweep across it.
  var col = vec3f(0.006, 0.006, 0.008);
  for (var i = 0u; i < beam_count(); i++) {
    let b = beam(i);
    let light = beam_light(b, in.world);
    if (light > 0.0) {
      let diffuse = max(dot(n, -b.dir), 0.0);
      let spec = pow(max(dot(reflect(b.dir, n), v), 0.0), 60.0);
      col += b.color * light * (diffuse * 0.4 + spec * 3.0);
    }
  }
  // A cold sheen from the fog at grazing angles.
  col += vec3f(0.02, 0.025, 0.035) * pow(1.0 - max(dot(n, v), 0.0), 4.0);

  // Faces that throw beams have a glowing lens in a dark bezel.
  let emits = (in.emitters >> in.face) & 1u;
  let tint = face_color(in);
  if (emits == 1u) {
    // The whole face is the light, hottest at its centre.
    col = col * 0.3 + tint * intensity * (0.9 + 2.2 * in.edge * in.edge);
  } else {
    // Engraved rings on the others.
    let ring = smoothstep(0.85, 1.0, abs(sin(in.edge * 28.0)));
    col *= 1.0 - 0.4 * ring;
  }
  // Neon trim along every edge.
  let trim = 1.0 - smoothstep(0.0, 0.05, in.edge);
  col += mix(tint, vec3f(1.0), 0.3) * trim * (0.25 + 0.35 * intensity);

  col = mix(FOG_COLOR, col, visibility(dist)) * in.info.x;
  return idle_fade(col);
}

@fragment
fn fs_crystal(in: ShapeOut) -> @location(0) vec4f {
  let cam = sc.cam_pos.xyz;
  let to_cam = cam - in.world;
  let dist = length(to_cam);
  let v = to_cam / dist;
  let n = normalize(in.normal);
  let facing = abs(dot(n, v));
  let intensity = in.info.y;
  let fresnel = pow(1.0 - facing, 3.0);
  // Dispersion: a rainbow that shifts with the viewing angle and from facet to facet.
  let rainbow = spectral(facing * 2.2 + f32(in.face) * 0.137 + u.time * 0.05);
  var col = rainbow * (0.03 + fresnel * 0.5) * (0.5 + 0.5 * intensity);
  // Beams passing through scatter into rainbow glints.
  for (var i = 0u; i < beam_count(); i++) {
    let b = beam(i);
    let light = beam_light(b, in.world);
    if (light > 0.0) {
      col += (b.color * 0.5 + rainbow * 0.8) * light * (0.3 + fresnel);
    }
  }
  // The crystal's heart glows white through the faces that shine.
  if (((in.emitters >> in.face) & 1u) == 1u) {
    col += vec3f(1.0) * smoothstep(0.5, 0.9, in.edge) * intensity * 0.6;
  }
  col += vec3f(0.8, 0.9, 1.0) * (1.0 - smoothstep(0.0, 0.04, in.edge)) * 0.25;
  col *= visibility(dist) * in.info.x;
  return vec4f(col * u.live, 1.0);
}

// ---------------------------------------------------------------- beams

struct BeamOut {
  @builtin(position) pos: vec4f,
  @location(0) world: vec3f,
  @location(1) @interpolate(flat) index: u32,
  @location(2) @interpolate(flat) len: f32,
};

struct BeamFrag {
  @location(0) color: vec4f,
  @builtin(frag_depth) depth: f32,
};

@vertex
fn vs_beam(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> BeamOut {
  var o: BeamOut;
  o.index = ii;
  let b = beam(ii);
  let cam = sc.cam_pos.xyz;
  let zo = view_of(b.origin).z;
  if (zo < 0.3 || b.intensity <= 0.001) {
    o.pos = vec4f(2.0, 2.0, 2.0, 1.0);
    return o;
  }
  // Stop the beam short of the camera.
  var len = BEAM_LENGTH;
  let ze = view_of(b.origin + b.dir * len).z;
  if (ze < 0.3) { len = len * (zo - 0.3) / (zo - ze); }
  o.len = len;
  let far = b.origin + b.dir * len;
  let r0 = aperture(b) * 2.0;
  let r1 = (aperture(b) + b.spread * len) * 2.0;
  // A quad in the plane facing the camera, covering the cone's silhouette.
  let mid = normalize(0.5 * (b.origin + far) - cam);
  var axis = b.dir - mid * dot(b.dir, mid);
  if (length(axis) < 1e-3) { axis = sc.cam_up.xyz; }
  axis = normalize(axis);
  let side = normalize(cross(axis, mid));
  var corners = array<vec2f, 6>(vec2f(0.0, -1.0), vec2f(1.0, -1.0), vec2f(0.0, 1.0),
                                vec2f(1.0, -1.0), vec2f(1.0, 1.0), vec2f(0.0, 1.0));
  let c = corners[vi];
  var world: vec3f;
  if (c.x < 0.5) {
    world = b.origin - axis * r0 + side * c.y * r0;
  } else {
    world = far + axis * r1 + side * c.y * r1;
  }
  o.pos = clip_of(world);
  o.world = world;
  return o;
}

@fragment
fn fs_beam(in: BeamOut) -> BeamFrag {
  var out: BeamFrag;
  let b = beam(in.index);
  let cam = sc.cam_pos.xyz;
  let rd = normalize(in.world - cam);
  // Closest approach between the view ray and the beam's axis.
  let w0 = cam - b.origin;
  let along = dot(rd, b.dir);
  let dd = dot(rd, w0);
  let e = dot(b.dir, w0);
  let denom = max(1.0 - along * along, 1e-4);
  let t = clamp((e - along * dd) / denom, 0.0, in.len);
  let s = max(along * t - dd, 0.0);
  let on_ray = cam + rd * s;
  let on_beam = b.origin + b.dir * t;
  let r = length(on_ray - on_beam);
  // Keep fine lasers at least about a pixel wide so they don't shimmer.
  let true_radius = aperture(b) + b.spread * t;
  let radius = max(true_radius, s * 0.0018);
  let core = exp(-2.0 * (r / radius) * (r / radius));
  // Looking down a beam you see through more of it.
  let depth_through = min(inverseSqrt(denom), 8.0);
  // The same light spread over a wider beam is dimmer per unit of fog.
  let power = b.intensity * 0.35 / (radius + 0.25) * true_radius / radius;
  let fade = exp(-t * 0.07) * (1.0 - smoothstep(in.len * 0.7, in.len, t));
  var col = b.color * core * power * depth_through * 0.25 * fog_density(on_beam) * fade
    * visibility(s);
  // Glare from the source when a beam points at the eye.
  let toward = max(dot(-b.dir, normalize(cam - b.origin)), 0.0);
  let off = 1.0 - max(dot(rd, normalize(b.origin - cam)), 0.0);
  col += (b.color + vec3f(0.6)) * b.intensity * pow(toward, 6.0) * exp(-off * 4000.0) * 2.0
    * visibility(length(b.origin - cam));
  out.color = vec4f(col * u.live, 1.0);
  out.depth = depth_of(on_ray);
  return out;
}

// ---------------------------------------------------------------- motes

struct MoteOut {
  @builtin(position) pos: vec4f,
  @location(0) q: vec2f,
  @location(1) light: vec3f,
};

@vertex
fn vs_mote(@builtin(vertex_index) vi: u32, @location(0) at: vec4f) -> MoteOut {
  var o: MoteOut;
  var corners = array<vec2f, 6>(vec2f(-1.0, -1.0), vec2f(1.0, -1.0), vec2f(-1.0, 1.0),
                                vec2f(1.0, -1.0), vec2f(1.0, 1.0), vec2f(-1.0, 1.0));
  let c = corners[vi];
  let size = 0.012;
  let world = at.xyz + (sc.cam_right.xyz * c.x + sc.cam_up.xyz * c.y) * size;
  o.pos = clip_of(world);
  o.q = c;
  var light = vec3f(0.0);
  for (var i = 0u; i < beam_count(); i++) {
    let b = beam(i);
    light += b.color * beam_light(b, at.xyz);
  }
  o.light = light * visibility(length(at.xyz - sc.cam_pos.xyz)) * 1.5;
  return o;
}

@fragment
fn fs_mote(in: MoteOut) -> @location(0) vec4f {
  return vec4f(in.light * exp(-dot(in.q, in.q) * 3.0) * u.live, 1.0);
}

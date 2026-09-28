// Full-screen background drawn before the mesh: pulsing rings, drifting nebula and a
// frequency bar arc. Fades to a dim, desaturated idle look while no audio is detected.
struct Controls {
  time: f32,
  aspect: f32,
  amplitude: f32,
  glow: f32,
  bands: vec4f,
  live: f32,
  _pad0: f32,
  _pad1: f32,
  _pad2: f32,
};

@group(0) @binding(0) var<uniform> u: Controls;

struct VsOut {
  @builtin(position) clip: vec4f,
  @location(0) uv: vec2f,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VsOut {
  let x = f32((vertex_index << 1u) & 2u);
  let y = f32(vertex_index & 2u);
  let pos = vec2f(x * 2.0 - 1.0, y * 2.0 - 1.0);
  return VsOut(vec4f(pos, 0.0, 1.0), pos);
}

fn hash(p: vec2f) -> f32 {
  return fract(sin(dot(p, vec2f(127.1, 311.7))) * 43758.5453);
}

fn noise(p: vec2f) -> f32 {
  let i = floor(p); let f = fract(p);
  let s = f * f * (3.0 - 2.0 * f);
  return mix(mix(hash(i), hash(i + vec2f(1.0, 0.0)), s.x),
             mix(hash(i + vec2f(0.0, 1.0)), hash(i + vec2f(1.0, 1.0)), s.x), s.y);
}

fn fbm(p0: vec2f) -> f32 {
  var p = p0; var v = 0.0; var a = 0.5;
  for (var i = 0; i < 5; i++) { v += a * noise(p); p = p * 2.03 + vec2f(1.7, 9.2); a *= 0.5; }
  return v;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4f {
  let t = u.time;
  let p = vec2f(in.uv.x * u.aspect, in.uv.y);
  let r = length(p);
  let a = atan2(p.y, p.x);
  let bass = u.bands.x; let lmid = u.bands.y; let hmid = u.bands.z; let treble = u.bands.w;

  // Nebula swirl that speeds up with mids.
  let swirl = fbm(p * 1.6 + vec2f(t * 0.05, -t * 0.03) + fbm(p * 2.5 - t * 0.1 * (1.0 + lmid)));
  var col = mix(vec3f(0.02, 0.01, 0.06), vec3f(0.25, 0.05, 0.45), swirl);
  col += vec3f(0.0, 0.25, 0.4) * pow(swirl, 3.0) * (0.5 + hmid);

  // Expanding rings driven by bass.
  let rings = sin(r * 14.0 - t * 3.0 - bass * 6.0);
  col += vec3f(1.0, 0.25, 0.6) * smoothstep(0.92, 1.0, rings) * bass * exp(-r * 0.8);

  // Radial spectrum arc: 64 spokes whose length follows the four bands.
  let spokes = 64.0;
  let seg = floor((a / 6.28318 + 0.5) * spokes);
  let bandf = abs(seg / spokes * 2.0 - 1.0) * 3.0;
  let b0 = u.bands[u32(clamp(floor(bandf), 0.0, 3.0))];
  let b1 = u.bands[u32(clamp(floor(bandf) + 1.0, 0.0, 3.0))];
  let level = mix(b0, b1, fract(bandf)) * (0.7 + 0.3 * hash(vec2f(seg, floor(t * 12.0))));
  let inner = 0.72 + 0.1 * bass;
  let outer = inner + 0.05 + 0.45 * level;
  let gap = abs(fract((a / 6.28318 + 0.5) * spokes) - 0.5);
  let spoke = step(inner, r) * step(r, outer) * step(gap, 0.32);
  col += mix(vec3f(0.2, 0.6, 1.0), vec3f(1.0, 0.3, 0.8), level) * spoke * (0.6 + level);

  // Twinkling stars on treble.
  let star = step(0.997, hash(floor(p * 90.0))) * (0.3 + 1.5 * treble);
  col += vec3f(star);

  // Centre glow and vignette.
  col += vec3f(0.6, 0.3, 1.0) * u.glow * 0.5 * exp(-r * 2.5);
  col *= 1.0 - 0.45 * smoothstep(0.6, 1.8, r);

  // Idle: desaturate and dim so a silent visualizer is never mistaken for a reacting one.
  let grey = vec3f(dot(col, vec3f(0.299, 0.587, 0.114)));
  col = mix(grey * 0.45, col, u.live);
  return vec4f(col, 1.0);
}

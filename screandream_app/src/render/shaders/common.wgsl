// Shared by the mode shaders that are loaded with it as a prefix (see `modes::shader_with_common`):
// the group-0 `Controls` uniform plus noise and full-screen helpers.
struct Controls {
  time: f32,
  aspect: f32,
  amplitude: f32,
  glow: f32,
  bands: vec4f, // bass, low-mid, high-mid, treble
  live: f32,
  _pad0: f32,
  _pad1: f32,
  _pad2: f32,
};

@group(0) @binding(0) var<uniform> u: Controls;

const TAU: f32 = 6.2831853;

// Integer hash of the lattice cell containing `p`; stable at large coordinates.
fn hash2(p: vec2f) -> f32 {
  let q = vec2i(floor(p));
  var h = (bitcast<u32>(q.x) * 1597334677u) ^ (bitcast<u32>(q.y) * 3812015801u);
  h = h ^ (h >> 16u);
  h = h * 2246822519u;
  h = h ^ (h >> 13u);
  h = h * 3266489917u;
  h = h ^ (h >> 16u);
  return f32(h) / 4294967295.0;
}

fn noise(p: vec2f) -> f32 {
  let i = floor(p);
  let f = fract(p);
  let s = f * f * (3.0 - 2.0 * f);
  return mix(mix(hash2(i), hash2(i + vec2f(1.0, 0.0)), s.x),
             mix(hash2(i + vec2f(0.0, 1.0)), hash2(i + vec2f(1.0, 1.0)), s.x), s.y);
}

fn fbm(p0: vec2f) -> f32 {
  var p = p0; var v = 0.0; var a = 0.5;
  for (var i = 0; i < 4; i++) { v += a * noise(p); p = p * 2.03 + vec2f(1.7, 9.2); a *= 0.5; }
  return v;
}

// Clip-space corner of a triangle that covers the whole screen (draw 3 vertices).
fn fullscreen_pos(vertex_index: u32) -> vec2f {
  let x = f32((vertex_index << 1u) & 2u);
  let y = f32(vertex_index & 2u);
  return vec2f(x * 2.0 - 1.0, y * 2.0 - 1.0);
}

// Dims and desaturates while idle so silence never looks like a reaction.
fn idle_fade(col: vec3f) -> vec4f {
  let grey = vec3f(dot(col, vec3f(0.299, 0.587, 0.114)));
  return vec4f(mix(grey * 0.45, col, u.live), 1.0);
}

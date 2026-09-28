// Draws the bundled PLY mesh: one large central crystal plus an orbiting ring of instances.
struct Controls {
  time: f32,
  aspect: f32,
  amplitude: f32,
  glow: f32,
  bands: vec4f, // bass, low-mid, high-mid, treble
  active: f32,
  _pad0: f32,
  _pad1: f32,
  _pad2: f32,
};

@group(0) @binding(0) var<uniform> u: Controls;

struct VsOut {
  @builtin(position) clip: vec4f,
  @location(0) world: vec3f,
  @location(1) tint: vec3f,
  @location(2) local: vec3f,
};

const RING: f32 = 18.0;

fn rot_y(a: f32) -> mat3x3f {
  let c = cos(a); let s = sin(a);
  return mat3x3f(vec3f(c, 0.0, -s), vec3f(0.0, 1.0, 0.0), vec3f(s, 0.0, c));
}

fn rot_x(a: f32) -> mat3x3f {
  let c = cos(a); let s = sin(a);
  return mat3x3f(vec3f(1.0, 0.0, 0.0), vec3f(0.0, c, s), vec3f(0.0, -s, c));
}

fn palette(t: f32) -> vec3f {
  return 0.5 + 0.5 * cos(6.28318 * (vec3f(0.0, 0.33, 0.67) + t));
}

@vertex
fn vs_main(@location(0) position: vec3f, @builtin(instance_index) instance: u32) -> VsOut {
  let bass = u.bands.x;
  let mid = u.bands.y;
  let t = u.time;
  var p = position;
  var world: vec3f;
  var tint: vec3f;

  if (instance == 0u) {
    // Central crystal: breathes with bass, spikes vertices with treble.
    let spike = 1.0 + 0.6 * u.bands.w * (0.5 + 0.5 * sin(t * 7.0 + dot(p, vec3f(3.1, 1.7, 2.3))));
    p = p * (1.05 + 0.9 * bass + 0.4 * u.amplitude) * spike;
    world = rot_x(t * 0.23 + mid) * (rot_y(t * 0.41) * p);
    tint = palette(t * 0.05 + u.bands.z * 0.3);
  } else {
    // Orbiting ring: radius pumps with bass, each shard spins on its own axis.
    let k = f32(instance - 1u);
    let phase = k / RING * 6.28318;
    let orbit = phase + t * (0.25 + 0.5 * mid);
    let radius = 3.2 + 1.4 * bass + 0.35 * sin(t * 1.3 + k);
    let size = 0.22 + 0.5 * u.bands[instance % 4u];
    let local = rot_x(t * 1.7 + k) * (rot_y(t * 2.3 + k * 1.3) * (p * size));
    let lift = sin(phase * 3.0 + t) * (0.5 + 1.5 * u.bands.z);
    world = vec3f(cos(orbit) * radius, lift, sin(orbit) * radius) + local;
    world = rot_x(0.35) * world;
    tint = palette(k / RING + t * 0.07);
  }

  // Simple perspective camera looking down -z.
  let view = world + vec3f(0.0, 0.0, -8.5);
  let f = 1.0 / tan(0.5 * 0.9);
  let near = 0.1; let far = 50.0;
  let clip = vec4f(
    view.x * f / u.aspect,
    view.y * f,
    (view.z * far / (near - far)) + (near * far / (near - far)),
    -view.z,
  );
  return VsOut(clip, world, tint, position);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4f {
  // Flat-shaded facets from screen-space derivatives.
  let n = normalize(cross(dpdx(in.world), dpdy(in.world)));
  let light = normalize(vec3f(0.4, 0.8, 0.6));
  let diffuse = abs(dot(n, light));
  let rim = pow(1.0 - abs(n.z), 2.0);
  let edge = smoothstep(0.85, 1.0, max(abs(in.local.x), max(abs(in.local.y), abs(in.local.z))));
  let energy = mix(0.25, 1.0, u.active);
  var color = in.tint * (0.25 + 0.75 * diffuse) * energy;
  color += in.tint * rim * (0.4 + 1.6 * u.glow) * energy;
  color += vec3f(1.0) * edge * 0.35 * u.bands.w * u.active;
  return vec4f(color, 1.0);
}

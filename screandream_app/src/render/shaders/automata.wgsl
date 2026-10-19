// Cellular automata adrift in 3D space (prefixed with common.wgsl at load time).
//   vs_space/fs_space  deep-space backdrop: nebula and stars, bent by the screen warps
//   vs_plane/fs_plane  one tessellated, warped plane per automaton; its cells are read from
//                      its layer of the cell texture (additive)
//   vs_space/fs_warps  faint chromatic rims showing where the screen warps are (additive)
// Screen space: x ∈ [-aspect, aspect], y ∈ [-1, 1], matching automata_sim.rs.
struct Field {
  cam_pos: vec4f,   // xyz, focal length
  cam_fwd: vec4f,   // xyz, beat pulse
  cam_right: vec4f, // xyz, energy
  cam_up: vec4f,    // xyz, treble
  // Per warp: (x, y, radius, strength × envelope), (kind, dir x, dir y, age).
  warps: array<vec4f, 16>,
  // Per automaton, 8 vec4s:
  //   0 centre, fade          1 u axis × half size, cells per side
  //   2 v axis × half size, growth radius (cells)
  //   3 warp: bend, twist, ripple amplitude, ripple frequency
  //   4 palette colour 0, ripple phase    5 palette colour 1, growth-front pulse
  //   6 palette colour 2, cell shape      7 seed, glow, -, 1 for elementary rules
  planes: array<vec4f, 288>,
};

@group(1) @binding(0) var<uniform> f: Field;
@group(1) @binding(1) var cells: texture_2d_array<f32>;
@group(1) @binding(2) var cell_sampler: sampler;

const MAX_WARPS: u32 = 8u;
const TESS: u32 = 40u;      // quads per plane side, so bends and screen warps stay smooth
const GRID: f32 = 128.0;    // texture layer size, matches automata_sim.rs
const NEAR: f32 = 0.05;
const FAR: f32 = 40.0;
const FOG_START: f32 = 18.0;
const FOG_END: f32 = 31.0;

fn plane(id: u32, k: u32) -> vec4f {
  return f.planes[id * 8u + k];
}

// Total displacement of the screen warps at screen point `p`.
fn warp_offset(p: vec2f) -> vec2f {
  var off = vec2f(0.0);
  for (var i = 0u; i < MAX_WARPS; i++) {
    let a = f.warps[i * 2u];
    let b = f.warps[i * 2u + 1u];
    if (a.w == 0.0) { continue; }
    let d = p - a.xy;
    let r2 = a.z * a.z;
    let kind = u32(b.x);
    if (kind == 0u) {        // lens: magnify (or shrink) around the centre
      off += d * a.w * exp(-dot(d, d) / r2);
    } else if (kind == 1u) { // swirl
      let angle = a.w * exp(-dot(d, d) / r2);
      let cs = vec2f(cos(angle), sin(angle));
      off += vec2f(d.x * cs.x - d.y * cs.y, d.x * cs.y + d.y * cs.x) - d;
    } else if (kind == 2u) { // shockwave ring
      let dist = length(d);
      let ring = (dist - a.z) / 0.07;
      off += d / max(dist, 1e-4) * a.w * exp(-ring * ring);
    } else {                 // band: a rippling shear sweeping along its normal
      let across = dot(d, b.yz);
      let tangent = vec2f(-b.z, b.y);
      off += tangent * a.w * exp(-across * across / r2) * sin(dot(d, tangent) * 9.0 + u.time * 3.0);
    }
  }
  return off;
}

// Three-colour gradient of an automaton's scheme.
fn scheme(c0: vec3f, c1: vec3f, c2: vec3f, t: f32) -> vec3f {
  return select(mix(c1, c2, t * 2.0 - 1.0), mix(c0, c1, t * 2.0), t < 0.5);
}

// ---------------------------------------------------------------- backdrop

struct ScreenOut {
  @builtin(position) pos: vec4f,
  @location(0) p: vec2f,
};

@vertex
fn vs_space(@builtin(vertex_index) vi: u32) -> ScreenOut {
  let c = fullscreen_pos(vi);
  var o: ScreenOut;
  o.pos = vec4f(c, 0.0, 1.0);
  o.p = vec2f(c.x * u.aspect, c.y);
  return o;
}

@fragment
fn fs_space(in: ScreenOut) -> @location(0) vec4f {
  let p = in.p - warp_offset(in.p);
  let dir = normalize(f.cam_fwd.xyz * f.cam_pos.w + f.cam_right.xyz * p.x + f.cam_up.xyz * p.y);
  let sky = vec2f(atan2(dir.x, -dir.z), asin(clamp(dir.y, -1.0, 1.0)));

  // Nebula: two layers of folded noise, slowly cycling hue.
  let n1 = fbm(sky * 2.2 + vec2f(u.time * 0.01, 3.0));
  let n2 = fbm(sky * 4.7 + vec2f(n1 * 1.6, -u.time * 0.013));
  var col = mix(vec3f(0.006, 0.0, 0.02), vec3f(0.05, 0.015, 0.09), n1);
  col += vec3f(0.02, 0.07, 0.1) * pow(n2, 3.0) * 2.5;
  col = mix(col, col.zxy, 0.5 + 0.5 * sin(u.time * 0.017));
  col *= 1.0 + 0.5 * f.cam_right.w;

  // Stars, twinkling with the treble.
  for (var layer = 0; layer < 2; layer++) {
    let density = 90.0 + 110.0 * f32(layer);
    let g = sky * density;
    let cell = floor(g);
    let h = hash2(cell + f32(layer) * 37.0);
    if (h > 0.97) {
      let jitter = vec2f(hash2(cell + 11.0), hash2(cell + 23.0)) - 0.5;
      let d = length(fract(g) - 0.5 - jitter * 0.6);
      let twinkle = 0.6 + 0.4 * sin(u.time * (2.0 + 6.0 * h) + h * 90.0) * (0.3 + f.cam_up.w);
      let tint = mix(vec3f(0.7, 0.8, 1.0), vec3f(1.0, 0.85, 0.7), hash2(cell + 5.0));
      col += tint * smoothstep(0.12, 0.0, d) * twinkle * (h - 0.97) * 30.0 * (0.5 - 0.2 * f32(layer));
    }
  }

  // Vignette.
  col *= 1.0 - 0.35 * dot(in.p / vec2f(u.aspect, 1.0), in.p / vec2f(u.aspect, 1.0)) * 0.5;
  return idle_fade(col);
}

// ---------------------------------------------------------------- automata planes

struct PlaneOut {
  @builtin(position) pos: vec4f,
  @location(0) uv: vec2f,
  @location(1) @interpolate(flat) id: u32,
  @location(2) depth: f32,
  // How steeply the plane is seen: 0 face-on, 1 edge-on.
  @location(3) grazing: f32,
};

@vertex
fn vs_plane(@builtin(vertex_index) vi: u32, @builtin(instance_index) id: u32) -> PlaneOut {
  var o: PlaneOut;
  o.id = id;
  let centre = plane(id, 0u);
  if (centre.w <= 0.001) {
    o.pos = vec4f(2.0, 2.0, 2.0, 1.0); // invisible: collapse outside the clip volume
    return o;
  }
  let corners = array<vec2u, 6>(
    vec2u(0u, 0u), vec2u(1u, 0u), vec2u(0u, 1u), vec2u(1u, 0u), vec2u(1u, 1u), vec2u(0u, 1u));
  let quad = vi / 6u;
  let g = vec2u(quad % TESS, quad / TESS) + corners[vi % 6u];
  let uv = vec2f(g) / f32(TESS);
  let l = uv * 2.0 - 1.0;

  let au = plane(id, 1u).xyz;
  let av = plane(id, 2u).xyz;
  let w = plane(id, 3u);
  let half_size = length(au);
  let n = normalize(cross(au, av));
  // Bend (cylinder), twist (saddle) and a ripple running out from the centre.
  let disp = w.x * l.x * l.x + w.y * l.x * l.y + w.z * sin(length(l) * w.w * 3.0 - plane(id, 4u).w);
  let world = centre.xyz + au * l.x + av * l.y + n * disp * half_size;

  let rel = world - f.cam_pos.xyz;
  let view = vec3f(dot(rel, f.cam_right.xyz), dot(rel, f.cam_up.xyz), dot(rel, f.cam_fwd.xyz));
  let focal = f.cam_pos.w;
  var xy = view.xy * focal;
  if (view.z > NEAR) {
    // Bend the projected point by the screen warps, keeping perspective-correct clipping.
    var s = xy / view.z;
    s += warp_offset(s);
    xy = s * view.z;
  }
  o.pos = vec4f(xy.x / u.aspect, xy.y, FAR * (view.z - NEAR) / (FAR - NEAR), view.z);
  o.uv = uv;
  o.depth = view.z;
  o.grazing = 1.0 - abs(dot(n, normalize(rel)));
  return o;
}

@fragment
fn fs_plane(in: PlaneOut) -> @location(0) vec4f {
  let id = in.id;
  let centre = plane(id, 0u);
  let dim = plane(id, 1u).w;
  let radius = plane(id, 2u).w;
  let style = plane(id, 7u);
  let g = in.uv * dim;
  // Screen pixels per cell, taken before any branching (derivatives need uniform control).
  let px = length(fwidth(g));

  let cell = min(vec2i(floor(g)), vec2i(i32(dim) - 1));
  let t = textureLoad(cells, cell, i32(id), 0);
  let soft = textureSampleLevel(cells, cell_sampler,
                                clamp(g, vec2f(0.5), vec2f(dim - 0.5)) / GRID, i32(id), 0.0);

  // Cells seeded by another automaton keep that automaton's colours.
  let owner = u32(round(t.b * 255.0));
  var src = id;
  if (owner > 0u) { src = owner - 1u; }
  let c0 = plane(src, 4u).xyz;
  let c1 = plane(src, 5u).xyz;
  let c2 = plane(src, 6u).xyz;
  let tone = fract(t.g * 0.8 + hash2(vec2f(cell) + style.x * 977.0) * 0.35);
  let col = scheme(c0, c1, c2, tone);

  // Cell shape, fading to a flat average once cells shrink below a few pixels.
  let q = fract(g) - 0.5;
  var d: f32;
  switch (u32(plane(id, 6u).w)) {
    case 0u: { d = length(max(abs(q) - vec2f(0.3), vec2f(0.0))) - 0.1; }
    case 1u: { d = length(q) - 0.4; }
    case 2u: { d = abs(q.x) + abs(q.y) - 0.47; }
    default: { d = abs(length(q) - 0.3) - 0.11; }
  }
  let aa = max(px, 0.02);
  let body = mix(1.0 - smoothstep(-aa, aa, d), 0.6, smoothstep(0.35, 1.0, px));

  let fresh = t.a;
  let sparkle = step(0.993, hash2(vec2f(cell) * 1.37 + floor(u.time * 10.0) + style.x * 31.0))
    * f.cam_up.w * t.r;
  var rgb = col * t.r * body * (0.8 + 1.4 * fresh)
          + vec3f(1.0) * (fresh * fresh * 0.35 + sparkle * 1.5) * body * t.r;
  rgb += col * soft.r * style.y * 0.4;

  // The unlocked "soil" shows a faint lattice; the growth front glows, flaring on beats.
  let own0 = plane(id, 4u).xyz;
  let own1 = plane(id, 5u);
  let own2 = plane(id, 6u).xyz;
  let mid = vec2f(dim * 0.5);
  let front_d = select(length(g - mid), abs(g.x - mid.x), style.w > 0.5);
  let inside = 1.0 - smoothstep(radius - 1.0, radius + 1.0, front_d);
  let edge = min(0.5 - abs(q.x), 0.5 - abs(q.y));
  let lattice = (1.0 - smoothstep(0.0, px * 1.5 + 0.02, edge)) * inside
              * (1.0 - smoothstep(0.3, 1.2, px));
  rgb += own1.xyz * lattice * 0.06;
  let ring = (front_d - radius) / max(1.5, px * 2.0);
  rgb += own2 * exp(-ring * ring) * (0.06 + 0.5 * own1.w);

  // A thin frame so each plane reads as a sheet in space, brighter when seen edge-on.
  let border = max(abs(in.uv.x * 2.0 - 1.0), abs(in.uv.y * 2.0 - 1.0));
  rgb += own0 * smoothstep(0.985, 1.0, border) * (0.12 + 0.3 * in.grazing);

  let fog = 1.0 - smoothstep(FOG_START, FOG_END, in.depth);
  let close = smoothstep(0.4, 2.5, in.depth);
  rgb *= centre.w * fog * close * (0.85 + 0.35 * f.cam_fwd.w);
  return idle_fade(rgb);
}

// ---------------------------------------------------------------- warp rims

@fragment
fn fs_warps(in: ScreenOut) -> @location(0) vec4f {
  var rgb = vec3f(0.0);
  for (var i = 0u; i < MAX_WARPS; i++) {
    let a = f.warps[i * 2u];
    let b = f.warps[i * 2u + 1u];
    if (a.w == 0.0) { continue; }
    let d = in.p - a.xy;
    let dist = length(d);
    let kind = u32(b.x);
    if (kind == 0u) {        // lens rim with a chromatic fringe
      let rim = (vec3f(dist) - a.z * vec3f(0.95, 1.0, 1.05)) / 0.015;
      rgb += exp(-rim * rim) * abs(a.w) * 0.5;
    } else if (kind == 1u) { // faint spiral arms
      let arms = 0.5 + 0.5 * sin(atan2(d.y, d.x) * 3.0 - dist * 18.0 * sign(a.w) + u.time * 2.0);
      rgb += vec3f(0.3, 0.5, 1.0) * exp(-dist * dist / (a.z * a.z)) * pow(arms, 8.0)
           * abs(a.w) * 0.04;
    } else if (kind == 2u) { // shockwave ring
      let ring = (vec3f(dist) - a.z * vec3f(0.98, 1.0, 1.02)) / 0.012;
      rgb += exp(-ring * ring) * a.w * 5.0;
    } else {                 // shimmering band
      let across = dot(d, b.yz) / a.z;
      let along = dot(d, vec2f(-b.z, b.y));
      rgb += vec3f(0.5, 0.8, 1.0) * exp(-across * across * 4.0)
           * (0.5 + 0.5 * sin(along * 30.0 - u.time * 4.0)) * a.w * 0.5;
    }
  }
  return vec4f(rgb * u.live, 1.0);
}

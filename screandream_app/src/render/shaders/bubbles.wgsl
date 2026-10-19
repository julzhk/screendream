// Iridescent bubbles in a Mediterranean sea (prefixed with common.wgsl at load time).
//   vs_sea/fs_sea        the undersea scene, plus ripple rings where bubbles pop
//   vs_bubble/fs_bubble  one quad per bubble: wobbly outline, lens refraction of the scene,
//                        thin-film iridescence, highlights (alpha-blended)
// Scene space: x ∈ [-aspect, aspect], y ∈ [-1, 1], matching bubble_sim.rs.
struct Sea {
  header: vec4f, // water drift, bass-hit impulse, current, jellyfish pulse
  bubbles: array<vec4f, 320>, // per bubble: (x, y, radius, wobble), (phase, film, level, seed)
  pops: array<vec4f, 24>,     // per pop: (x, y, age, size)
};

@group(1) @binding(0) var<uniform> s: Sea;

const SURFACE_Y: f32 = 0.93;   // matches bubble_sim.rs
const POP_LIFETIME: f32 = 1.4;

// ---------------------------------------------------------------- building blocks

// Classic iterative water caustics: bright wandering light network.
fn caustic(p: vec2f, t: f32) -> f32 {
  let q = p * 5.0;
  var i = q;
  var c = 1.0;
  let inten = 0.005;
  for (var n = 0; n < 4; n++) {
    let tt = t * (1.0 - 3.5 / f32(n + 1));
    i = q + vec2f(cos(tt - i.x) + sin(tt + i.y), sin(tt - i.y) + cos(tt + i.x));
    c += 1.0 / length(vec2f(q.x / ((sin(i.x + tt) + 1e-4) / inten),
                            q.y / ((cos(i.y + tt) + 1e-4) / inten)));
  }
  c = 1.17 - pow(c / 4.0, 1.4);
  return clamp(pow(abs(c), 8.0), 0.0, 1.0);
}

fn water(p: vec2f) -> vec3f {
  let h = clamp((p.y + 1.0) * 0.5, 0.0, 1.0); // 0 at the bottom, 1 at the top
  var col = mix(vec3f(0.0, 0.1, 0.24), vec3f(0.0, 0.4, 0.52), smoothstep(0.0, 0.55, h));
  return mix(col, vec3f(0.1, 0.8, 0.78), smoothstep(0.5, 1.0, h));
}

// Slanting shafts of sunlight from the surface.
fn sun_rays(p: vec2f) -> f32 {
  let x = p.x + (SURFACE_Y - p.y) * 0.35 + 0.1 * sin(u.time * 0.2);
  let r = noise(vec2f(x * 3.0, u.time * 0.15)) * noise(vec2f(x * 7.3 + 4.0, u.time * 0.23));
  return r * r * smoothstep(-0.9, 0.9, p.y);
}

fn sand_top(x: f32) -> f32 {
  return -0.8 + 0.035 * sin(x * 2.1 + 1.0) + 0.04 * fbm(vec2f(x * 3.0, 9.0));
}

fn ground_top(x: f32) -> f32 {
  let rocky = smoothstep(0.3, 0.45, noise(vec2f(x * 0.8 + 2.0, 2.0)));
  let rock = -0.55 + 0.22 * fbm(vec2f(x * 1.6 + 11.0, 4.2));
  return max(sand_top(x), mix(sand_top(x), rock, rocky));
}

// Posidonia seagrass meadows: tall ribbon blades swaying in the swell.
fn seagrass(p: vec2f, base_y: f32) -> f32 {
  let meadow = smoothstep(0.35, 0.55, noise(vec2f(p.x * 1.3, 5.0)));
  if (meadow <= 0.0 || p.y < base_y) { return 0.0; }
  let bx = p.x * 30.0;
  var m = 0.0;
  for (var dc = -1; dc <= 1; dc++) {
    let cell = floor(bx) + f32(dc);
    let len = (0.08 + 0.16 * hash2(vec2f(cell, 3.0))) * meadow;
    let y01 = (p.y - base_y) / len;
    if (y01 > 1.0) { continue; }
    // Swaying in the swell, bending further the way the beat-driven current flows.
    let sway = (sin(u.time * 0.9 + cell * 0.35) * (0.6 + 0.8 * u.amplitude) + s.header.z * 12.0)
      * y01 * y01;
    let d = abs(bx - (cell + 0.5 + sway));
    m = max(m, step(d, 0.16 * (1.0 - 0.6 * y01)));
  }
  return m;
}

// Signed-ish distance helpers.
fn ellipse(q: vec2f, r: vec2f) -> f32 {
  return (length(q / r) - 1.0) * min(r.x, r.y);
}

fn box(q: vec2f, half: vec2f) -> f32 {
  let d = abs(q) - half;
  return length(max(d, vec2f(0.0))) + min(max(d.x, d.y), 0.0);
}

fn rotate(q: vec2f, a: f32) -> vec2f {
  let c = cos(a); let sn = sin(a);
  return vec2f(c * q.x + sn * q.y, -sn * q.x + c * q.y);
}

// A terracotta amphora lying half buried in the sand.
fn amphora(p: vec2f) -> f32 {
  let x0 = u.aspect * 0.38;
  let q = rotate(p - vec2f(x0, sand_top(x0) + 0.03), 1.1);
  var d = ellipse(q, vec2f(0.05, 0.11));
  d = min(d, box(q - vec2f(0.0, 0.13), vec2f(0.016, 0.04)));
  d = min(d, box(q - vec2f(0.0, 0.17), vec2f(0.026, 0.008)));
  let lug = abs(length((q - vec2f(0.0, 0.13)) / vec2f(1.0, 1.3)) - 0.035) - 0.006;
  d = min(d, max(lug, -q.y + 0.1));
  return d;
}

// A fallen, fluted marble column.
fn column(p: vec2f) -> vec2f {
  let x0 = -u.aspect * 0.42;
  let q = rotate(p - vec2f(x0, sand_top(x0) + 0.02), -0.12);
  let broken = q.x - 0.17 - 0.02 * sin(q.y * 90.0);
  let d = max(box(q, vec2f(0.2, 0.035)), broken);
  let flute = 0.5 + 0.5 * cos(q.y * 180.0);
  return vec2f(d, flute);
}

fn fish(p: vec2f, col0: vec3f) -> vec3f {
  var col = col0;
  for (var sc = 0; sc < 2; sc++) {
    let fs = f32(sc);
    let dir = select(1.0, -1.0, sc == 1);
    let lead = vec2f(
      dir * ((fract(u.time * 0.018 + fs * 0.5)) * 2.0 - 1.0) * (u.aspect + 0.6),
      0.05 + 0.3 * fs + 0.05 * sin(u.time * 0.3 + fs));
    for (var i = 0; i < 9; i++) {
      let fi = f32(i);
      let off = (vec2f(hash2(vec2f(fi, fs + 1.0)), hash2(vec2f(fi, fs + 7.0))) - 0.5) * vec2f(0.35, 0.16)
        + 0.01 * vec2f(sin(u.time * 1.3 + fi), cos(u.time * 1.7 + fi * 2.0));
      var q = (p - lead - off) * vec2f(dir, 1.0);
      let body = ellipse(q, vec2f(0.028, 0.009));
      q.y += sin(u.time * 12.0 + fi) * 0.003 * smoothstep(-0.01, -0.04, q.x);
      let tail = max(abs(q.y) - (-q.x - 0.022) * 0.8, max(q.x + 0.022, -q.x - 0.042));
      let m = 1.0 - smoothstep(0.0, 0.002, min(body, tail));
      let glint = smoothstep(0.004, 0.0, abs(q.y - 0.004)) * step(abs(q.x), 0.02);
      col = mix(col, vec3f(0.03, 0.14, 0.2) + vec3f(0.4, 0.6, 0.6) * glint * 0.4, m * 0.75);
    }
  }
  return col;
}

// A drifting jellyfish that contracts on bass hits.
fn jellyfish(p: vec2f, col0: vec3f) -> vec3f {
  var col = col0;
  let t = u.time;
  let pulse = s.header.w;
  let centre = vec2f(-u.aspect * 0.5 + 0.12 * sin(t * 0.07), 0.25 + 0.12 * sin(t * 0.11));
  let r = vec2f(0.085 * (1.0 - 0.2 * pulse), 0.065 * (1.0 + 0.12 * pulse));
  let q = p - centre;

  // Trailing tentacles.
  for (var k = 0; k < 7; k++) {
    let fk = f32(k);
    let x0 = centre.x + (fk / 6.0 - 0.5) * r.x * 1.6;
    let below = centre.y - p.y;
    let wave = 0.015 * sin(below * 25.0 - t * 3.0 + fk * 1.3) * below * 4.0;
    let line = smoothstep(0.003, 0.0, abs(p.x - x0 - wave)) * step(0.0, below)
      * (1.0 - smoothstep(0.05, 0.32, below));
    col += vec3f(0.8, 0.55, 0.95) * line * 0.25;
  }

  // Bell: translucent with a glowing rim and four inner rings.
  let e = length(q / r);
  let bell = step(-0.01, q.y) * (1.0 - smoothstep(0.96, 1.0, e));
  let rim = pow(clamp(e, 0.0, 1.0), 5.0);
  let rings = smoothstep(0.02, 0.0, abs(length((q - vec2f(0.0, 0.02)) / r * 3.0 - vec2f(0.0, 0.0)) - 0.9)) * 0.5;
  let jelly = vec3f(0.9, 0.55, 0.95) * (0.2 + 0.9 * rim + rings) * (1.0 + 0.8 * pulse);
  col = mix(col, col * 0.6 + jelly, bell * 0.6);
  return col;
}

// ---------------------------------------------------------------- the scene

fn sea(p: vec2f) -> vec3f {
  let t = u.time;
  var col = water(p);
  col += vec3f(0.55, 0.95, 0.9) * sun_rays(p) * (0.3 + 0.5 * u.bands.y);

  // Distant reef, hazy blue.
  let far = -0.3 + 0.3 * fbm(vec2f(p.x * 2.2 + 3.0, 1.7));
  col = mix(col, vec3f(0.0, 0.26, 0.36), (1.0 - smoothstep(far - 0.004, far + 0.004, p.y)) * 0.55);

  col = fish(p, col);
  col = jellyfish(p, col);

  let light = caustic(vec2f(p.x, p.y * 1.6) + vec2f(0.0, t * 0.02), t * 0.6) * (0.5 + 0.9 * u.bands.z);
  let ground = ground_top(p.x);
  let sand = sand_top(p.x);

  // Rocks, then sand with ripples, both dappled by caustics.
  if (p.y < ground) {
    let rock = vec3f(0.1, 0.19, 0.2) * (0.55 + 0.7 * fbm(p * 9.0));
    col = mix(rock, water(p), 0.25) + vec3f(0.4, 0.8, 0.75) * light * 0.35;
  }
  if (p.y < sand) {
    let ripples = 0.85 + 0.15 * sin(p.x * 70.0 + sin(p.y * 40.0) * 2.0);
    let sand_col = vec3f(0.62, 0.55, 0.38) * ripples;
    col = mix(sand_col, water(p), 0.45) + vec3f(0.6, 0.95, 0.85) * light * 0.3;
  }

  // Ancient relics on the seabed.
  let jar = amphora(p);
  let jar_m = 1.0 - smoothstep(0.0, 0.002, jar);
  col = mix(col, mix(vec3f(0.5, 0.22, 0.1), water(p), 0.35) + vec3f(0.5, 0.8, 0.7) * light * 0.3, jar_m);
  let pillar = column(p);
  let pillar_m = 1.0 - smoothstep(0.0, 0.002, pillar.x);
  let marble = vec3f(0.75, 0.76, 0.72) * (0.75 + 0.25 * pillar.y);
  col = mix(col, mix(marble, water(p), 0.4) + vec3f(0.5, 0.9, 0.8) * light * 0.35, pillar_m);

  // Seagrass over everything on the bottom.
  let grass = seagrass(p, ground);
  let tip = clamp((p.y - ground) * 6.0, 0.0, 1.0);
  col = mix(col, mix(vec3f(0.02, 0.15, 0.07), vec3f(0.12, 0.36, 0.1), tip) + vec3f(0.3, 0.6, 0.4) * light * 0.2, grass * 0.9);

  // Plankton glinting in the light, drifting upwards.
  let g = (p - vec2f(s.header.x, 0.0)) * 55.0 + vec2f(t * 0.4, -t * 1.2);
  let cell = floor(g);
  let spark = step(0.985, hash2(cell)) * (1.0 - smoothstep(0.03, 0.09, length(fract(g) - 0.5)));
  col += vec3f(0.8, 1.0, 0.95) * spark * (0.25 + 1.2 * u.bands.w);

  // Underside of the surface: bright, rippling, criss-crossed with light.
  let wave = 0.006 * sin(p.x * 7.0 - t) + 0.004 * sin(p.x * 18.0 + t * 1.5);
  let above = smoothstep(-0.015, 0.005, p.y - SURFACE_Y - wave);
  let sheen = vec3f(0.3, 0.82, 0.82) * (0.6 + 0.7 * caustic(vec2f(p.x, p.y * 4.0), t * 0.8));
  col = mix(col, sheen, above);

  // Bass hits brighten the whole sea a little.
  return col * (1.0 + 0.12 * s.header.y);
}

// ---------------------------------------------------------------- passes

struct SeaOut {
  @builtin(position) clip: vec4f,
  @location(0) uv: vec2f,
};

@vertex
fn vs_sea(@builtin(vertex_index) vertex_index: u32) -> SeaOut {
  let pos = fullscreen_pos(vertex_index);
  return SeaOut(vec4f(pos, 0.0, 1.0), pos);
}

@fragment
fn fs_sea(in: SeaOut) -> @location(0) vec4f {
  let p = vec2f(in.uv.x * u.aspect, in.uv.y);
  var col = sea(p);
  // Ripple rings spreading across the surface where bubbles popped.
  for (var i = 0; i < 24; i++) {
    let pop = s.pops[i];
    if (pop.z >= POP_LIFETIME) { continue; }
    let life = 1.0 - pop.z / POP_LIFETIME;
    let ring = pop.w * (1.0 + pop.z * 3.0);
    let d = abs(length((p - pop.xy) * vec2f(1.0, 3.0)) - ring);
    col += vec3f(0.85, 1.0, 1.0) * exp(-d * 140.0) * life * 0.8;
  }
  let vignette = 1.0 - 0.35 * smoothstep(0.6, 1.6, length(in.uv * vec2f(1.0, 0.8)));
  return idle_fade(col * vignette);
}

struct BubbleOut {
  @builtin(position) clip: vec4f,
  @location(0) local: vec2f, // in units of the bubble radius
  @location(1) world: vec2f,
  @location(2) @interpolate(flat) index: u32,
};

@vertex
fn vs_bubble(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> BubbleOut {
  var corners = array<vec2f, 6>(vec2f(-1.0, -1.0), vec2f(1.0, -1.0), vec2f(1.0, 1.0),
    vec2f(-1.0, -1.0), vec2f(1.0, 1.0), vec2f(-1.0, 1.0));
  let a = s.bubbles[ii * 2u];
  let local = corners[vi] * (1.1 + a.w * 0.3);
  let world = a.xy + local * a.z;
  return BubbleOut(vec4f(world.x / u.aspect, world.y, 0.0, 1.0), local, world, ii);
}

@fragment
fn fs_bubble(in: BubbleOut) -> @location(0) vec4f {
  let a = s.bubbles[in.index * 2u];
  let b = s.bubbles[in.index * 2u + 1u];
  let q = in.local;
  let ang = atan2(q.y, q.x);
  let ph = b.x;

  // Wobbly outline: a few travelling lobes whose size follows the band's level.
  let shape = 1.0 + a.w * (0.12 * sin(2.0 * ang + ph) + 0.07 * sin(3.0 * ang - ph * 1.3)
    + 0.04 * sin(5.0 * ang + ph * 0.7));
  let d = length(q) / shape;
  let aa = fwidth(d) * 1.5;
  let edge = 1.0 - smoothstep(1.0 - aa, 1.0, d);
  if (edge <= 0.0) { discard; }

  let dd = min(d, 1.0);
  let nxy = q / shape;
  let nz = sqrt(max(1.0 - dd * dd, 0.0));
  let n = vec3f(nxy, nz);

  // An air bubble in water is a diverging lens: the sea behind appears shrunk and bent.
  let sp = a.xy + (in.world - a.xy) * (0.35 + 0.25 * nz) - nxy * a.z * 0.25;
  var col = sea(sp) * 0.92;

  // Thin-film interference: colour depends on film thickness and viewing angle.
  let fres = pow(1.0 - nz, 2.2);
  let thick = b.y + 0.6 * nz + 0.3 * noise(q * 1.7 + vec2f(ph * 0.3, b.w * 10.0)) + u.time * 0.03;
  let film = 0.5 + 0.5 * cos(TAU * (thick * 1.6 + vec3f(0.0, 0.33, 0.67)));
  col = mix(col, film * 1.15, fres * (0.75 + 0.25 * b.z));
  col += film * 0.14 * (0.5 + b.z);

  // The sunlit surface reflected top-left, a faint crescent below, and a silvery rim.
  let key = pow(max(dot(n, normalize(vec3f(-0.45, 0.6, 0.66))), 0.0), 90.0) * 1.6;
  let soft = pow(max(dot(n, normalize(vec3f(-0.3, 0.4, 0.86))), 0.0), 12.0) * 0.1;
  let low = pow(max(dot(n, normalize(vec3f(0.35, -0.6, 0.72))), 0.0), 25.0) * 0.35;
  col += vec3f(1.0, 1.0, 0.95) * (key + soft) + vec3f(0.6, 1.0, 0.95) * low;
  col += vec3f(0.75, 0.95, 1.0) * smoothstep(0.82, 1.0, dd) * 0.45;

  return vec4f(idle_fade(col).rgb, edge);
}

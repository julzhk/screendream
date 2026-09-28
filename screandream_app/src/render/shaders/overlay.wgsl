// Translucent rounded-rectangle panels drawn behind overlay text so it stays legible
// over the animation. One instance per panel; rects are in physical pixels.
struct Screen {
  size: vec2f,
  _pad: vec2f,
};

@group(0) @binding(0) var<uniform> screen: Screen;

struct Panel {
  @location(0) rect: vec4f,   // x, y, width, height
  @location(1) color: vec4f,  // straight (non-premultiplied) rgba
  @location(2) radius: f32,
};

struct VsOut {
  @builtin(position) clip: vec4f,
  @location(0) local: vec2f,
  @location(1) half_size: vec2f,
  @location(2) color: vec4f,
  @location(3) radius: f32,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32, panel: Panel) -> VsOut {
  let corner = vec2f(f32(vertex_index & 1u), f32(vertex_index >> 1u));
  let pixel = panel.rect.xy + corner * panel.rect.zw;
  let ndc = vec2f(pixel.x / screen.size.x * 2.0 - 1.0, 1.0 - pixel.y / screen.size.y * 2.0);
  let half_size = panel.rect.zw * 0.5;
  return VsOut(vec4f(ndc, 0.0, 1.0), (corner - 0.5) * panel.rect.zw, half_size, panel.color, panel.radius);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4f {
  let q = abs(in.local) - in.half_size + vec2f(in.radius);
  let dist = length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0) - in.radius;
  let coverage = clamp(0.5 - dist, 0.0, 1.0);
  return vec4f(in.color.rgb, in.color.a * coverage);
}

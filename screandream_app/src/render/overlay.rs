//! Draws `ui::overlay::OverlayContent` on top of the visualizer: glyphon text over
//! translucent rounded panels.

use crate::ui::overlay::{OverlayContent, Tone};
use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport, Weight, cosmic_text::Align,
};

const PANEL: [f32; 4] = [0.02, 0.01, 0.06, 0.72];
const WHITE: Color = Color::rgb(245, 245, 250);
const MUTED: Color = Color::rgb(190, 188, 210);
const ACCENT: Color = Color::rgb(160, 200, 255);
const WARNING: Color = Color::rgb(255, 196, 92);

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PanelInstance {
    rect: [f32; 4],
    color: [f32; 4],
    radius: f32,
}

/// One piece of text whose buffer is reshaped only when its text, size or wrap width changes.
struct Label {
    buffer: Buffer,
    key: (String, u32, u32),
    weight: Weight,
    /// Logical font size; line height is 1.35× this.
    size: f32,
}

impl Label {
    fn new(font_system: &mut FontSystem, size: f32, weight: Weight) -> Self {
        Self {
            buffer: Buffer::new(font_system, Metrics::new(size, size * 1.35)),
            key: (String::new(), 0, 0),
            weight,
            size,
        }
    }

    /// Shapes `text` centred and wrapped to `max_width`, returning its (width, height).
    fn layout(
        &mut self,
        fs: &mut FontSystem,
        text: &str,
        scale: f32,
        max_width: f32,
    ) -> (f32, f32) {
        let key = (text.to_string(), scale.to_bits(), max_width.to_bits());
        if key != self.key {
            let size = self.size * scale;
            self.buffer.set_metrics(fs, Metrics::new(size, size * 1.35));
            self.buffer.set_size(fs, Some(max_width), None);
            self.buffer.set_text(
                fs,
                text,
                Attrs::new().family(Family::SansSerif).weight(self.weight),
                Shaping::Advanced,
            );
            // Wrap at the maximum width, then shrink the box to the widest line so
            // centring happens within the text's own width.
            self.buffer.shape_until_scroll(fs, false);
            let width = self.measure().0.ceil();
            self.buffer.set_size(fs, Some(width), None);
            for line in &mut self.buffer.lines {
                line.set_align(Some(Align::Center));
            }
            self.buffer.shape_until_scroll(fs, false);
            self.key = key;
        }
        self.measure()
    }

    fn measure(&self) -> (f32, f32) {
        self.buffer
            .layout_runs()
            .fold((0.0f32, 0.0f32), |(w, h), run| {
                (w.max(run.line_w), h.max(run.line_top + run.line_height))
            })
    }
}

/// Per-frame overlay inputs.
pub struct OverlayFrame<'a> {
    pub content: &'a OverlayContent,
    /// Window scale factor (physical pixels per logical point).
    pub scale: f32,
    /// Opacity of the bottom key hints, so they can fade out.
    pub hint_alpha: f32,
}

struct Placed {
    label: usize,
    left: f32,
    top: f32,
    color: Color,
}

pub struct OverlayRenderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
    viewport: Viewport,
    atlas: TextAtlas,
    text: TextRenderer,
    labels: Vec<Label>,
    pipeline: wgpu::RenderPipeline,
    screen_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
    panel_count: u32,
    placed: Vec<Placed>,
}

const STATUS: usize = 0;
const TITLE: usize = 1;
const BODY: usize = 2;
const ACTIONS: usize = 3;
const HINT: usize = 4;
const MAX_PANELS: u64 = 4;

impl OverlayRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let mut font_system = FontSystem::new();
        // cosmic-text's default sans family lacks a bold face on macOS and falls back to a
        // monospace font; Helvetica Neue ships with every Mac in all the weights used here.
        #[cfg(target_os = "macos")]
        font_system.db_mut().set_sans_serif_family("Helvetica Neue");
        let cache = Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        let mut atlas = TextAtlas::new(device, queue, &cache, format);
        let text = TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);
        let labels = vec![
            Label::new(&mut font_system, 16.0, Weight::MEDIUM),
            Label::new(&mut font_system, 30.0, Weight::BOLD),
            Label::new(&mut font_system, 18.0, Weight::NORMAL),
            Label::new(&mut font_system, 16.0, Weight::MEDIUM),
            Label::new(&mut font_system, 14.0, Weight::NORMAL),
        ];

        let screen_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("overlay screen"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("overlay"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("overlay"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: screen_buffer.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("overlay.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/overlay.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("overlay panels"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<PanelInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x4, 1 => Float32x4, 2 => Float32
                    ],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("overlay panels"),
            size: MAX_PANELS * std::mem::size_of::<PanelInstance>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            font_system,
            swash_cache: SwashCache::new(),
            viewport,
            atlas,
            text,
            labels,
            pipeline,
            screen_buffer,
            bind_group,
            instance_buffer,
            panel_count: 0,
            placed: Vec::new(),
        }
    }

    /// Lays out the overlay for a `width`×`height` physical-pixel surface.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &OverlayFrame,
        width: u32,
        height: u32,
    ) {
        let (content, scale, hint_alpha) = (frame.content, frame.scale, frame.hint_alpha);
        let (w, h) = (width as f32, height as f32);
        let margin = 28.0 * scale;
        let max_text = (w - 4.0 * margin).max(64.0);
        let mut panels = Vec::new();
        self.placed.clear();
        let fs = &mut self.font_system;

        if let Some(status) = &content.status {
            let pad = (18.0 * scale, 10.0 * scale);
            let (tw, th) = self.labels[STATUS].layout(fs, &status.text, scale, max_text);
            let rect = [
                (w - tw) / 2.0 - pad.0,
                margin,
                tw + 2.0 * pad.0,
                th + 2.0 * pad.1,
            ];
            panels.push(panel(rect, PANEL, rect[3] / 2.0));
            self.placed.push(Placed {
                label: STATUS,
                left: rect[0] + pad.0,
                top: rect[1] + pad.1,
                color: match status.tone {
                    Tone::Info => WHITE,
                    Tone::Warning => WARNING,
                },
            });
        }

        if let Some(dialog) = &content.dialog {
            let pad = 36.0 * scale;
            let wrap = max_text.min(680.0 * scale);
            let gap = 14.0 * scale;
            let sizes = [
                self.labels[TITLE].layout(fs, &dialog.title, scale, wrap),
                self.labels[BODY].layout(fs, &dialog.body, scale, wrap),
                self.labels[ACTIONS].layout(fs, &dialog.actions, scale, wrap),
            ];
            let inner_w = sizes.iter().map(|s| s.0).fold(0.0, f32::max);
            let inner_h = sizes.iter().map(|s| s.1).sum::<f32>() + gap * 3.0;
            let rect = [
                (w - inner_w) / 2.0 - pad,
                (h - inner_h) / 2.0 - pad,
                inner_w + 2.0 * pad,
                inner_h + 2.0 * pad,
            ];
            panels.push(panel(rect, PANEL, 18.0 * scale));
            let mut top = rect[1] + pad;
            for (index, (label, color)) in [(TITLE, WHITE), (BODY, MUTED), (ACTIONS, ACCENT)]
                .into_iter()
                .enumerate()
            {
                // Extra space separates the key actions from the explanation.
                if label == ACTIONS {
                    top += gap * 2.0;
                }
                self.placed.push(Placed {
                    label,
                    left: (w - sizes[index].0) / 2.0,
                    top,
                    color,
                });
                top += sizes[index].1 + gap;
            }
        }

        if let Some(hint) = content.hint.as_deref().filter(|_| hint_alpha > 0.01) {
            let pad = (16.0 * scale, 8.0 * scale);
            let (tw, th) = self.labels[HINT].layout(fs, hint, scale, max_text);
            let rect = [
                (w - tw) / 2.0 - pad.0,
                h - margin - th - 2.0 * pad.1,
                tw + 2.0 * pad.0,
                th + 2.0 * pad.1,
            ];
            let mut color = PANEL;
            color[3] *= hint_alpha;
            panels.push(panel(rect, color, rect[3] / 2.0));
            self.placed.push(Placed {
                label: HINT,
                left: rect[0] + pad.0,
                top: rect[1] + pad.1,
                color: Color::rgba(MUTED.r(), MUTED.g(), MUTED.b(), (hint_alpha * 255.0) as u8),
            });
        }

        self.panel_count = panels.len() as u32;
        queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&panels));
        queue.write_buffer(
            &self.screen_buffer,
            0,
            bytemuck::cast_slice(&[w, h, 0.0, 0.0]),
        );
        self.viewport.update(queue, Resolution { width, height });

        let areas = self.placed.iter().map(|placed| TextArea {
            buffer: &self.labels[placed.label].buffer,
            left: placed.left,
            top: placed.top,
            scale: 1.0,
            bounds: TextBounds {
                left: 0,
                top: 0,
                right: width as i32,
                bottom: height as i32,
            },
            default_color: placed.color,
            custom_glyphs: &[],
        });
        if let Err(error) = self.text.prepare(
            device,
            queue,
            &mut self.font_system,
            &mut self.atlas,
            &self.viewport,
            areas,
            &mut self.swash_cache,
        ) {
            eprintln!("Screendream: overlay text skipped: {error}");
            self.placed.clear();
        }
    }

    pub fn render<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        if self.panel_count > 0 {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
            pass.draw(0..4, 0..self.panel_count);
        }
        if !self.placed.is_empty() {
            let _ = self.text.render(&self.atlas, &self.viewport, pass);
        }
    }

    pub fn trim(&mut self) {
        self.atlas.trim();
    }
}

fn panel(rect: [f32; 4], color: [f32; 4], radius: f32) -> PanelInstance {
    PanelInstance {
        rect,
        color,
        radius,
    }
}

//! Pluggable visualizer modes. Each mode owns its pipelines and GPU resources; the shared
//! `Controls` uniform (time, aspect, audio levels) is bound at group 0 by `GpuRenderer`
//! before `draw`, and a mode may bind its own resources from group 1 upwards.
//!
//! To add a mode: implement [`VisualizerMode`] in a new file and register it in [`build_all`].

mod canyon;
mod crystal;
mod windmill;

use crate::render::{renderer::VisualizerUniform, scene_mesh::SceneMesh};

pub use canyon::CanyonMode;
pub use crystal::CrystalMode;
pub use windmill::WindmillMode;

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Mode shown first when the app starts.
pub const FIRST_MODE: &str = WindmillMode::NAME;

pub struct ModeContext<'a> {
    pub device: &'a wgpu::Device,
    pub format: wgpu::TextureFormat,
    /// Layout of the shared `Controls` uniform at group 0.
    pub controls_layout: &'a wgpu::BindGroupLayout,
}

pub struct FrameInput<'a> {
    pub uniform: &'a VisualizerUniform,
    pub time: f32,
    pub active: bool,
}

pub trait VisualizerMode {
    fn name(&self) -> &'static str;

    /// Called when the mode becomes the visible one.
    fn activate(&mut self) {}

    /// Upload per-frame data before the render pass begins.
    fn prepare(&mut self, _queue: &wgpu::Queue, _frame: &FrameInput) {}

    /// Record draw calls. The pass has a colour target, a cleared depth buffer and the
    /// shared controls bound at group 0.
    fn draw(&self, pass: &mut wgpu::RenderPass<'_>);
}

pub fn build_all(ctx: &ModeContext, scene: &SceneMesh) -> Vec<Box<dyn VisualizerMode>> {
    vec![
        Box::new(CrystalMode::new(ctx, scene)),
        Box::new(CanyonMode::new(ctx)),
        Box::new(WindmillMode::new(ctx)),
    ]
}

/// Describes a render pipeline in the shape every mode uses: one colour target, depth test.
pub(crate) struct PipelineSpec<'a> {
    pub label: &'a str,
    pub layout: &'a wgpu::PipelineLayout,
    pub shader: &'a wgpu::ShaderModule,
    pub vs: &'a str,
    pub fs: &'a str,
    pub buffers: &'a [wgpu::VertexBufferLayout<'a>],
    pub depth_write: bool,
}

pub(crate) fn pipeline(ctx: &ModeContext, spec: PipelineSpec) -> wgpu::RenderPipeline {
    ctx.device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(spec.label),
            layout: Some(spec.layout),
            vertex: wgpu::VertexState {
                module: spec.shader,
                entry_point: Some(spec.vs),
                buffers: spec.buffers,
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: spec.shader,
                entry_point: Some(spec.fs),
                targets: &[Some(wgpu::ColorTargetState {
                    format: ctx.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: spec.depth_write,
                depth_compare: if spec.depth_write {
                    wgpu::CompareFunction::Less
                } else {
                    wgpu::CompareFunction::Always
                },
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
}

/// A mode shader that relies on `common.wgsl` (shared `Controls`, noise, helpers).
pub(crate) fn shader_with_common(
    ctx: &ModeContext,
    label: &str,
    source: &'static str,
) -> wgpu::ShaderModule {
    ctx.device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Wgsl(
                format!("{}\n{source}", include_str!("../shaders/common.wgsl")).into(),
            ),
        })
}

pub(crate) fn shader(ctx: &ModeContext, label: &str, source: &'static str) -> wgpu::ShaderModule {
    ctx.device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        })
}

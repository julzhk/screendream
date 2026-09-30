//! The original mode: a nebula backdrop with a spectrum arc, and the bundled PLY crystal
//! surrounded by an orbiting ring of shards.

use super::{ModeContext, PipelineSpec, VisualizerMode, pipeline, shader};
use crate::render::scene_mesh::SceneMesh;
use wgpu::util::DeviceExt;

const MESH_INSTANCES: u32 = 1 + 18;

pub struct CrystalMode {
    background: wgpu::RenderPipeline,
    mesh: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

impl CrystalMode {
    pub const NAME: &'static str = "Crystal";

    pub fn new(ctx: &ModeContext, scene: &SceneMesh) -> Self {
        let layout = ctx
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("crystal"),
                bind_group_layouts: &[ctx.controls_layout],
                push_constant_ranges: &[],
            });
        let background_shader = shader(
            ctx,
            "post_process.wgsl",
            include_str!("../shaders/post_process.wgsl"),
        );
        let mesh_shader = shader(
            ctx,
            "visualizer.wgsl",
            include_str!("../shaders/visualizer.wgsl"),
        );

        let background = pipeline(
            ctx,
            PipelineSpec {
                label: "crystal background",
                layout: &layout,
                shader: &background_shader,
                vs: "vs_main",
                fs: "fs_main",
                buffers: &[],
                depth_write: false,
            },
        );
        let mesh = pipeline(
            ctx,
            PipelineSpec {
                label: "crystal mesh",
                layout: &layout,
                shader: &mesh_shader,
                vs: "vs_main",
                fs: "fs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 12,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3],
                }],
                depth_write: true,
            },
        );

        let vertex_buffer = ctx
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("scene vertices"),
                contents: bytemuck::cast_slice(&scene.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let index_buffer = ctx
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("scene indices"),
                contents: bytemuck::cast_slice(&scene.indices),
                usage: wgpu::BufferUsages::INDEX,
            });

        Self {
            background,
            mesh,
            vertex_buffer,
            index_buffer,
            index_count: scene.indices.len() as u32,
        }
    }
}

impl VisualizerMode for CrystalMode {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.background);
        pass.draw(0..3, 0..1);
        pass.set_pipeline(&self.mesh);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..MESH_INSTANCES);
    }
}

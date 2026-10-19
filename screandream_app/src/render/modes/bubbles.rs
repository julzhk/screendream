//! Iridescent bubbles rising through a turquoise Mediterranean sea. The bubbles come from
//! `render::bubble_sim`; this mode uploads them and draws the sea behind them. The sea is
//! painted procedurally, so each bubble refracts it by evaluating the same sea function at
//! a shrunken, bent position — a true lens effect without an extra render pass.

use super::{
    FrameInput, ModeContext, PipelineSpec, VisualizerMode, pipeline, pipeline_with_blend,
    shader_with_common,
};
use crate::analysis::audio_features::SPECTRUM_BINS;
use crate::render::bubble_sim::{BANDS, BubbleSim, MAX_BUBBLES, MAX_POPS};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SeaParams {
    /// Water drift, bass-hit impulse, current, jellyfish pulse.
    header: [f32; 4],
    /// Two vec4s per bubble: (x, y, radius, wobble) and (phase, film, level, seed).
    bubbles: [[f32; 4]; MAX_BUBBLES * 2],
    /// Per pop: (x, y, age, size).
    pops: [[f32; 4]; MAX_POPS],
}

pub struct BubblesMode {
    sea: wgpu::RenderPipeline,
    bubbles: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    params_buffer: wgpu::Buffer,
    params: Box<SeaParams>,
    sim: BubbleSim,
    bubble_count: u32,
    last_time: Option<f32>,
    last_bass: f32,
    kick: f32,
    pulse: f32,
}

impl BubblesMode {
    pub const NAME: &'static str = "Bubbles";

    pub fn new(ctx: &ModeContext) -> Self {
        let device = ctx.device;
        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sea params"),
            size: std::mem::size_of::<SeaParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sea"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sea"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buffer.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sea"),
            bind_group_layouts: &[ctx.controls_layout, &layout],
            push_constant_ranges: &[],
        });
        let module =
            shader_with_common(ctx, "bubbles.wgsl", include_str!("../shaders/bubbles.wgsl"));
        let spec = |label, vs, fs| PipelineSpec {
            label,
            layout: &pipeline_layout,
            shader: &module,
            vs,
            fs,
            buffers: &[],
            depth_write: false,
        };
        let sea = pipeline(ctx, spec("sea", "vs_sea", "fs_sea"));
        let bubbles = pipeline_with_blend(
            ctx,
            spec("bubbles", "vs_bubble", "fs_bubble"),
            wgpu::BlendState::ALPHA_BLENDING,
        );

        Self {
            sea,
            bubbles,
            bind_group,
            params_buffer,
            params: Box::new(bytemuck::Zeroable::zeroed()),
            sim: new_sim(),
            bubble_count: 0,
            last_time: None,
            last_bass: 0.0,
            kick: 0.0,
            pulse: 0.0,
        }
    }
}

impl VisualizerMode for BubblesMode {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn activate(&mut self) {
        self.sim = new_sim();
        self.last_time = None;
    }

    fn prepare(&mut self, queue: &wgpu::Queue, frame: &FrameInput) {
        let dt = self
            .last_time
            .map_or(0.0, |last| (frame.time - last).clamp(0.0, 0.1));
        self.last_time = Some(frame.time);

        let audio = frame.uniform;
        let bins_per_band = SPECTRUM_BINS / BANDS;
        let levels: [f32; BANDS] = std::array::from_fn(|band| {
            if !frame.active {
                return 0.0;
            }
            let bins = &audio.spectrum[band * bins_per_band..(band + 1) * bins_per_band];
            bins.iter().sum::<f32>() / bins_per_band as f32
        });
        let bass = if frame.active { audio.bands[0] } else { 0.0 };
        let rise = (bass - self.last_bass).max(0.0);
        self.last_bass = bass;
        self.kick = (self.kick * (1.0 - dt * 4.0).max(0.0) + rise * 3.0).min(1.5);
        // The jellyfish pulses on bass hits and relaxes slowly.
        self.pulse = (self.pulse * (1.0 - dt * 1.5).max(0.0)).max(self.kick.min(1.0));

        let amplitude = if frame.active { audio.amplitude } else { 0.0 };
        self.sim
            .step(dt, frame.aspect, &levels, amplitude, self.kick);

        // Small bubbles first so the big ones are drawn in front.
        let mut order: Vec<_> = self.sim.bubbles().iter().collect();
        order.sort_by(|a, b| a.radius.total_cmp(&b.radius));
        for (i, b) in order.iter().enumerate() {
            self.params.bubbles[i * 2] = [b.pos[0], b.pos[1], b.radius, b.wobble];
            self.params.bubbles[i * 2 + 1] = [b.phase, b.film, b.level, b.seed];
        }
        let pops = self.sim.pops();
        for (index, slot) in self.params.pops.iter_mut().enumerate() {
            *slot = pops.get(index).map_or([0.0, 0.0, 99.0, 0.0], |p| {
                [p.pos[0], p.pos[1], p.age, p.size]
            });
        }
        self.bubble_count = order.len() as u32;
        self.params.header = [self.sim.drift(), self.kick, self.sim.current(), self.pulse];
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&*self.params));
    }

    fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_bind_group(1, &self.bind_group, &[]);
        pass.set_pipeline(&self.sea);
        pass.draw(0..3, 0..1);
        if self.bubble_count > 0 {
            pass.set_pipeline(&self.bubbles);
            pass.draw(0..6, 0..self.bubble_count);
        }
    }
}

fn new_sim() -> BubbleSim {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |elapsed| elapsed.as_nanos() as u64);
    BubbleSim::new(seed)
}

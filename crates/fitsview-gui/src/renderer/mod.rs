use fitsview_core::{colormap::Colormap, scale::ScaleMode};

pub mod cpu;
#[cfg(feature = "wgpu")]
pub mod gpu;

pub trait Renderer: Send {
    fn render(
        &self,
        pixels: &[f32],
        width: usize,
        height: usize,
        vmin: f32,
        vmax: f32,
        scale_mode: ScaleMode,
        colormap: Colormap,
    ) -> Vec<u8>;
}

#[cfg(feature = "wgpu")]
pub fn create_renderer(
    render_state: Option<&egui_wgpu::RenderState>,
) -> Box<dyn Renderer> {
    if let Some(rs) = render_state {
        if let Some(gpu) = gpu::GpuRenderer::try_new(rs) {
            return Box::new(gpu);
        }
    }
    Box::new(cpu::CpuRenderer)
}

#[cfg(not(feature = "wgpu"))]
pub fn create_renderer() -> Box<dyn Renderer> {
    Box::new(cpu::CpuRenderer)
}

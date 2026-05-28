use fitsview_core::{colormap::Colormap, scale::ScaleMode};

pub mod cpu;
#[cfg(feature = "gpu")]
pub mod gpu;

/// All parameters needed to render a pixel buffer to RGBA.
pub struct RenderParams<'a> {
    pub pixels: &'a [f32],
    pub width: usize,
    pub height: usize,
    pub vmin: f32,
    pub vmax: f32,
    pub scale_mode: ScaleMode,
    pub colormap: Colormap,
    pub contrast: f32,
    pub bias: f32,
    /// 65536-entry HistEq LUT from `build_histeq_lut`; None when not using HistEq.
    pub histeq_lut: Option<&'a [f32]>,
}

pub trait Renderer: Send {
    fn render(&self, params: &RenderParams<'_>) -> Vec<u8>;
}

#[cfg(feature = "gpu")]
pub fn create_renderer(
    render_state: Option<&eframe::egui_wgpu::RenderState>,
) -> Box<dyn Renderer> {
    if let Some(rs) = render_state {
        if let Some(g) = gpu::GpuRenderer::try_new(rs) {
            return Box::new(g);
        }
    }
    Box::new(cpu::CpuRenderer)
}

#[cfg(not(feature = "gpu"))]
pub fn create_renderer() -> Box<dyn Renderer> {
    Box::new(cpu::CpuRenderer)
}

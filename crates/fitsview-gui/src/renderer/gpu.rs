use fitsview_core::{colormap::Colormap, scale::ScaleMode};

use super::{cpu::CpuRenderer, Renderer};

/// GPU renderer using wgpu compute shaders for tonemapping + colormap.
/// Falls back to CPU if GPU initialisation fails.
pub struct GpuRenderer {
    _render_state: egui_wgpu::RenderState,
    _cpu_fallback: CpuRenderer,
}

impl GpuRenderer {
    /// Returns `None` if the GPU device is unavailable; caller falls back to CPU.
    pub fn try_new(render_state: &egui_wgpu::RenderState) -> Option<Self> {
        let _device = &render_state.device;
        // TODO: create compute pipeline, LUT texture
        // For now we wrap the render_state but use CPU path until pipeline is built.
        Some(Self {
            _render_state: render_state.clone(),
            _cpu_fallback: CpuRenderer,
        })
    }
}

impl Renderer for GpuRenderer {
    fn render(
        &self,
        pixels: &[f32],
        width: usize,
        height: usize,
        vmin: f32,
        vmax: f32,
        scale_mode: ScaleMode,
        colormap: Colormap,
    ) -> Vec<u8> {
        // TODO: dispatch compute shader; use CPU path until pipeline is complete.
        self._cpu_fallback.render(pixels, width, height, vmin, vmax, scale_mode, colormap)
    }
}

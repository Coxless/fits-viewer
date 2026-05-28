use super::{cpu::CpuRenderer, RenderParams, Renderer};

/// GPU renderer using wgpu compute shaders for tonemapping + colormap LUT.
/// Falls back to CPU rendering until the compute pipeline is complete.
pub struct GpuRenderer {
    _render_state: eframe::egui_wgpu::RenderState,
    cpu: CpuRenderer,
}

impl GpuRenderer {
    pub fn try_new(render_state: &eframe::egui_wgpu::RenderState) -> Option<Self> {
        let _device = &render_state.device;
        log::info!("GPU renderer initialised (wgpu adapter: {:?})", render_state.adapter.get_info().name);
        Some(Self { _render_state: render_state.clone(), cpu: CpuRenderer })
    }
}

impl Renderer for GpuRenderer {
    fn render(&self, params: &RenderParams<'_>) -> Vec<u8> {
        self.cpu.render(params)
    }
}

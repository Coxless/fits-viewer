use fitsview_core::{colormap::Colormap, scale::ScaleMode};

use super::{cpu::CpuRenderer, Renderer};

/// GPU renderer using wgpu compute shaders for tonemapping + colormap LUT.
/// Falls back to CPU rendering until the compute pipeline is complete.
/// When `try_new` returns `None`, the caller uses `CpuRenderer` instead.
pub struct GpuRenderer {
    // Store a clone of the render state so we can use the device/queue later
    // when the compute pipeline is wired up.
    _render_state: eframe::egui_wgpu::RenderState, // kept alive so wgpu resources aren't freed
    cpu: CpuRenderer,
}

impl GpuRenderer {
    /// Attempt to initialise the GPU renderer.
    /// Returns `None` if the GPU device is not available.
    pub fn try_new(render_state: &eframe::egui_wgpu::RenderState) -> Option<Self> {
        // Verify the device is available (accessing it confirms wgpu is up)
        let _device = &render_state.device;
        log::info!("GPU renderer initialised (wgpu adapter: {:?})", render_state.adapter.get_info().name);

        // TODO: create compute pipeline, LUT texture, buffer pool.
        // For now we delegate to the CPU path; the GPU pipeline will be
        // implemented incrementally once the rest of Step 3 is stable.
        Some(Self { _render_state: render_state.clone(), cpu: CpuRenderer })
    }
}

impl Renderer for GpuRenderer {
    #[allow(clippy::too_many_arguments)]
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
        // Delegate to CPU renderer until the compute pipeline is built.
        self.cpu.render(pixels, width, height, vmin, vmax, scale_mode, colormap)
    }
}

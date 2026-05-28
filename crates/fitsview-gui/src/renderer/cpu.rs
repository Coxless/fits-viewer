use fitsview_core::colormap::render_to_rgba;

use super::{RenderParams, Renderer};

pub struct CpuRenderer;

impl Renderer for CpuRenderer {
    fn render(&self, p: &RenderParams<'_>) -> Vec<u8> {
        render_to_rgba(
            p.pixels,
            p.vmin,
            p.vmax,
            p.colormap,
            p.scale_mode,
            p.contrast,
            p.bias,
            p.histeq_lut,
        )
    }
}

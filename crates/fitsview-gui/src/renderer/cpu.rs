use fitsview_core::{colormap::{apply_colormap, Colormap}, scale::ScaleMode};
use rayon::prelude::*;

use super::Renderer;

pub struct CpuRenderer;

impl Renderer for CpuRenderer {
    fn render(
        &self,
        pixels: &[f32],
        _width: usize,
        _height: usize,
        vmin: f32,
        vmax: f32,
        scale_mode: ScaleMode,
        colormap: Colormap,
    ) -> Vec<u8> {
        let range = vmax - vmin;
        let mut out = vec![0u8; pixels.len() * 4];
        out.par_chunks_mut(4)
            .zip(pixels.par_iter())
            .for_each(|(rgba, &v)| {
                let norm = if range > 0.0 {
                    ((v - vmin) / range).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let norm = apply_transfer(norm, scale_mode);
                let c = apply_colormap(norm, colormap);
                rgba.copy_from_slice(&c);
            });
        out
    }
}

fn apply_transfer(t: f32, mode: ScaleMode) -> f32 {
    match mode {
        ScaleMode::Linear | ScaleMode::ZScale | ScaleMode::MinMax => t,
        ScaleMode::Log => (t * 999.0 + 1.0).log10() / 3.0,
        ScaleMode::Sqrt => t.sqrt(),
        ScaleMode::Asinh => (t.asinh() / std::f32::consts::PI).clamp(0.0, 1.0),
    }
}

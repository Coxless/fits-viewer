use rayon::prelude::*;

use crate::scale::{apply_contrast_bias, apply_transfer, ScaleMode};

pub struct RgbChannel {
    pub data: Vec<f32>,
    pub vmin: f32,
    pub vmax: f32,
    pub scale: ScaleMode,
    pub contrast: f32,
    pub bias: f32,
}

pub struct RgbCompositeData {
    pub r: RgbChannel,
    pub g: RgbChannel,
    pub b: RgbChannel,
    pub width: usize,
    pub height: usize,
}

impl RgbCompositeData {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        r_data: Vec<f32>, r_vmin: f32, r_vmax: f32, r_scale: ScaleMode,
        g_data: Vec<f32>, g_vmin: f32, g_vmax: f32, g_scale: ScaleMode,
        b_data: Vec<f32>, b_vmin: f32, b_vmax: f32, b_scale: ScaleMode,
        width: usize, height: usize,
    ) -> Self {
        Self {
            r: RgbChannel { data: r_data, vmin: r_vmin, vmax: r_vmax, scale: r_scale, contrast: 1.0, bias: 0.5 },
            g: RgbChannel { data: g_data, vmin: g_vmin, vmax: g_vmax, scale: g_scale, contrast: 1.0, bias: 0.5 },
            b: RgbChannel { data: b_data, vmin: b_vmin, vmax: b_vmax, scale: b_scale, contrast: 1.0, bias: 0.5 },
            width,
            height,
        }
    }
}

fn normalize_pixel(v: f32, ch: &RgbChannel) -> u8 {
    let range = (ch.vmax - ch.vmin).max(f32::EPSILON);
    let norm = ((v - ch.vmin) / range).clamp(0.0, 1.0);
    let t = apply_transfer(norm, ch.scale);
    let t = apply_contrast_bias(t, ch.contrast, ch.bias);
    (t * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Render three single-channel FITS images into an RGBA buffer.
///
/// Each channel is independently scaled. Channels that are shorter than
/// `width * height` are zero-padded.
pub fn render_rgb_to_rgba(composite: &RgbCompositeData) -> Vec<u8> {
    let n = composite.width * composite.height;
    let mut rgba = vec![0u8; n * 4];

    rgba.par_chunks_mut(4).enumerate().for_each(|(i, px)| {
        let r = composite.r.data.get(i).copied().unwrap_or(0.0);
        let g = composite.g.data.get(i).copied().unwrap_or(0.0);
        let b = composite.b.data.get(i).copied().unwrap_or(0.0);
        px[0] = normalize_pixel(r, &composite.r);
        px[1] = normalize_pixel(g, &composite.g);
        px[2] = normalize_pixel(b, &composite.b);
        px[3] = 255;
    });

    rgba
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_rgb() {
        let comp = RgbCompositeData::new(
            vec![0.0, 1.0], 0.0, 1.0, ScaleMode::Linear,
            vec![0.5, 0.5], 0.0, 1.0, ScaleMode::Linear,
            vec![1.0, 0.0], 0.0, 1.0, ScaleMode::Linear,
            2, 1,
        );
        let rgba = render_rgb_to_rgba(&comp);
        assert_eq!(rgba.len(), 8);
        assert_eq!(rgba[3], 255); // alpha is 255
        assert_eq!(rgba[0], 0);   // r channel first pixel = 0
        assert_eq!(rgba[4], 255); // r channel second pixel = 255
    }
}

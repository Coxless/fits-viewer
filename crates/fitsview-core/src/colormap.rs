use std::sync::OnceLock;

use crate::scale::{apply_transfer, ScaleMode};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Colormap {
    Gray,
    Viridis,
    Plasma,
    Inferno,
    Hot,
    Rainbow,
}

pub fn apply_colormap(value: f32, cmap: Colormap) -> [u8; 4] {
    let t = value.clamp(0.0, 1.0);
    let [r, g, b] = match cmap {
        Colormap::Gray    => gray_lut()[lut_idx(t)],
        Colormap::Viridis => lerp_lut(VIRIDIS_CTRL, t),
        Colormap::Plasma  => lerp_lut(PLASMA_CTRL, t),
        Colormap::Inferno => lerp_lut(INFERNO_CTRL, t),
        Colormap::Hot     => hot_lut()[lut_idx(t)],
        Colormap::Rainbow => rainbow_lut()[lut_idx(t)],
    };
    [r, g, b, 255]
}

pub fn render_to_rgba(data: &[f32], vmin: f32, vmax: f32, cmap: Colormap, scale_mode: ScaleMode) -> Vec<u8> {
    let range = (vmax - vmin).max(f32::EPSILON);
    let mut rgba = Vec::with_capacity(data.len() * 4);
    for &v in data {
        let t = apply_transfer(((v - vmin) / range).clamp(0.0, 1.0), scale_mode);
        let [r, g, b, a] = apply_colormap(t, cmap);
        rgba.extend_from_slice(&[r, g, b, a]);
    }
    rgba
}

fn lut_idx(t: f32) -> usize {
    (t * 255.0).round() as usize
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + t * (b as f32 - a as f32)).round() as u8
}

fn lerp_lut(control: &[[u8; 3]], t: f32) -> [u8; 3] {
    let n = control.len() - 1;
    let pos = t.clamp(0.0, 1.0) * n as f32;
    let lo = (pos.floor() as usize).min(n);
    let hi = (lo + 1).min(n);
    let frac = pos - lo as f32;
    [
        lerp_u8(control[lo][0], control[hi][0], frac),
        lerp_u8(control[lo][1], control[hi][1], frac),
        lerp_u8(control[lo][2], control[hi][2], frac),
    ]
}

// ---------------------------------------------------------------------------
// Gray LUT
// ---------------------------------------------------------------------------

fn gray_lut() -> &'static [[u8; 3]; 256] {
    static CACHE: OnceLock<[[u8; 3]; 256]> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mut lut = [[0u8; 3]; 256];
        for (i, entry) in lut.iter_mut().enumerate() {
            *entry = [i as u8; 3];
        }
        lut
    })
}

// ---------------------------------------------------------------------------
// Hot LUT — R ramp, then G ramp, then B ramp
// ---------------------------------------------------------------------------

fn hot_lut() -> &'static [[u8; 3]; 256] {
    static CACHE: OnceLock<[[u8; 3]; 256]> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mut lut = [[0u8; 3]; 256];
        for (i, entry) in lut.iter_mut().enumerate() {
            let r = ((i as f32 / 85.0) * 255.0).clamp(0.0, 255.0) as u8;
            let g = if i < 85 { 0 } else { (((i - 85) as f32 / 85.0) * 255.0).clamp(0.0, 255.0) as u8 };
            let b = if i < 170 { 0 } else { (((i - 170) as f32 / 85.0) * 255.0).clamp(0.0, 255.0) as u8 };
            *entry = [r, g, b];
        }
        lut
    })
}

// ---------------------------------------------------------------------------
// Rainbow LUT — HSV hue rotation 240° (blue) → 0° (red)
// ---------------------------------------------------------------------------

fn rainbow_lut() -> &'static [[u8; 3]; 256] {
    static CACHE: OnceLock<[[u8; 3]; 256]> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mut lut = [[0u8; 3]; 256];
        for (i, entry) in lut.iter_mut().enumerate() {
            let hue = (1.0 - i as f32 / 255.0) * 240.0; // 240 → 0 degrees
            *entry = hsv_to_rgb(hue, 1.0, 1.0);
        }
        lut
    })
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let h = h.rem_euclid(360.0);
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    ]
}

// ---------------------------------------------------------------------------
// Viridis control points (matplotlib, RGB 0–255)
// ---------------------------------------------------------------------------

const VIRIDIS_CTRL: &[[u8; 3]] = &[
    [68,  1,   84],
    [72,  40,  120],
    [62,  74,  137],
    [49,  104, 142],
    [38,  130, 142],
    [31,  158, 137],
    [53,  183, 121],
    [110, 206, 88],
    [181, 222, 43],
    [253, 231, 37],
];

// ---------------------------------------------------------------------------
// Plasma control points (matplotlib, RGB 0–255)
// ---------------------------------------------------------------------------

const PLASMA_CTRL: &[[u8; 3]] = &[
    [13,  8,   135],
    [75,  3,   161],
    [125, 3,   168],
    [168, 34,  150],
    [203, 70,  121],
    [229, 107, 93],
    [248, 148, 65],
    [253, 195, 40],
    [240, 249, 33],
];

// ---------------------------------------------------------------------------
// Inferno control points (matplotlib, RGB 0–255)
// ---------------------------------------------------------------------------

const INFERNO_CTRL: &[[u8; 3]] = &[
    [0,   0,   4],
    [31,  12,  72],
    [85,  15,  109],
    [136, 34,  106],
    [186, 54,  85],
    [227, 89,  51],
    [249, 140, 10],
    [249, 201, 50],
    [252, 255, 164],
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gray_boundaries() {
        assert_eq!(apply_colormap(0.0, Colormap::Gray), [0, 0, 0, 255]);
        assert_eq!(apply_colormap(1.0, Colormap::Gray), [255, 255, 255, 255]);
    }

    #[test]
    fn test_render_to_rgba_length() {
        let data = vec![0.0_f32, 0.5, 1.0];
        let result = render_to_rgba(&data, 0.0, 1.0, Colormap::Viridis, crate::scale::ScaleMode::Linear);
        assert_eq!(result.len(), 12);
    }

    #[test]
    fn test_cmap_clamp_no_panic() {
        let _ = apply_colormap(-1.0, Colormap::Hot);
        let _ = apply_colormap(2.0, Colormap::Hot);
    }

    #[test]
    fn test_all_cmaps_have_alpha_255() {
        for cmap in [
            Colormap::Gray,
            Colormap::Viridis,
            Colormap::Plasma,
            Colormap::Inferno,
            Colormap::Hot,
            Colormap::Rainbow,
        ] {
            let px = apply_colormap(0.5, cmap);
            assert_eq!(px[3], 255, "{cmap:?} alpha must be 255");
        }
    }
}

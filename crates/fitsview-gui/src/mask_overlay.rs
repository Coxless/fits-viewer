use std::collections::HashMap;

use egui::{Color32, ColorImage, Painter, Rect, TextureHandle, TextureOptions};

use crate::viewport::ViewState;

/// A binary or multi-bit mask overlay.
pub struct MaskOverlay {
    pub data: Vec<u8>,
    pub width: usize,
    pub height: usize,
    pub default_color: Color32,
    pub bit_colors: HashMap<u8, Color32>,
    pub visible: bool,
    pub name: String,
    texture: Option<TextureHandle>,
}

impl MaskOverlay {
    pub fn new(data: Vec<u8>, width: usize, height: usize, name: String) -> Self {
        Self {
            data,
            width,
            height,
            default_color: Color32::from_rgba_unmultiplied(255, 30, 30, 140),
            bit_colors: HashMap::new(),
            visible: true,
            name,
            texture: None,
        }
    }

    fn build_texture(&mut self, ctx: &egui::Context) {
        let n = self.width * self.height;
        let mut rgba = vec![0u8; n * 4];

        for (i, &v) in self.data[..n.min(self.data.len())].iter().enumerate() {
            if v == 0 {
                continue; // transparent
            }
            let color = self.bit_colors.get(&v).copied().unwrap_or(self.default_color);
            let base = i * 4;
            rgba[base]     = color.r();
            rgba[base + 1] = color.g();
            rgba[base + 2] = color.b();
            rgba[base + 3] = color.a();
        }

        let ci = ColorImage::from_rgba_unmultiplied([self.width, self.height], &rgba);
        self.texture = Some(ctx.load_texture(
            format!("mask-{}", self.name),
            ci,
            TextureOptions::NEAREST,
        ));
    }

    pub fn invalidate(&mut self) {
        self.texture = None;
    }
}

/// Draw mask overlays on top of the image.
pub fn draw_masks(
    painter: &Painter,
    masks: &mut [MaskOverlay],
    panel_rect: Rect,
    view: &ViewState,
    ctx: &egui::Context,
) {
    for mask in masks {
        if !mask.visible || mask.width == 0 || mask.height == 0 {
            continue;
        }

        if mask.texture.is_none() {
            mask.build_texture(ctx);
        }

        if let Some(tex) = &mask.texture {
            let img_w = mask.width as f32;
            let img_h = mask.height as f32;
            let display_size = egui::vec2(img_w * view.zoom, img_h * view.zoom);
            let origin = panel_rect.min.to_vec2() + view.offset * view.zoom;
            let dest_rect = Rect::from_min_size(origin.to_pos2(), display_size);
            let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            painter.image(tex.id(), dest_rect, uv, Color32::WHITE);
        }
    }
}

/// Load a FITS image as a mask (non-zero pixels become the mask).
pub fn load_mask_from_fits(
    data: &[f32],
    width: usize,
    height: usize,
    name: String,
) -> MaskOverlay {
    let byte_data: Vec<u8> = data.iter()
        .map(|&v| if v != 0.0 && v.is_finite() { 1u8 } else { 0u8 })
        .collect();
    MaskOverlay::new(byte_data, width, height, name)
}

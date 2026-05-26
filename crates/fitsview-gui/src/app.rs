use std::path::PathBuf;

use egui::{vec2, Color32, ColorImage, Rect, TextureHandle, TextureOptions, ViewportCommand};
use fitsview_core::{
    colormap::{render_to_rgba, Colormap},
    fits_reader::{load_fits, FitsImage},
    scale::{compute_scale, ScaleMode},
};

use crate::{header_panel::HeaderPanel, viewport::ViewState};

pub struct FitsViewApp {
    image: Option<FitsImage>,
    texture: Option<TextureHandle>,
    scale_mode: ScaleMode,
    colormap: Colormap,
    view: ViewState,
    header_panel: HeaderPanel,
    title: String,
    needs_fit: bool,
}

impl FitsViewApp {
    pub fn new(_cc: &eframe::CreationContext<'_>, path: Option<PathBuf>) -> Self {
        let (image, title) = match path {
            Some(ref p) => match load_fits(p) {
                Ok(img) => {
                    let name = p
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "unknown".to_owned());
                    (Some(img), format!("fits-view — {}", name))
                }
                Err(e) => {
                    log::error!("Failed to load FITS: {e}");
                    (None, "fits-view — error loading file".to_owned())
                }
            },
            None => (None, "fits-view".to_owned()),
        };

        Self {
            image,
            texture: None,
            scale_mode: ScaleMode::ZScale,
            colormap: Colormap::Gray,
            view: ViewState::default(),
            header_panel: HeaderPanel::default(),
            title,
            needs_fit: true,
        }
    }
}

fn build_texture(
    ctx: &egui::Context,
    img: &FitsImage,
    mode: ScaleMode,
    cmap: Colormap,
) -> TextureHandle {
    let sr = compute_scale(&img.data, mode);
    let rgba = render_to_rgba(&img.data, sr.vmin, sr.vmax, cmap);
    let ci = ColorImage::from_rgba_unmultiplied([img.width, img.height], &rgba);
    ctx.load_texture("fits-image", ci, TextureOptions::LINEAR)
}

impl eframe::App for FitsViewApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.send_viewport_cmd(ViewportCommand::Title(self.title.clone()));

        // Keyboard shortcuts
        let mut do_fit = false;
        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::H) {
                self.header_panel.visible = !self.header_panel.visible;
            }
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::Num0) {
                do_fit = true;
            }
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::Plus)
                || i.consume_key(egui::Modifiers::CTRL, egui::Key::Equals)
            {
                self.view.zoom =
                    (self.view.zoom * ViewState::ZOOM_STEP).clamp(ViewState::ZOOM_MIN, ViewState::ZOOM_MAX);
            }
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::Minus) {
                self.view.zoom =
                    (self.view.zoom / ViewState::ZOOM_STEP).clamp(ViewState::ZOOM_MIN, ViewState::ZOOM_MAX);
            }
        });

        // Build texture lazily
        if self.texture.is_none() {
            if let Some(ref img) = self.image {
                self.texture = Some(build_texture(ctx, img, self.scale_mode, self.colormap));
            }
        }

        // Header panel — must precede CentralPanel
        if let Some(ref img) = self.image {
            self.header_panel.show(ctx, &img.header);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_rect_before_wrap();

            // Initial fit or Ctrl+0
            if self.needs_fit || do_fit {
                self.needs_fit = false;
                if let Some(ref img) = self.image {
                    let img_size = vec2(img.width as f32, img.height as f32);
                    self.view.fit_to_rect(available, img_size);
                }
            }

            let response = ui.allocate_rect(available, egui::Sense::drag());

            // Mouse wheel zoom
            let scroll_delta = ctx.input(|i| i.smooth_scroll_delta.y);
            if response.hovered() && scroll_delta.abs() > 0.1 {
                let factor = (scroll_delta / 50.0).exp();
                let cursor_pos = ctx
                    .input(|i| i.pointer.hover_pos())
                    .unwrap_or(available.center());
                // Convert cursor to panel-relative position
                let cursor_rel = cursor_pos.to_vec2() - available.min.to_vec2();
                self.view.zoom_toward(factor, cursor_rel);
            }

            // Mouse drag pan
            if response.dragged() {
                self.view.pan(response.drag_delta());
            }

            // Draw image
            if let (Some(ref texture), Some(ref img)) = (&self.texture, &self.image) {
                let img_size = vec2(img.width as f32, img.height as f32);
                let display_size = img_size * self.view.zoom;
                let display_origin = available.min.to_vec2() + self.view.offset * self.view.zoom;
                let display_rect =
                    Rect::from_min_size(display_origin.to_pos2(), display_size);

                let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                ui.painter()
                    .image(texture.id(), display_rect, uv, Color32::WHITE);
            } else {
                ui.centered_and_justified(|ui| {
                    ui.heading("No file loaded. Pass a FITS path as argument.");
                });
            }
        });
    }
}

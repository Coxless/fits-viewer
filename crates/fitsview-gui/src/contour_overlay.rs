use egui::{Color32, Painter, Pos2, Rect, Stroke};
use fitsview_core::contour::ContourLines;

use crate::viewport::ViewState;

/// Draw contour polylines on top of the image.
pub fn draw_contours(
    painter: &Painter,
    contour_lines: &ContourLines,
    panel_rect: Rect,
    view: &ViewState,
) {
    for (level, polylines) in contour_lines.levels.iter().zip(contour_lines.polylines.iter()) {
        let [r, g, b, a] = level.color;
        let color = Color32::from_rgba_unmultiplied(r, g, b, a);
        let stroke = Stroke::new(1.0, color);

        for poly in polylines {
            if poly.len() < 2 {
                continue;
            }
            for win in poly.windows(2) {
                let p0 = img_to_screen(win[0].0, win[0].1, panel_rect, view);
                let p1 = img_to_screen(win[1].0, win[1].1, panel_rect, view);
                if in_bounds(p0, panel_rect) || in_bounds(p1, panel_rect) {
                    painter.line_segment([p0, p1], stroke);
                }
            }
        }
    }
}

fn img_to_screen(ix: f32, iy: f32, panel_rect: Rect, view: &ViewState) -> Pos2 {
    Pos2::new(
        panel_rect.min.x + (ix + view.offset.x) * view.zoom,
        panel_rect.min.y + (iy + view.offset.y) * view.zoom,
    )
}

fn in_bounds(p: Pos2, rect: Rect) -> bool {
    p.x >= rect.min.x - 10.0
        && p.x <= rect.max.x + 10.0
        && p.y >= rect.min.y - 10.0
        && p.y <= rect.max.y + 10.0
}

/// Settings panel for contour configuration.
#[derive(Default)]
pub struct ContourSettings {
    pub visible: bool,
    /// Sigma multiples to use for auto-levels.
    pub sigma_levels: Vec<f32>,
    pub color: [u8; 4],
    pub custom_levels_str: String,
}

impl ContourSettings {
    /// Show the contour settings window.
    ///
    /// Returns `Some((mean, std))` (image statistics needed for sigma levels)
    /// if the user pressed "Apply" — the caller should compute the contours.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        image_mean: Option<f32>,
        image_std: Option<f32>,
    ) -> Option<ContourRequest> {
        if !self.visible {
            return None;
        }

        let mut request = None;

        egui::Window::new("Contour Settings")
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Sigma levels (comma-separated):");
                let sigma_str: String = self.sigma_levels.iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                let mut input = sigma_str.clone();
                if ui.text_edit_singleline(&mut input).changed() {
                    self.sigma_levels = input.split(',')
                        .filter_map(|s| s.trim().parse::<f32>().ok())
                        .collect();
                }

                ui.label("Color:");
                let mut color32 = egui::Color32::from_rgba_unmultiplied(
                    self.color[0], self.color[1], self.color[2], self.color[3]
                );
                if ui.color_edit_button_srgba(&mut color32).changed() {
                    self.color = [color32.r(), color32.g(), color32.b(), color32.a()];
                }

                ui.add_space(4.0);
                if ui.button("Apply").clicked() {
                    if let (Some(mean), Some(std)) = (image_mean, image_std) {
                        if self.sigma_levels.is_empty() {
                            self.sigma_levels = vec![2.0, 4.0, 8.0];
                        }
                        request = Some(ContourRequest {
                            mean,
                            std,
                            sigma_levels: self.sigma_levels.clone(),
                            color: self.color,
                        });
                    }
                }

                if ui.button("Clear Contours").clicked() {
                    request = Some(ContourRequest {
                        mean: 0.0, std: 0.0, sigma_levels: Vec::new(), color: self.color,
                    });
                }
            });

        request
    }
}

pub struct ContourRequest {
    pub mean: f32,
    pub std: f32,
    pub sigma_levels: Vec<f32>,
    pub color: [u8; 4],
}

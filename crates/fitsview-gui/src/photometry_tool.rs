use egui::{Color32, Painter, Pos2, Rect, Stroke};
use fitsview_core::photometry::ApertureResult;

use crate::viewport::ViewState;

/// Aperture photometry interaction state.
pub struct PhotometryTool {
    pub active: bool,
    pub aperture_radius: f64,
    pub sky_inner: f64,
    pub sky_outer: f64,
    pub results_visible: bool,
}

impl Default for PhotometryTool {
    fn default() -> Self {
        Self {
            active: false,
            aperture_radius: 5.0,
            sky_inner: 8.0,
            sky_outer: 14.0,
            results_visible: false,
        }
    }
}

/// Draw aperture circles for all photometry results.
pub fn draw_apertures(
    painter: &Painter,
    results: &[ApertureResult],
    panel_rect: Rect,
    view: &ViewState,
) {
    let ap_stroke = Stroke::new(1.5, Color32::from_rgb(0, 220, 255));
    let sky_stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 220, 255, 120));

    for r in results {
        let center = img_to_screen(r.x as f32, r.y as f32, panel_rect, view);
        let ap_r = r.aperture_radius as f32 * view.zoom;
        let sky_in = r.sky_inner as f32 * view.zoom;
        let sky_out = r.sky_outer as f32 * view.zoom;

        if !in_panel(center, panel_rect) {
            continue;
        }

        painter.circle_stroke(center, ap_r, ap_stroke);
        painter.circle_stroke(center, sky_in, sky_stroke);
        painter.circle_stroke(center, sky_out, sky_stroke);
    }
}

/// Draw the preview aperture at the cursor position.
pub fn draw_preview_aperture(
    painter: &Painter,
    tool: &PhotometryTool,
    cursor_pos: Pos2,
    view: &ViewState,
) {
    if !tool.active {
        return;
    }
    let stroke = Stroke::new(1.5, Color32::from_rgba_unmultiplied(255, 255, 0, 200));
    let sky_stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 0, 100));

    let ap_r = tool.aperture_radius as f32 * view.zoom;
    let sky_in = tool.sky_inner as f32 * view.zoom;
    let sky_out = tool.sky_outer as f32 * view.zoom;

    painter.circle_stroke(cursor_pos, ap_r, stroke);
    painter.circle_stroke(cursor_pos, sky_in, sky_stroke);
    painter.circle_stroke(cursor_pos, sky_out, sky_stroke);
}

/// Handle click to place a new aperture.
///
/// Returns `Some((x, y))` in image coordinates if a new aperture should be measured.
pub fn handle_click(
    tool: &PhotometryTool,
    response: &egui::Response,
    panel_rect: Rect,
    view: &ViewState,
) -> Option<(f64, f64)> {
    if !tool.active {
        return None;
    }
    if response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            let ix = (pos.x - panel_rect.min.x) / view.zoom - view.offset.x;
            let iy = (pos.y - panel_rect.min.y) / view.zoom - view.offset.y;
            return Some((ix as f64, iy as f64));
        }
    }
    None
}

/// Show the photometry results table.
pub fn show_results_panel(
    results: &[ApertureResult],
    tool: &mut PhotometryTool,
    ctx: &egui::Context,
) {
    if !tool.results_visible || results.is_empty() {
        return;
    }

    egui::Window::new("Photometry Results")
        .resizable(true)
        .default_size([460.0, 240.0])
        .show(ctx, |ui| {
            // Settings row
            ui.horizontal(|ui| {
                ui.label("Aperture (px):");
                ui.add(egui::DragValue::new(&mut tool.aperture_radius).range(1.0..=50.0).speed(0.5));
                ui.label("Sky inner:");
                ui.add(egui::DragValue::new(&mut tool.sky_inner).range(1.0..=100.0).speed(0.5));
                ui.label("Sky outer:");
                ui.add(egui::DragValue::new(&mut tool.sky_outer).range(2.0..=200.0).speed(0.5));

                if ui.small_button("Export CSV").clicked() {
                    let csv = export_csv(results);
                    ui.output_mut(|o| o.copied_text = csv);
                }
            });

            ui.separator();

            egui::ScrollArea::both().show(ui, |ui| {
                egui::Grid::new("phot_table")
                    .num_columns(6)
                    .striped(true)
                    .min_col_width(60.0)
                    .show(ui, |ui| {
                        // Header
                        ui.strong("X"); ui.strong("Y"); ui.strong("Flux");
                        ui.strong("Sky"); ui.strong("SNR"); ui.strong("Mag");
                        ui.end_row();

                        for r in results {
                            ui.label(format!("{:.1}", r.x));
                            ui.label(format!("{:.1}", r.y));
                            ui.label(format!("{:.1}", r.net_flux));
                            ui.label(format!("{:.3}", r.sky_mean));
                            ui.label(format!("{:.1}", r.snr));
                            ui.label(r.magnitude.map(|m| format!("{m:.3}")).unwrap_or_else(|| "--".to_owned()));
                            ui.end_row();
                        }
                    });
            });
        });
}

fn export_csv(results: &[ApertureResult]) -> String {
    let mut out = String::from("x,y,ra,dec,flux,sky_mean,snr,magnitude\n");
    for r in results {
        let ra = r.ra.map(|v| format!("{v:.6}")).unwrap_or_default();
        let dec = r.dec.map(|v| format!("{v:.6}")).unwrap_or_default();
        let mag = r.magnitude.map(|v| format!("{v:.4}")).unwrap_or_default();
        out.push_str(&format!("{:.2},{:.2},{ra},{dec},{:.4},{:.4},{:.2},{mag}\n",
            r.x, r.y, r.net_flux, r.sky_mean, r.snr));
    }
    out
}

fn img_to_screen(ix: f32, iy: f32, panel_rect: Rect, view: &ViewState) -> Pos2 {
    Pos2::new(
        panel_rect.min.x + (ix + view.offset.x) * view.zoom,
        panel_rect.min.y + (iy + view.offset.y) * view.zoom,
    )
}

fn in_panel(p: Pos2, rect: Rect) -> bool {
    p.x >= rect.min.x && p.x <= rect.max.x && p.y >= rect.min.y && p.y <= rect.max.y
}

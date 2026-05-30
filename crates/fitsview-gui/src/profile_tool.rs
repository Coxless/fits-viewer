use egui::{Color32, Painter, Pos2, Rect, Stroke};
use fitsview_core::profile::{fit_gaussian_1d, extract_profile, GaussianFit};

use crate::viewport::ViewState;

/// Interaction state for the line profile tool.
#[derive(Default)]
pub struct ProfileTool {
    pub active: bool,
    /// Start point in image coordinates (0-based).
    pub start: Option<(f64, f64)>,
    /// End point set when drag is complete.
    pub end: Option<(f64, f64)>,
    pub n_samples: usize,
    /// Most recent Gaussian fit result.
    pub gaussian_fit: Option<GaussianFit>,
    /// Whether to display fit result in next frame.
    pub show_fit: bool,
}

impl ProfileTool {
    pub fn new() -> Self {
        Self { n_samples: 200, ..Default::default() }
    }
}

/// Draw the active profile line.
pub fn draw_profile_line(
    painter: &Painter,
    tool: &ProfileTool,
    panel_rect: Rect,
    view: &ViewState,
    cursor_pos: Option<egui::Pos2>,
) {
    if !tool.active {
        return;
    }

    let stroke = Stroke::new(1.5, Color32::from_rgb(255, 220, 0));

    if let Some((x0, y0)) = tool.start {
        let p0 = img_to_screen(x0 as f32, y0 as f32, panel_rect, view);

        let p1 = if let Some((x1, y1)) = tool.end {
            img_to_screen(x1 as f32, y1 as f32, panel_rect, view)
        } else if let Some(cur) = cursor_pos {
            cur
        } else {
            return;
        };

        painter.line_segment([p0, p1], stroke);
        painter.circle_filled(p0, 3.0, Color32::from_rgb(255, 220, 0));
        painter.circle_filled(p1, 3.0, Color32::from_rgb(255, 220, 0));
    }
}

fn img_to_screen(ix: f32, iy: f32, panel_rect: Rect, view: &ViewState) -> Pos2 {
    Pos2::new(
        panel_rect.min.x + (ix + view.offset.x) * view.zoom,
        panel_rect.min.y + (iy + view.offset.y) * view.zoom,
    )
}

/// Handle drag events to define the profile line.
///
/// Returns a completed profile if the user finished a drag.
pub fn handle_profile_drag(
    tool: &mut ProfileTool,
    response: &egui::Response,
    panel_rect: Rect,
    view: &ViewState,
) -> bool {
    if !tool.active {
        return false;
    }

    if response.drag_started() {
        if let Some(pos) = response.interact_pointer_pos() {
            let ix = (pos.x - panel_rect.min.x) / view.zoom - view.offset.x;
            let iy = (pos.y - panel_rect.min.y) / view.zoom - view.offset.y;
            tool.start = Some((ix as f64, iy as f64));
            tool.end = None;
        }
    }

    if response.drag_stopped() {
        if let Some(pos) = response.interact_pointer_pos() {
            let ix = (pos.x - panel_rect.min.x) / view.zoom - view.offset.x;
            let iy = (pos.y - panel_rect.min.y) / view.zoom - view.offset.y;
            tool.end = Some((ix as f64, iy as f64));
            return true; // profile ready
        }
    }

    false
}

/// Extract the profile data and optionally run a Gaussian fit.
pub fn compute_and_show_profile(
    tool: &mut ProfileTool,
    data: &[f32],
    width: usize,
    height: usize,
    plot_panel: &mut crate::plot_panel::PlotPanel,
    run_fit: bool,
) {
    let Some((x0, y0)) = tool.start else { return };
    let Some((x1, y1)) = tool.end else { return };

    let profile = extract_profile(data, width, height, x0, y0, x1, y1, tool.n_samples);
    if profile.values.is_empty() {
        return;
    }

    if run_fit {
        tool.gaussian_fit = fit_gaussian_1d(&profile);
        tool.show_fit = tool.gaussian_fit.is_some();
    }

    let x: Vec<f64> = profile.distances.iter().map(|&d| d as f64).collect();
    let y: Vec<f64> = profile.values.iter().map(|&v| v as f64).collect();
    plot_panel.add_spectrum(x, y, "Line Profile".to_owned(), "Distance (px)".to_owned(), "Value".to_owned());
    plot_panel.visible = true;
}

/// Show the Gaussian fit result in a small floating window.
pub fn show_fit_result(tool: &ProfileTool, ctx: &egui::Context) {
    if !tool.show_fit {
        return;
    }
    if let Some(ref fit) = tool.gaussian_fit {
        egui::Window::new("Gaussian Fit")
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-8.0, 40.0))
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                egui::Grid::new("fit_grid").num_columns(2).show(ui, |ui| {
                    ui.label("Amplitude:"); ui.label(format!("{:.3}", fit.amplitude)); ui.end_row();
                    ui.label("Center:");    ui.label(format!("{:.3} px", fit.center));  ui.end_row();
                    ui.label("Sigma:");     ui.label(format!("{:.3} px", fit.sigma));   ui.end_row();
                    ui.label("FWHM:");      ui.label(format!("{:.3} px", fit.fwhm));    ui.end_row();
                    ui.label("Offset:");    ui.label(format!("{:.3}", fit.offset));     ui.end_row();
                });
            });
    }
}

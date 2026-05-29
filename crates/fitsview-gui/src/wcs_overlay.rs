use egui::{Color32, Painter, Rect, Stroke};
use fitsview_core::wcs_grid::compute_wcs_grid;

use crate::{tab_manager::Tab, viewport::ViewState};

/// Draw RA/Dec grid lines over the image viewport.
pub fn draw_wcs_grid(painter: &Painter, tab: &Tab, panel_rect: Rect, view: &ViewState) {
    let wcs = match &tab.wcs {
        Some(w) => w,
        None => return,
    };

    let w = tab.data.width();
    let h = tab.data.height();
    if w == 0 || h == 0 {
        return;
    }

    let grid = compute_wcs_grid(wcs, w, h);

    let grid_color = Color32::from_rgba_unmultiplied(180, 220, 255, 100);
    let stroke = Stroke::new(1.0, grid_color);
    let label_color = Color32::from_rgba_unmultiplied(180, 220, 255, 180);

    // Convert image-pixel coord to screen coord
    let to_screen = |px: f32, py: f32| -> egui::Pos2 {
        egui::pos2(
            panel_rect.min.x + (px + view.offset.x) * view.zoom,
            panel_rect.min.y + (py + view.offset.y) * view.zoom,
        )
    };

    for line in &grid.lines {
        if line.points.len() < 2 {
            continue;
        }

        // Draw line segments
        let screen_pts: Vec<egui::Pos2> = line
            .points
            .iter()
            .map(|&(px, py)| to_screen(px, py))
            .collect();

        for pair in screen_pts.windows(2) {
            let in_view = panel_rect.expand(20.0).contains(pair[0])
                || panel_rect.expand(20.0).contains(pair[1]);
            if in_view {
                painter.line_segment([pair[0], pair[1]], stroke);
            }
        }

        // Draw label at the first point inside the panel
        if let Some(&sp) = screen_pts.iter().find(|p| panel_rect.contains(**p)) {
            let offset = if line.is_ra { egui::vec2(2.0, 2.0) } else { egui::vec2(2.0, -2.0) };
            let anchor = if line.is_ra { egui::Align2::LEFT_TOP } else { egui::Align2::LEFT_BOTTOM };
            painter.text(sp + offset, anchor, &line.label, egui::FontId::monospace(9.0), label_color);
        }
    }
}

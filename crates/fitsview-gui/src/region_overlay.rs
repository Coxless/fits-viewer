use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2};
use fitsview_core::{
    region::{CoordFrame, RegionFile, RegionShape},
    wcs::Wcs,
};

use crate::viewport::ViewState;

/// Draw all loaded region files onto the image viewport.
pub fn draw_regions(
    painter: &Painter,
    region_files: &[RegionFile],
    panel_rect: Rect,
    view: &ViewState,
    img_width: usize,
    img_height: usize,
    wcs: Option<&Wcs>,
) {
    for rf in region_files {
        for region in &rf.regions {
            let [r, g, b] = region.style.color;
            let alpha = if region.excluded { 128u8 } else { 220u8 };
            let color = Color32::from_rgba_unmultiplied(r, g, b, alpha);
            let stroke = Stroke::new(region.style.line_width, color);

            let needs_wcs = matches!(region.frame, CoordFrame::Fk5 | CoordFrame::Icrs | CoordFrame::Galactic);
            if needs_wcs && wcs.is_none() {
                continue;
            }

            let to_screen = |ix: f64, iy: f64| -> Pos2 {
                if needs_wcs {
                    // For WCS regions, convert world→pixel first
                    let (px, py) = wcs
                        .and_then(|w| w.world_to_pixel(ix, iy))
                        .unwrap_or((ix, iy));
                    img_to_screen(px, py, panel_rect, view)
                } else {
                    // Image/Physical coords are 1-based in DS9; convert to 0-based
                    img_to_screen(ix - 1.0, iy - 1.0, panel_rect, view)
                }
            };

            let radius_to_screen = |r: f64| -> f32 { (r * view.zoom as f64) as f32 };

            match &region.shape {
                RegionShape::Circle { x, y, r } => {
                    let center = to_screen(*x, *y);
                    let r_px = radius_to_screen(*r);
                    if r_px > 0.5 && rect_contains_approx(panel_rect, center, r_px) {
                        painter.circle_stroke(center, r_px, stroke);
                    }
                }
                RegionShape::Ellipse { x, y, rx, ry, angle_deg } => {
                    let center = to_screen(*x, *y);
                    let rx_px = radius_to_screen(*rx);
                    let ry_px = radius_to_screen(*ry);
                    if rx_px > 0.5 || ry_px > 0.5 {
                        draw_ellipse(painter, center, rx_px, ry_px, *angle_deg as f32, stroke);
                    }
                }
                RegionShape::Box { x, y, w, h, angle_deg } => {
                    let center = to_screen(*x, *y);
                    let hw = radius_to_screen(*w * 0.5);
                    let hh = radius_to_screen(*h * 0.5);
                    draw_rotated_box(painter, center, hw, hh, *angle_deg as f32, stroke);
                }
                RegionShape::Polygon(pts) => {
                    if pts.len() < 3 {
                        continue;
                    }
                    let screen_pts: Vec<Pos2> = pts.iter().map(|(x, y)| to_screen(*x, *y)).collect();
                    let mut all_pts = screen_pts.clone();
                    all_pts.push(screen_pts[0]); // close
                    for win in all_pts.windows(2) {
                        painter.line_segment([win[0], win[1]], stroke);
                    }
                }
                RegionShape::Line { x1, y1, x2, y2 } => {
                    let p1 = to_screen(*x1, *y1);
                    let p2 = to_screen(*x2, *y2);
                    painter.line_segment([p1, p2], stroke);
                }
                RegionShape::Point { x, y } => {
                    let center = to_screen(*x, *y);
                    let size = 4.0_f32;
                    painter.line_segment([center - Vec2::X * size, center + Vec2::X * size], stroke);
                    painter.line_segment([center - Vec2::Y * size, center + Vec2::Y * size], stroke);
                }
                RegionShape::Annulus { x, y, r_inner, r_outer } => {
                    let center = to_screen(*x, *y);
                    let ri = radius_to_screen(*r_inner);
                    let ro = radius_to_screen(*r_outer);
                    if ri > 0.5 { painter.circle_stroke(center, ri, stroke); }
                    if ro > ri { painter.circle_stroke(center, ro, stroke); }
                }
                RegionShape::Text { x, y, text } => {
                    if !text.is_empty() {
                        let pos = to_screen(*x, *y);
                        painter.text(pos, egui::Align2::LEFT_TOP, text, egui::FontId::proportional(12.0), color);
                    }
                }
            }

            // Indicate exclusion with a dashed style visual cue (slightly different color)
            if region.excluded {
                // Already drawn with lower alpha above; no additional work needed
            }
            let _ = img_width; // reserved for future clipping
            let _ = img_height;
        }
    }
}

fn img_to_screen(ix: f64, iy: f64, panel_rect: Rect, view: &ViewState) -> Pos2 {
    Pos2::new(
        panel_rect.min.x + (ix as f32 + view.offset.x) * view.zoom,
        panel_rect.min.y + (iy as f32 + view.offset.y) * view.zoom,
    )
}

fn rect_contains_approx(rect: Rect, center: Pos2, r: f32) -> bool {
    center.x + r > rect.min.x
        && center.x - r < rect.max.x
        && center.y + r > rect.min.y
        && center.y - r < rect.max.y
}

fn draw_ellipse(painter: &Painter, center: Pos2, rx: f32, ry: f32, angle_deg: f32, stroke: Stroke) {
    let steps = 64usize;
    let angle_rad = angle_deg.to_radians();
    let cos_a = angle_rad.cos();
    let sin_a = angle_rad.sin();

    let pts: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let theta = i as f32 / steps as f32 * std::f32::consts::TAU;
            let ex = rx * theta.cos();
            let ey = ry * theta.sin();
            Pos2::new(
                center.x + ex * cos_a - ey * sin_a,
                center.y + ex * sin_a + ey * cos_a,
            )
        })
        .collect();

    for win in pts.windows(2) {
        painter.line_segment([win[0], win[1]], stroke);
    }
}

fn draw_rotated_box(painter: &Painter, center: Pos2, hw: f32, hh: f32, angle_deg: f32, stroke: Stroke) {
    let angle_rad = angle_deg.to_radians();
    let cos_a = angle_rad.cos();
    let sin_a = angle_rad.sin();

    let corners_local = [
        Vec2::new(-hw, -hh),
        Vec2::new( hw, -hh),
        Vec2::new( hw,  hh),
        Vec2::new(-hw,  hh),
    ];

    let pts: Vec<Pos2> = corners_local
        .iter()
        .map(|v| {
            Pos2::new(
                center.x + v.x * cos_a - v.y * sin_a,
                center.y + v.x * sin_a + v.y * cos_a,
            )
        })
        .collect();

    for i in 0..4 {
        painter.line_segment([pts[i], pts[(i + 1) % 4]], stroke);
    }
}

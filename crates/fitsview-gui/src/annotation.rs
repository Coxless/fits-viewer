use egui::Painter;
use fitsview_core::wcs::Wcs;
use serde::{Deserialize, Serialize};

use crate::viewport::ViewState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnnotationShape {
    Circle { cx: f64, cy: f64, r: f64 },
    Box { cx: f64, cy: f64, w: f64, h: f64, angle: f64 },
    Line { x1: f64, y1: f64, x2: f64, y2: f64 },
    Text { x: f64, y: f64, text: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub shape: AnnotationShape,
    pub color: [u8; 3],
    pub label: String,
    /// If true, coordinates are RA/Dec (degrees); otherwise image pixels (0-based).
    pub use_wcs: bool,
}

#[derive(Debug, Default, PartialEq, Clone)]
pub enum ShapeType {
    #[default]
    Circle,
    Box,
    Line,
    Text,
}

#[derive(Default)]
pub enum AnnotationMode {
    #[default]
    Disabled,
    Placing(ShapeType),
    Selected(usize),
}

#[derive(Default)]
pub struct AnnotationOverlay {
    pub mode: AnnotationMode,
    drag_start: Option<egui::Pos2>,
}

/// Convert an annotation coordinate to image pixels (0-based).
fn to_image_px(ax: f64, ay: f64, use_wcs: bool, wcs: Option<&Wcs>) -> Option<(f64, f64)> {
    if use_wcs {
        wcs?.world_to_pixel(ax, ay)
    } else {
        Some((ax, ay))
    }
}

fn img_to_screen(ix: f64, iy: f64, panel_rect: egui::Rect, view: &ViewState) -> egui::Pos2 {
    egui::pos2(
        panel_rect.min.x + (ix as f32 + view.offset.x) * view.zoom,
        panel_rect.min.y + (iy as f32 + view.offset.y) * view.zoom,
    )
}

pub fn draw_annotations(
    painter: &Painter,
    annotations: &[Annotation],
    view: &ViewState,
    panel_rect: egui::Rect,
    wcs: Option<&Wcs>,
) {
    for ann in annotations {
        let color = egui::Color32::from_rgb(ann.color[0], ann.color[1], ann.color[2]);
        let stroke = egui::Stroke::new(1.5, color);

        match &ann.shape {
            AnnotationShape::Circle { cx, cy, r } => {
                let Some((px, py)) = to_image_px(*cx, *cy, ann.use_wcs, wcs) else { continue };
                let center = img_to_screen(px, py, panel_rect, view);
                let radius = (*r * view.zoom as f64) as f32;
                painter.circle_stroke(center, radius, stroke);
                if !ann.label.is_empty() {
                    painter.text(
                        center + egui::vec2(radius + 4.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        &ann.label,
                        egui::FontId::proportional(12.0),
                        color,
                    );
                }
            }
            AnnotationShape::Box { cx, cy, w, h, angle } => {
                let Some((px, py)) = to_image_px(*cx, *cy, ann.use_wcs, wcs) else { continue };
                let center = img_to_screen(px, py, panel_rect, view);
                let hw = (*w / 2.0 * view.zoom as f64) as f32;
                let hh = (*h / 2.0 * view.zoom as f64) as f32;
                let a = angle.to_radians() as f32;
                let corners = [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)];
                let rotated: Vec<egui::Pos2> = corners
                    .iter()
                    .map(|(rx, ry)| {
                        center
                            + egui::vec2(
                                rx * a.cos() - ry * a.sin(),
                                rx * a.sin() + ry * a.cos(),
                            )
                    })
                    .collect();
                for i in 0..4 {
                    painter.line_segment([rotated[i], rotated[(i + 1) % 4]], stroke);
                }
            }
            AnnotationShape::Line { x1, y1, x2, y2 } => {
                let Some((px1, py1)) = to_image_px(*x1, *y1, ann.use_wcs, wcs) else { continue };
                let Some((px2, py2)) = to_image_px(*x2, *y2, ann.use_wcs, wcs) else { continue };
                let p1 = img_to_screen(px1, py1, panel_rect, view);
                let p2 = img_to_screen(px2, py2, panel_rect, view);
                painter.line_segment([p1, p2], stroke);
            }
            AnnotationShape::Text { x, y, text } => {
                let Some((px, py)) = to_image_px(*x, *y, ann.use_wcs, wcs) else { continue };
                let pos = img_to_screen(px, py, panel_rect, view);
                painter.text(
                    pos,
                    egui::Align2::LEFT_BOTTOM,
                    text,
                    egui::FontId::proportional(13.0),
                    color,
                );
            }
        }
    }
}

impl AnnotationOverlay {
    /// Process click/drag interactions and draw annotations.
    /// Returns true if annotations were modified.
    pub fn interact_and_draw(
        &mut self,
        annotations: &mut Vec<Annotation>,
        painter: &Painter,
        response: &egui::Response,
        panel_rect: egui::Rect,
        view: &ViewState,
        wcs: Option<&Wcs>,
    ) -> bool {
        draw_annotations(painter, annotations, view, panel_rect, wcs);

        let mut changed = false;

        match &self.mode {
            AnnotationMode::Placing(shape_type) => {
                if response.drag_started() {
                    self.drag_start = response.interact_pointer_pos();
                }

                if response.drag_stopped() {
                    if let Some(start) = self.drag_start.take() {
                        let end = response.interact_pointer_pos().unwrap_or(start);
                        let to_img = |screen: egui::Pos2| -> (f64, f64) {
                            let panel_pos = screen - panel_rect.min;
                            (
                                panel_pos.x as f64 / view.zoom as f64 - view.offset.x as f64,
                                panel_pos.y as f64 / view.zoom as f64 - view.offset.y as f64,
                            )
                        };
                        let (ix1, iy1) = to_img(start);
                        let (ix2, iy2) = to_img(end);

                        let (ax1, ay1, ax2, ay2) = if let Some(w) = wcs {
                            let w1 = w.pixel_to_world(ix1, iy1);
                            let w2 = w.pixel_to_world(ix2, iy2);
                            if let (Some((ra1, dec1)), Some((ra2, dec2))) = (w1, w2) {
                                (ra1, dec1, ra2, dec2)
                            } else {
                                (ix1, iy1, ix2, iy2)
                            }
                        } else {
                            (ix1, iy1, ix2, iy2)
                        };
                        let use_wcs = wcs.is_some();

                        let shape = match shape_type {
                            ShapeType::Circle => {
                                let r = ((ix2 - ix1).powi(2) + (iy2 - iy1).powi(2)).sqrt();
                                AnnotationShape::Circle { cx: ax1, cy: ay1, r }
                            }
                            ShapeType::Box => AnnotationShape::Box {
                                cx: (ax1 + ax2) / 2.0,
                                cy: (ay1 + ay2) / 2.0,
                                w: (ax2 - ax1).abs(),
                                h: (ay2 - ay1).abs(),
                                angle: 0.0,
                            },
                            ShapeType::Line => {
                                AnnotationShape::Line { x1: ax1, y1: ay1, x2: ax2, y2: ay2 }
                            }
                            ShapeType::Text => AnnotationShape::Text {
                                x: ax1,
                                y: ay1,
                                text: "Label".to_string(),
                            },
                        };

                        annotations.push(Annotation {
                            shape,
                            color: [255, 255, 0],
                            label: String::new(),
                            use_wcs,
                        });
                        changed = true;
                        self.mode = AnnotationMode::Disabled;
                    }
                }
            }
            AnnotationMode::Disabled => {
                // Click to select nearest annotation
                if response.clicked() {
                    if let Some(click_pos) = response.interact_pointer_pos() {
                        let threshold = 10.0f32;
                        let nearest = annotations.iter().enumerate().rev().find_map(|(i, ann)| {
                            let hit = match &ann.shape {
                                AnnotationShape::Circle { cx, cy, .. }
                                | AnnotationShape::Box { cx, cy, .. }
                                | AnnotationShape::Text { x: cx, y: cy, .. } => {
                                    if let Some((px, py)) =
                                        to_image_px(*cx, *cy, ann.use_wcs, wcs)
                                    {
                                        let s = img_to_screen(px, py, panel_rect, view);
                                        s.distance(click_pos) < threshold
                                    } else {
                                        false
                                    }
                                }
                                AnnotationShape::Line { x1, y1, .. } => {
                                    if let Some((px, py)) =
                                        to_image_px(*x1, *y1, ann.use_wcs, wcs)
                                    {
                                        let s = img_to_screen(px, py, panel_rect, view);
                                        s.distance(click_pos) < threshold
                                    } else {
                                        false
                                    }
                                }
                            };
                            if hit { Some(i) } else { None }
                        });
                        if let Some(i) = nearest {
                            self.mode = AnnotationMode::Selected(i);
                        }
                    }
                }
            }
            AnnotationMode::Selected(i) => {
                let i = *i;
                // Show a small delete button near the selected annotation
                if let Some(ann) = annotations.get(i) {
                    let screen_pos = match &ann.shape {
                        AnnotationShape::Circle { cx, cy, .. }
                        | AnnotationShape::Box { cx, cy, .. }
                        | AnnotationShape::Text { x: cx, y: cy, .. } => {
                            to_image_px(*cx, *cy, ann.use_wcs, wcs)
                                .map(|(px, py)| img_to_screen(px, py, panel_rect, view))
                        }
                        AnnotationShape::Line { x1, y1, .. } => {
                            to_image_px(*x1, *y1, ann.use_wcs, wcs)
                                .map(|(px, py)| img_to_screen(px, py, panel_rect, view))
                        }
                    };

                    if let Some(sp) = screen_pos {
                        // Highlight selected
                        painter.circle_stroke(
                            sp,
                            6.0,
                            egui::Stroke::new(1.5, egui::Color32::YELLOW),
                        );
                    }
                }
                // Click away to deselect
                if response.clicked() {
                    self.mode = AnnotationMode::Disabled;
                }
                // Delete key to remove
                if response.ctx.input(|inp| inp.key_pressed(egui::Key::Delete))
                    && i < annotations.len()
                {
                    annotations.remove(i);
                    self.mode = AnnotationMode::Disabled;
                    changed = true;
                }
            }
        }

        changed
    }
}

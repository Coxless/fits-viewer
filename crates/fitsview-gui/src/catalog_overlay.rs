use egui::{Color32, Painter, Pos2, Rect, Stroke};
use fitsview_core::{catalog::CatalogSource, wcs::Wcs};

use crate::viewport::ViewState;

/// A named set of catalog sources to display as an overlay.
pub struct CatalogOverlay {
    pub sources: Vec<CatalogSource>,
    pub name: String,
    pub symbol_color: Color32,
    pub symbol_radius: f32,
    pub visible: bool,
}

impl CatalogOverlay {
    pub fn new(sources: Vec<CatalogSource>, name: String) -> Self {
        Self {
            sources,
            name,
            symbol_color: Color32::from_rgb(255, 255, 0),
            symbol_radius: 6.0,
            visible: true,
        }
    }
}

/// Draw catalog source symbols and handle hover tooltips.
///
/// Returns the index of the hovered source (if any).
pub fn draw_catalog(
    painter: &Painter,
    overlay: &CatalogOverlay,
    wcs: &Wcs,
    panel_rect: Rect,
    view: &ViewState,
    ctx: &egui::Context,
) -> Option<usize> {
    if !overlay.visible {
        return None;
    }

    let stroke = Stroke::new(1.5, overlay.symbol_color);
    let cursor = ctx.input(|i| i.pointer.hover_pos());

    let mut hovered_idx = None;

    for (i, src) in overlay.sources.iter().enumerate() {
        let Some((px, py)) = wcs.world_to_pixel(src.ra, src.dec) else { continue };
        let screen = img_to_screen(px as f32, py as f32, panel_rect, view);

        if !in_panel(screen, panel_rect) {
            continue;
        }

        let r = overlay.symbol_radius;
        // Draw cross-hair marker
        painter.circle_stroke(screen, r, stroke);
        painter.line_segment([screen - egui::vec2(r * 1.5, 0.0), screen - egui::vec2(r, 0.0)], stroke);
        painter.line_segment([screen + egui::vec2(r, 0.0), screen + egui::vec2(r * 1.5, 0.0)], stroke);
        painter.line_segment([screen - egui::vec2(0.0, r * 1.5), screen - egui::vec2(0.0, r)], stroke);
        painter.line_segment([screen + egui::vec2(0.0, r), screen + egui::vec2(0.0, r * 1.5)], stroke);

        // Hover detection
        if let Some(cursor_pos) = cursor {
            if (cursor_pos - screen).length() < r + 4.0 {
                hovered_idx = Some(i);
                // Tooltip
                let name = src.name.clone().unwrap_or_default();
                let otype = src.object_type.clone().unwrap_or_default();
                let mag = src.magnitude;
                let ra = src.ra;
                let dec = src.dec;
                egui::show_tooltip(ctx, egui::layers::LayerId::debug(), egui::Id::new("cat_tooltip"), |ui| {
                    if !name.is_empty() { ui.label(&name); }
                    if !otype.is_empty() { ui.label(format!("Type: {otype}")); }
                    if let Some(m) = mag { ui.label(format!("Mag: {m:.2}")); }
                    ui.label(format!("RA: {ra:.6}°  Dec: {dec:.6}°"));
                });
            }
        }
    }

    hovered_idx
}

fn img_to_screen(ix: f32, iy: f32, panel_rect: Rect, view: &ViewState) -> Pos2 {
    Pos2::new(
        panel_rect.min.x + (ix + view.offset.x) * view.zoom,
        panel_rect.min.y + (iy + view.offset.y) * view.zoom,
    )
}

fn in_panel(p: Pos2, rect: Rect) -> bool {
    p.x >= rect.min.x - 20.0
        && p.x <= rect.max.x + 20.0
        && p.y >= rect.min.y - 20.0
        && p.y <= rect.max.y + 20.0
}

/// Panel for catalog query controls.
#[derive(Default)]
pub struct CatalogPanel {
    pub visible: bool,
    pub search_radius_arcmin: f64,
    pub max_results: usize,
    pub simbad_input: Option<String>,
    pub vizier_catalog: String,
    pub vizier_input: Option<String>,
    pub gaia_input: Option<String>,
    pub votable_input: Option<String>,
}

impl CatalogPanel {
    pub fn new() -> Self {
        Self {
            search_radius_arcmin: 5.0,
            max_results: 100,
            vizier_catalog: "II/246".to_owned(),
            ..Default::default()
        }
    }
}

pub enum CatalogQuery {
    Simbad { ra: f64, dec: f64, radius_arcmin: f64, max_results: usize },
    Vizier { catalog_id: String, ra: f64, dec: f64, radius_arcmin: f64 },
    GaiaDr3 { ra: f64, dec: f64, radius_arcmin: f64, max_results: usize },
    LocalVotable(std::path::PathBuf),
}

impl CatalogPanel {
    /// Show the catalog query panel.
    ///
    /// `image_center_wcs` is the (RA, Dec) of the current image center (if WCS available).
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        image_center_wcs: Option<(f64, f64)>,
    ) -> Option<CatalogQuery> {
        if !self.visible {
            return None;
        }

        let mut query = None;

        egui::Window::new("Catalog Overlay")
            .resizable(false)
            .show(ctx, |ui| {
                if image_center_wcs.is_none() {
                    ui.colored_label(egui::Color32::YELLOW, "WCS required for catalog queries.");
                    return;
                }
                let (ra, dec) = image_center_wcs.unwrap();

                ui.horizontal(|ui| {
                    ui.label("Search radius (arcmin):");
                    ui.add(egui::DragValue::new(&mut self.search_radius_arcmin).range(0.1..=60.0).speed(0.1));
                });
                ui.horizontal(|ui| {
                    ui.label("Max results:");
                    ui.add(egui::DragValue::new(&mut self.max_results).range(1..=10000));
                });

                ui.separator();

                // SIMBAD
                ui.heading("SIMBAD");
                if ui.button("Query SIMBAD").clicked() {
                    query = Some(CatalogQuery::Simbad {
                        ra, dec,
                        radius_arcmin: self.search_radius_arcmin,
                        max_results: self.max_results,
                    });
                }

                ui.separator();

                // VizieR
                ui.heading("VizieR");
                ui.horizontal(|ui| {
                    ui.label("Catalog ID:");
                    ui.text_edit_singleline(&mut self.vizier_catalog);
                });
                if ui.button("Query VizieR").clicked() {
                    query = Some(CatalogQuery::Vizier {
                        catalog_id: self.vizier_catalog.clone(),
                        ra, dec,
                        radius_arcmin: self.search_radius_arcmin,
                    });
                }

                ui.separator();

                // Gaia DR3
                ui.heading("Gaia DR3");
                if ui.button("Query Gaia DR3").clicked() {
                    query = Some(CatalogQuery::GaiaDr3 {
                        ra, dec,
                        radius_arcmin: self.search_radius_arcmin,
                        max_results: self.max_results,
                    });
                }

                ui.separator();

                // Local VOTable
                ui.heading("Local VOTable");
                let mut clear_votable = false;
                let mut load_votable_path: Option<std::path::PathBuf> = None;
                if let Some(ref mut path) = self.votable_input {
                    ui.text_edit_singleline(path);
                    ui.horizontal(|ui| {
                        if ui.button("Load").clicked() {
                            load_votable_path = Some(std::path::PathBuf::from(path.trim()));
                            clear_votable = true;
                        }
                        if ui.button("Cancel").clicked() {
                            clear_votable = true;
                        }
                    });
                } else if ui.button("Load VOTable file...").clicked() {
                    self.votable_input = Some(String::new());
                }
                if let Some(p) = load_votable_path {
                    query = Some(CatalogQuery::LocalVotable(p));
                }
                if clear_votable { self.votable_input = None; }
            });

        query
    }
}

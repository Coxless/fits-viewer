use egui::Pos2;
use fitsview_core::colormap::Colormap;
use fitsview_core::scale::ScaleMode;

use crate::tab_manager::{FileData, Tab};

pub struct StatusBar;

impl StatusBar {
    /// Render the bottom status bar.
    ///
    /// `tile_status`: `Some((pending, cache_mb))` for large-image tabs.
    pub fn show(
        ctx: &egui::Context,
        active_tab: Option<&Tab>,
        cursor_pos: Option<Pos2>,
        tile_status: Option<(usize, usize)>,
    ) {
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(24.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;

                    if let Some(tab) = active_tab {
                        // Left: filename + HDU
                        let fname = tab.title();
                        ui.label(egui::RichText::new(&fname).monospace());

                        ui.separator();

                        // Event mode indicator
                        if let FileData::Event(e) = &tab.data {
                            ui.label(
                                egui::RichText::new(format!(
                                    "EVENTS | {}/{} | {} events | bin={:.1}",
                                    e.x_col, e.y_col, e.total_events, e.bin_size
                                ))
                                .monospace()
                                .color(egui::Color32::from_rgb(100, 200, 255)),
                            );
                            ui.separator();
                        }

                        // Tile loading progress for large images
                        if let Some((pending, cache_mb)) = tile_status {
                            if pending > 0 {
                                ui.label(
                                    egui::RichText::new(format!("Loading {pending} tiles…"))
                                        .monospace()
                                        .color(egui::Color32::from_rgb(255, 200, 80)),
                                );
                                ui.separator();
                            }
                            ui.label(
                                egui::RichText::new(format!("Cache: {cache_mb} MB"))
                                    .monospace()
                                    .weak(),
                            );
                            ui.separator();
                        }

                        // Center: cursor pixel value (small images only)
                        if !tab.data.is_large() {
                            if let Some(pos) = cursor_pos {
                                let px = pos.x as usize;
                                let py = pos.y as usize;
                                let w = tab.data.width();
                                let h = tab.data.height();
                                let data = tab.data.pixel_data();
                                if px < w && py < h && !data.is_empty() {
                                    let val = data[py * w + px];
                                    ui.label(
                                        egui::RichText::new(format!("({px}, {py}) = {val:.4}"))
                                            .monospace(),
                                    );
                                    ui.separator();
                                }
                            }
                        }

                        // Right: scale + colormap + zoom
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(format!("{:.2}x", tab.view.zoom))
                                    .monospace(),
                            );
                            ui.separator();
                            ui.label(egui::RichText::new(colormap_name(tab.colormap)).monospace());
                            ui.separator();
                            ui.label(egui::RichText::new(scale_name(tab.scale_mode)).monospace());
                        });
                    } else {
                        ui.label(egui::RichText::new("No file open").monospace().weak());
                    }
                });
            });
    }
}

fn scale_name(m: ScaleMode) -> &'static str {
    match m {
        ScaleMode::ZScale => "ZScale",
        ScaleMode::Linear => "Linear",
        ScaleMode::Log => "Log",
        ScaleMode::Sqrt => "Sqrt",
        ScaleMode::Asinh => "ASinh",
        ScaleMode::MinMax => "MinMax",
    }
}

fn colormap_name(c: Colormap) -> &'static str {
    match c {
        Colormap::Gray => "Gray",
        Colormap::Viridis => "Viridis",
        Colormap::Plasma => "Plasma",
        Colormap::Inferno => "Inferno",
        Colormap::Hot => "Hot",
        Colormap::Rainbow => "Rainbow",
    }
}

use egui::Pos2;
use fitsview_core::colormap::Colormap;
use fitsview_core::scale::ScaleMode;
use fitsview_core::wcs::Wcs;

use crate::tab_manager::{FileData, Tab};
use crate::theme;

pub struct StatusBar;

impl StatusBar {
    /// Render the bottom status bar.
    ///
    /// `tile_status`: `Some((pending, cache_mb))` for large-image tabs.
    /// `wcs_pos`: precomputed (ra_deg, dec_deg) for cursor position.
    /// `blink`: blink state `Some(interval_sec)` when blink is active.
    pub fn show(
        ctx: &egui::Context,
        active_tab: Option<&Tab>,
        cursor_pos: Option<Pos2>,
        tile_status: Option<(usize, usize)>,
        blink_interval: Option<f32>,
    ) {
        egui::TopBottomPanel::bottom("status_bar")
            .frame(theme::status_bar_frame())
            .exact_height(26.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;

                    if let Some(tab) = active_tab {
                        let dot = egui::RichText::new("  ·  ").size(12.0).color(theme::SEPARATOR);

                        // Left: filename
                        let fname = tab.title();
                        ui.label(egui::RichText::new(&fname).monospace().size(12.5).color(theme::TEXT_PRIMARY));

                        // HDU indicator
                        if tab.hdu_list.len() > 1 {
                            ui.label(dot.clone());
                            ui.label(
                                egui::RichText::new(format!("HDU {}/{}", tab.hdu_index, tab.hdu_list.len() - 1))
                                    .monospace()
                                    .size(12.5)
                                    .color(theme::HDU_COLOR),
                            );
                        }

                        // Event mode indicator
                        if let FileData::Event(e) = &tab.data {
                            ui.label(dot.clone());
                            ui.label(
                                egui::RichText::new(format!(
                                    "EVENTS  {}/{}  {} events  bin={:.1}",
                                    e.x_col, e.y_col, e.total_events, e.bin_size
                                ))
                                .monospace()
                                .size(12.5)
                                .color(theme::EVENTS_COLOR),
                            );
                        }

                        // Tile loading progress for large images
                        if let Some((pending, cache_mb)) = tile_status {
                            if pending > 0 {
                                ui.label(dot.clone());
                                ui.label(
                                    egui::RichText::new(format!("Loading {pending} tiles…"))
                                        .monospace()
                                        .size(12.5)
                                        .color(theme::LOADING_COLOR),
                                );
                            }
                            ui.label(dot.clone());
                            ui.label(
                                egui::RichText::new(format!("Cache {cache_mb} MB"))
                                    .monospace()
                                    .size(12.5)
                                    .color(theme::TEXT_MUTED),
                            );
                        }

                        // Blink indicator
                        if let Some(interval) = blink_interval {
                            ui.label(dot.clone());
                            ui.label(
                                egui::RichText::new(format!("BLINK {interval:.1}s"))
                                    .monospace()
                                    .size(12.5)
                                    .color(theme::BLINK_COLOR),
                            );
                        }

                        // Cursor position: pixel value + optional WCS coords
                        if !tab.data.is_large() {
                            if let Some(pos) = cursor_pos {
                                let px = pos.x as usize;
                                let py = pos.y as usize;
                                let w = tab.data.width();
                                let h = tab.data.height();
                                let data = tab.data.pixel_data();
                                if px < w && py < h && !data.is_empty() {
                                    let val = data[py * w + px];
                                    ui.label(dot.clone());
                                    ui.label(
                                        egui::RichText::new(format!("({px}, {py}) = {val:.4}"))
                                            .monospace()
                                            .size(12.5)
                                            .color(theme::CURSOR_COLOR),
                                    );

                                    if let Some(wcs) = &tab.wcs {
                                        if let Some((ra, dec)) = wcs.pixel_to_world(pos.x as f64, pos.y as f64) {
                                            ui.label(dot.clone());
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "RA {}  Dec {}",
                                                    Wcs::format_ra(ra),
                                                    Wcs::format_dec(dec)
                                                ))
                                                .monospace()
                                                .size(12.5)
                                                .color(theme::CURSOR_COLOR),
                                            );
                                        }
                                    }
                                }
                            }
                        }

                        // Right: scale + colormap + zoom
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(format!("{:.2}×", tab.view.zoom))
                                    .monospace()
                                    .size(12.5)
                                    .color(theme::TEXT_OVERLAY),
                            );
                            ui.label(dot.clone());
                            ui.label(
                                egui::RichText::new(colormap_name(tab.colormap))
                                    .monospace()
                                    .size(12.5)
                                    .color(theme::TEXT_OVERLAY),
                            );
                            ui.label(dot.clone());
                            ui.label(
                                egui::RichText::new(scale_name(tab.scale_mode))
                                    .monospace()
                                    .size(12.5)
                                    .color(theme::TEXT_OVERLAY),
                            );

                            if (tab.contrast - 1.0).abs() > 0.01 || (tab.bias - 0.5).abs() > 0.01 {
                                ui.label(dot.clone());
                                ui.label(
                                    egui::RichText::new(format!(
                                        "C:{:.2}  B:{:.2}",
                                        tab.contrast, tab.bias
                                    ))
                                    .monospace()
                                    .size(12.5)
                                    .color(theme::SCALE_COLOR),
                                );
                            }
                        });
                    } else {
                        ui.label(
                            egui::RichText::new("No file open")
                                .monospace()
                                .size(12.5)
                                .color(theme::TEXT_MUTED),
                        );
                    }
                });
            });
    }
}

fn scale_name(m: ScaleMode) -> &'static str {
    match m {
        ScaleMode::ZScale  => "ZScale",
        ScaleMode::Linear  => "Linear",
        ScaleMode::Log     => "Log",
        ScaleMode::Sqrt    => "Sqrt",
        ScaleMode::Asinh   => "ASinh",
        ScaleMode::MinMax  => "MinMax",
        ScaleMode::HistEq  => "HistEq",
    }
}

fn colormap_name(c: Colormap) -> &'static str {
    match c {
        Colormap::Gray    => "Gray",
        Colormap::Viridis => "Viridis",
        Colormap::Plasma  => "Plasma",
        Colormap::Inferno => "Inferno",
        Colormap::Hot     => "Hot",
        Colormap::Rainbow => "Rainbow",
    }
}

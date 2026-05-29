use fitsview_core::scale::{compute_scale, ScaleMode};

use crate::{tab_manager::Tab, theme};

const N_BINS: usize = 256;

#[derive(Clone, Copy, PartialEq)]
enum DragTarget {
    VMin,
    VMax,
}

#[derive(Default)]
pub struct HistogramPanel {
    pub visible: bool,
    dragging: Option<DragTarget>,
}

/// Compute histogram bins from pixel data.
pub fn compute_histogram(data: &[f32], n_bins: usize) -> (Vec<u32>, Vec<f32>) {
    let finite: Vec<f32> = data.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        return (vec![0; n_bins], vec![0.0; n_bins + 1]);
    }

    let mut vmin = finite[0];
    let mut vmax = finite[0];
    for &v in &finite {
        if v < vmin { vmin = v; }
        if v > vmax { vmax = v; }
    }
    if (vmax - vmin).abs() < f32::EPSILON {
        vmax = vmin + 1.0;
    }

    let mut bins = vec![0u32; n_bins];
    let range = vmax - vmin;
    for &v in &finite {
        let idx = ((v - vmin) / range * n_bins as f32) as usize;
        let idx = idx.min(n_bins - 1);
        bins[idx] += 1;
    }

    let mut edges = Vec::with_capacity(n_bins + 1);
    for i in 0..=n_bins {
        edges.push(vmin + range * i as f32 / n_bins as f32);
    }

    (bins, edges)
}

/// Compute a percentile cut: returns (vmin, vmax) at lo_pct% and hi_pct%.
fn percentile_cut(data: &[f32], lo_pct: f32, hi_pct: f32) -> (f32, f32) {
    let mut finite: Vec<f32> = data.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        return (0.0, 1.0);
    }
    finite.sort_unstable_by(|a, b| a.total_cmp(b));
    let n = finite.len();
    let lo_idx = ((lo_pct / 100.0 * n as f32) as usize).min(n - 1);
    let hi_idx = ((hi_pct / 100.0 * n as f32) as usize).min(n - 1);
    (finite[lo_idx], finite[hi_idx])
}

impl HistogramPanel {
    /// Show the histogram panel.
    ///
    /// Returns `Some((vmin, vmax))` if the user changed the display limits.
    pub fn show(&mut self, ctx: &egui::Context, tab: Option<&Tab>) -> Option<(f32, f32)> {
        if !self.visible {
            return None;
        }

        let mut result: Option<(f32, f32)> = None;

        egui::Window::new("Histogram")
            .resizable(true)
            .collapsible(false)
            .default_size(egui::vec2(340.0, 220.0))
            .show(ctx, |ui| {
                let Some(tab) = tab else {
                    ui.label("No file open.");
                    return;
                };

                let data = tab.data.pixel_data();
                if data.is_empty() {
                    ui.weak("Loading…");
                    return;
                }

                // Current vmin/vmax in use
                let (cur_vmin, cur_vmax) = match &tab.scale_result {
                    Some(sr) => (
                        tab.vmin_override.unwrap_or(sr.vmin),
                        tab.vmax_override.unwrap_or(sr.vmax),
                    ),
                    None => (0.0f32, 1.0f32),
                };

                // Use cached histogram or compute inline (small files only; large
                // files trigger background computation via app.rs)
                let (bins, edges) = if let (Some(b), Some(e)) = (&tab.hist_bins, &tab.hist_edges) {
                    (b.as_ref().clone(), e.as_ref().clone())
                } else if !tab.hist_computing && !data.is_empty() {
                    compute_histogram(data, N_BINS)
                } else {
                    ui.add(egui::Spinner::new());
                    return;
                };

                if bins.is_empty() || edges.len() < 2 {
                    ui.weak("No data.");
                    return;
                }

                let data_min = edges[0];
                let data_max = *edges.last().unwrap();
                let data_range = data_max - data_min;

                // Preset buttons row
                ui.horizontal(|ui| {
                    ui.label("Cuts:");
                    if ui.small_button("ZScale").clicked() {
                        let sr = compute_scale(data, ScaleMode::ZScale);
                        result = Some((sr.vmin, sr.vmax));
                    }
                    if ui.small_button("MinMax").clicked() {
                        let sr = compute_scale(data, ScaleMode::MinMax);
                        result = Some((sr.vmin, sr.vmax));
                    }
                    if ui.small_button("99%").clicked() {
                        result = Some(percentile_cut(data, 0.5, 99.5));
                    }
                    if ui.small_button("99.9%").clicked() {
                        result = Some(percentile_cut(data, 0.05, 99.95));
                    }
                    if ui.small_button("Reset").clicked() {
                        result = Some((f32::NAN, f32::NAN)); // sentinel: clear overrides
                    }
                });

                ui.add_space(4.0);

                // Histogram drawing area
                let desired = egui::vec2(ui.available_width(), 130.0);
                let (hist_rect, hist_resp) =
                    ui.allocate_exact_size(desired, egui::Sense::drag());
                let painter = ui.painter_at(hist_rect);

                // Background
                painter.rect_filled(hist_rect, 4.0, theme::BG_PANEL);

                // Draw bars (log scale on Y axis)
                let max_count = *bins.iter().max().unwrap_or(&1) as f32;
                let log_max = (max_count + 1.0).ln();

                for (i, &count) in bins.iter().enumerate() {
                    let x0 = hist_rect.min.x + i as f32 / N_BINS as f32 * hist_rect.width();
                    let x1 = hist_rect.min.x + (i + 1) as f32 / N_BINS as f32 * hist_rect.width();
                    let log_h = (count as f32 + 1.0).ln() / log_max;
                    let y0 = hist_rect.max.y - log_h * hist_rect.height();
                    let bar_rect =
                        egui::Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, hist_rect.max.y));
                    painter.rect_filled(bar_rect, 0.0, egui::Color32::from_rgb(80, 120, 200));
                }

                // Helper: value → x position in hist_rect
                let val_to_x = |v: f32| -> f32 {
                    if data_range.abs() < f32::EPSILON {
                        return hist_rect.min.x;
                    }
                    hist_rect.min.x + (v - data_min) / data_range * hist_rect.width()
                };

                // Draw vmin / vmax marker lines
                let vmin_x = val_to_x(cur_vmin);
                let vmax_x = val_to_x(cur_vmax);

                painter.line_segment(
                    [egui::pos2(vmin_x, hist_rect.min.y), egui::pos2(vmin_x, hist_rect.max.y)],
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(80, 200, 80)),
                );
                painter.line_segment(
                    [egui::pos2(vmax_x, hist_rect.min.y), egui::pos2(vmax_x, hist_rect.max.y)],
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(200, 80, 80)),
                );

                // Handle drag to move vmin/vmax
                let x_to_val = |x: f32| -> f32 {
                    data_min + (x - hist_rect.min.x) / hist_rect.width() * data_range
                };

                if hist_resp.drag_started() {
                    if let Some(pos) = hist_resp.interact_pointer_pos() {
                        let dist_min = (pos.x - vmin_x).abs();
                        let dist_max = (pos.x - vmax_x).abs();
                        self.dragging =
                            Some(if dist_min < dist_max { DragTarget::VMin } else { DragTarget::VMax });
                    }
                }
                if hist_resp.dragged() {
                    if let (Some(target), Some(pos)) =
                        (self.dragging, hist_resp.interact_pointer_pos())
                    {
                        let new_val = x_to_val(pos.x).clamp(data_min, data_max);
                        match target {
                            DragTarget::VMin => {
                                let new_vmax = cur_vmax.max(new_val + f32::EPSILON);
                                result = Some((new_val, new_vmax));
                            }
                            DragTarget::VMax => {
                                let new_vmin = cur_vmin.min(new_val - f32::EPSILON);
                                result = Some((new_vmin, new_val));
                            }
                        }
                    }
                }
                if hist_resp.drag_stopped() {
                    self.dragging = None;
                }

                ui.add_space(4.0);

                // Numeric readout / input row
                let mut v0 = cur_vmin;
                let mut v1 = cur_vmax;
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("vmin:").color(egui::Color32::from_rgb(80, 200, 80)).size(12.0));
                    if ui
                        .add(egui::DragValue::new(&mut v0).speed(data_range / 500.0).max_decimals(3))
                        .changed()
                    {
                        result = Some((v0, cur_vmax.max(v0 + f32::EPSILON)));
                    }
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("vmax:").color(egui::Color32::from_rgb(200, 80, 80)).size(12.0));
                    if ui
                        .add(egui::DragValue::new(&mut v1).speed(data_range / 500.0).max_decimals(3))
                        .changed()
                    {
                        result = Some((cur_vmin.min(v1 - f32::EPSILON), v1));
                    }
                });
            });

        result
    }
}

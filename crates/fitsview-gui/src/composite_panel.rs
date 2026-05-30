use fitsview_core::{composite::RgbCompositeData, scale::ScaleMode};

use crate::tab_manager::Tab;

/// State for the RGB composite dialog.
pub struct CompositePanel {
    pub visible: bool,
    pub r_tab_idx: usize,
    pub g_tab_idx: usize,
    pub b_tab_idx: usize,
    pub r_vmin: f32,
    pub r_vmax: f32,
    pub g_vmin: f32,
    pub g_vmax: f32,
    pub b_vmin: f32,
    pub b_vmax: f32,
    pub r_scale: ScaleMode,
    pub g_scale: ScaleMode,
    pub b_scale: ScaleMode,
}

impl Default for CompositePanel {
    fn default() -> Self {
        Self {
            visible: false,
            r_tab_idx: 0, g_tab_idx: 0, b_tab_idx: 0,
            r_vmin: 0.0, r_vmax: 0.0,
            g_vmin: 0.0, g_vmax: 0.0,
            b_vmin: 0.0, b_vmax: 0.0,
            r_scale: ScaleMode::ZScale,
            g_scale: ScaleMode::ZScale,
            b_scale: ScaleMode::ZScale,
        }
    }
}

impl CompositePanel {
    /// Show the RGB composite dialog.
    ///
    /// Returns `Some(composite)` when the user clicks "Render".
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        tabs: &[Tab],
    ) -> Option<RgbCompositeData> {
        if !self.visible {
            return None;
        }

        let tab_titles: Vec<String> = tabs.iter().map(|t| t.title()).collect();
        if tab_titles.is_empty() {
            return None;
        }

        let r_idx = self.r_tab_idx.min(tabs.len().saturating_sub(1));
        let g_idx = self.g_tab_idx.min(tabs.len().saturating_sub(1));
        let b_idx = self.b_tab_idx.min(tabs.len().saturating_sub(1));

        // Auto-initialize vmin/vmax from scale_result on first open
        if self.r_vmax == 0.0 && self.r_vmin == 0.0 {
            if let Some(sr) = tabs.get(r_idx).and_then(|t| t.scale_result) {
                self.r_vmin = sr.vmin; self.r_vmax = sr.vmax;
            }
        }
        if self.g_vmax == 0.0 && self.g_vmin == 0.0 {
            if let Some(sr) = tabs.get(g_idx).and_then(|t| t.scale_result) {
                self.g_vmin = sr.vmin; self.g_vmax = sr.vmax;
            }
        }
        if self.b_vmax == 0.0 && self.b_vmin == 0.0 {
            if let Some(sr) = tabs.get(b_idx).and_then(|t| t.scale_result) {
                self.b_vmin = sr.vmin; self.b_vmax = sr.vmax;
            }
        }

        let mut result = None;
        let mut close = false;

        egui::Window::new("RGB Composite")
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                egui::Grid::new("rgb_grid")
                    .num_columns(2)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        // R channel
                        ui.colored_label(egui::Color32::from_rgb(255, 80, 80), "R:");
                        egui::ComboBox::from_id_salt("r_tab")
                            .selected_text(tab_titles.get(self.r_tab_idx).cloned().unwrap_or_default())
                            .show_ui(ui, |ui| {
                                for (i, t) in tab_titles.iter().enumerate() {
                                    ui.selectable_value(&mut self.r_tab_idx, i, t);
                                }
                            });
                        ui.end_row();
                        ui.label("  vmin:"); ui.add(egui::DragValue::new(&mut self.r_vmin).speed(1.0)); ui.end_row();
                        ui.label("  vmax:"); ui.add(egui::DragValue::new(&mut self.r_vmax).speed(1.0)); ui.end_row();
                        ui.label("  scale:"); scale_combo(ui, "r_scale", &mut self.r_scale); ui.end_row();

                        ui.separator(); ui.separator(); ui.end_row();

                        // G channel
                        ui.colored_label(egui::Color32::from_rgb(80, 200, 80), "G:");
                        egui::ComboBox::from_id_salt("g_tab")
                            .selected_text(tab_titles.get(self.g_tab_idx).cloned().unwrap_or_default())
                            .show_ui(ui, |ui| {
                                for (i, t) in tab_titles.iter().enumerate() {
                                    ui.selectable_value(&mut self.g_tab_idx, i, t);
                                }
                            });
                        ui.end_row();
                        ui.label("  vmin:"); ui.add(egui::DragValue::new(&mut self.g_vmin).speed(1.0)); ui.end_row();
                        ui.label("  vmax:"); ui.add(egui::DragValue::new(&mut self.g_vmax).speed(1.0)); ui.end_row();
                        ui.label("  scale:"); scale_combo(ui, "g_scale", &mut self.g_scale); ui.end_row();

                        ui.separator(); ui.separator(); ui.end_row();

                        // B channel
                        ui.colored_label(egui::Color32::from_rgb(80, 130, 255), "B:");
                        egui::ComboBox::from_id_salt("b_tab")
                            .selected_text(tab_titles.get(self.b_tab_idx).cloned().unwrap_or_default())
                            .show_ui(ui, |ui| {
                                for (i, t) in tab_titles.iter().enumerate() {
                                    ui.selectable_value(&mut self.b_tab_idx, i, t);
                                }
                            });
                        ui.end_row();
                        ui.label("  vmin:"); ui.add(egui::DragValue::new(&mut self.b_vmin).speed(1.0)); ui.end_row();
                        ui.label("  vmax:"); ui.add(egui::DragValue::new(&mut self.b_vmax).speed(1.0)); ui.end_row();
                        ui.label("  scale:"); scale_combo(ui, "b_scale", &mut self.b_scale); ui.end_row();
                    });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Render").clicked() {
                        if let (Some(rt), Some(gt), Some(bt)) = (
                            tabs.get(self.r_tab_idx),
                            tabs.get(self.g_tab_idx),
                            tabs.get(self.b_tab_idx),
                        ) {
                            let rw = rt.data.width();
                            let rh = rt.data.height();
                            if rw > 0 && rh > 0 {
                                let w = rw.min(gt.data.width()).min(bt.data.width());
                                let h = rh.min(gt.data.height()).min(bt.data.height());
                                result = Some(RgbCompositeData::new(
                                    rt.data.pixel_data().to_vec(), self.r_vmin, self.r_vmax, self.r_scale,
                                    gt.data.pixel_data().to_vec(), self.g_vmin, self.g_vmax, self.g_scale,
                                    bt.data.pixel_data().to_vec(), self.b_vmin, self.b_vmax, self.b_scale,
                                    w, h,
                                ));
                                close = true;
                            }
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });

        if close {
            self.visible = false;
        }
        result
    }
}

fn scale_combo(ui: &mut egui::Ui, id: &str, mode: &mut ScaleMode) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(format!("{:?}", mode))
        .show_ui(ui, |ui| {
            for m in [ScaleMode::Linear, ScaleMode::ZScale, ScaleMode::Log, ScaleMode::Sqrt, ScaleMode::Asinh, ScaleMode::MinMax] {
                ui.selectable_value(mode, m, format!("{:?}", m));
            }
        });
}

use fitsview_core::arithmetic::ArithOp;

use crate::tab_manager::Tab;

/// Dialog for image arithmetic (A op B → new tab).
pub struct ArithmeticPanel {
    pub visible: bool,
    a_idx: usize,
    b_idx: usize,
    op: ArithOp,
    scale: f32,
}

impl Default for ArithmeticPanel {
    fn default() -> Self {
        Self { visible: false, a_idx: 0, b_idx: 1, op: ArithOp::Sub, scale: 1.0 }
    }
}

pub struct ArithRequest {
    pub a_idx: usize,
    pub b_idx: usize,
    pub op: ArithOp,
    pub scale: f32,
}

impl ArithmeticPanel {
    pub fn new() -> Self {
        Self { scale: 1.0, ..Default::default() }
    }

    /// Show the arithmetic dialog.
    ///
    /// Returns `Some(request)` when the user clicks "Apply".
    pub fn show(&mut self, ctx: &egui::Context, tabs: &[Tab]) -> Option<ArithRequest> {
        if !self.visible {
            return None;
        }

        let tab_titles: Vec<String> = tabs.iter().map(|t| t.title()).collect();
        if tab_titles.len() < 2 {
            return None;
        }

        let a_idx = self.a_idx.min(tabs.len().saturating_sub(1));
        let b_idx = self.b_idx.min(tabs.len().saturating_sub(1));

        let mut result = None;
        let mut close = false;

        egui::Window::new("Image Arithmetic")
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label("Compute: result = (A op B) × scale");
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.label("A:");
                    egui::ComboBox::from_id_salt("arith_a")
                        .selected_text(tab_titles.get(self.a_idx).cloned().unwrap_or_default())
                        .show_ui(ui, |ui| {
                            for (i, t) in tab_titles.iter().enumerate() {
                                ui.selectable_value(&mut self.a_idx, i, t);
                            }
                        });
                });

                ui.horizontal(|ui| {
                    ui.label("Op:");
                    for op in [ArithOp::Add, ArithOp::Sub, ArithOp::Mul, ArithOp::Div, ArithOp::RelDiff] {
                        ui.selectable_value(&mut self.op, op, op.to_string());
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("B:");
                    egui::ComboBox::from_id_salt("arith_b")
                        .selected_text(tab_titles.get(self.b_idx).cloned().unwrap_or_default())
                        .show_ui(ui, |ui| {
                            for (i, t) in tab_titles.iter().enumerate() {
                                ui.selectable_value(&mut self.b_idx, i, t);
                            }
                        });
                });

                ui.horizontal(|ui| {
                    ui.label("Scale:");
                    ui.add(egui::DragValue::new(&mut self.scale).speed(0.01).range(-1000.0..=1000.0));
                });

                let a_size = tabs.get(a_idx).map(|t| (t.data.width(), t.data.height())).unwrap_or((0, 0));
                let b_size = tabs.get(b_idx).map(|t| (t.data.width(), t.data.height())).unwrap_or((0, 0));

                if a_size != b_size && a_size != (0, 0) && b_size != (0, 0) {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        format!("⚠ Size mismatch: A={}×{} B={}×{}", a_size.0, a_size.1, b_size.0, b_size.1),
                    );
                }

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let can_apply = a_size == b_size && a_size != (0, 0);
                    if ui.add_enabled(can_apply, egui::Button::new("Apply")).clicked() {
                        result = Some(ArithRequest {
                            a_idx: self.a_idx,
                            b_idx: self.b_idx,
                            op: self.op,
                            scale: self.scale,
                        });
                        close = true;
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

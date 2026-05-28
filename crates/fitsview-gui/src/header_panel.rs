use std::collections::HashMap;

use fitsview_core::fits_reader::HduType;

use crate::tab_manager::Tab;

pub struct HeaderPanel {
    pub visible: bool,
    pub search: String,
}

impl Default for HeaderPanel {
    fn default() -> Self {
        Self { visible: true, search: String::new() }
    }
}

impl HeaderPanel {
    /// Render the header panel. Must be called before `CentralPanel`.
    /// Returns `Some(new_hdu_index)` if the user selected a different HDU.
    pub fn show(&mut self, ctx: &egui::Context, tab: &Tab) -> Option<usize> {
        if !self.visible {
            return None;
        }
        let mut selected_hdu = None;
        egui::SidePanel::right("header_panel")
            .default_width(280.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading("FITS Header");
                ui.separator();

                // HDU selector (only when multiple HDUs exist)
                if tab.hdu_list.len() > 1 {
                    ui.label(egui::RichText::new("HDU:").size(13.0));
                    let current_label = if tab.hdu_index < tab.hdu_list.len() {
                        let h = &tab.hdu_list[tab.hdu_index];
                        if h.name.is_empty() {
                            format!("[{}] {:?}", tab.hdu_index, h.hdu_type)
                        } else {
                            format!("[{}] {}", tab.hdu_index, h.name)
                        }
                    } else {
                        format!("[{}]", tab.hdu_index)
                    };

                    egui::ComboBox::from_id_salt("hdu_select")
                        .selected_text(&current_label)
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for (i, hdu) in tab.hdu_list.iter().enumerate() {
                                let lbl = if hdu.name.is_empty() {
                                    format!("[{i}] {:?} {:?}", hdu.hdu_type, hdu.naxis)
                                } else {
                                    format!("[{i}] {} {:?}", hdu.name, hdu.naxis)
                                };
                                let can_view = matches!(hdu.hdu_type, HduType::Image);
                                if ui.add_enabled(can_view, egui::SelectableLabel::new(tab.hdu_index == i, lbl)).clicked()
                                    && tab.hdu_index != i
                                {
                                    selected_hdu = Some(i);
                                }
                            }
                        });
                    ui.separator();
                }

                // Contrast/bias sliders
                ui.label(egui::RichText::new("Display:").size(13.0));

                // We need mutable access but only have a reference here — display as read-only
                // The actual mutation happens in app.rs via the returned values
                ui.horizontal(|ui| {
                    ui.label("Contrast");
                    ui.label(egui::RichText::new(format!("{:.2}", tab.contrast)).monospace());
                });
                ui.horizontal(|ui| {
                    ui.label("Bias");
                    ui.label(egui::RichText::new(format!("{:.2}", tab.bias)).monospace());
                });
                ui.add_space(2.0);

                ui.label(egui::RichText::new("Search:").size(14.0));
                ui.add(egui::TextEdit::singleline(&mut self.search)
                    .font(egui::FontId::proportional(14.0)));
                ui.separator();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    let header: &HashMap<String, String> = tab.data.header();
                    let search_lower = self.search.to_ascii_lowercase();
                    let mut keys: Vec<&String> = header.keys().collect();
                    keys.sort_unstable();
                    for key in keys {
                        let val = &header[key];
                        if search_lower.is_empty()
                            || key.to_ascii_lowercase().contains(&search_lower)
                            || val.to_ascii_lowercase().contains(&search_lower)
                        {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(key).monospace().size(14.0).strong());
                                ui.label(egui::RichText::new(val).monospace().size(14.0));
                            });
                        }
                    }
                });
            });
        selected_hdu
    }
}

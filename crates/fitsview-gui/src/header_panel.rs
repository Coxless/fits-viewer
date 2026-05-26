use std::collections::HashMap;

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
    pub fn show(&mut self, ctx: &egui::Context, header: &HashMap<String, String>) {
        if !self.visible {
            return;
        }
        egui::SidePanel::right("header_panel")
            .default_width(280.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading("FITS Header");
                ui.separator();
                ui.label("Search:");
                ui.text_edit_singleline(&mut self.search);
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
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
                                ui.label(egui::RichText::new(key).monospace().strong());
                                ui.label(egui::RichText::new(val).monospace());
                            });
                        }
                    }
                });
            });
    }
}

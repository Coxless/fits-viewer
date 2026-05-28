use fitsview_core::stats::ImageStats;

#[derive(Default)]
pub struct StatsPanel {
    pub visible: bool,
}

impl StatsPanel {
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        stats: Option<&ImageStats>,
        computing: bool,
        img_width: usize,
        img_height: usize,
    ) {
        if !self.visible {
            return;
        }
        let mut open = self.visible;
        egui::Window::new("Image Statistics")
            .open(&mut open)
            .resizable(true)
            .collapsible(false)
            .default_width(280.0)
            .show(ctx, |ui| {
                if computing {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new());
                        ui.label("Computing…");
                    });
                    return;
                }
                if let Some(s) = stats {
                    egui::Grid::new("stats_grid")
                        .num_columns(2)
                        .striped(true)
                        .show(ui, |ui| {
                            stat_row(ui, "Width",  &format!("{img_width} px"));
                            stat_row(ui, "Height", &format!("{img_height} px"));
                            stat_row(ui, "Total pixels",  &format!("{}", s.npix));
                            stat_row(ui, "Valid pixels",  &format!("{}", s.n_finite));
                            stat_row(ui, "Minimum",  &format!("{:.6}", s.min));
                            stat_row(ui, "Maximum",  &format!("{:.6}", s.max));
                            stat_row(ui, "Mean",     &format!("{:.6}", s.mean));
                            stat_row(ui, "Median",   &format!("{:.6}", s.median));
                            stat_row(ui, "Std Dev",  &format!("{:.6}", s.std_dev));
                            stat_row(ui, "Sum",      &format!("{:.6e}", s.sum));
                        });
                } else {
                    ui.label("No image loaded.");
                }
            });
        self.visible = open;
    }
}

fn stat_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(egui::RichText::new(label).monospace());
    ui.label(egui::RichText::new(value).monospace());
    ui.end_row();
}

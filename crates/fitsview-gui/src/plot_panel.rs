use egui_plot::{Line, Plot, PlotPoints};

pub struct PlotSeries {
    pub label: String,
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    pub unit_x: String,
    pub unit_y: String,
}

#[derive(Default)]
pub struct PlotPanel {
    pub visible: bool,
    pub series: Vec<PlotSeries>,
}

impl PlotPanel {
    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.visible || self.series.is_empty() {
            return;
        }

        egui::Window::new("Plot")
            .resizable(true)
            .default_size(egui::vec2(400.0, 240.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.small_button("Export CSV").clicked() {
                        let csv = self.export_csv();
                        ui.output_mut(|o| o.copied_text = csv);
                    }
                    if ui.small_button("Clear").clicked() {
                        self.series.clear();
                        self.visible = false;
                    }
                });

                ui.add_space(4.0);

                Plot::new("plot_panel")
                    .height(180.0)
                    .show(ui, |plot_ui| {
                        for s in &self.series {
                            let pts: PlotPoints = s.x.iter().zip(s.y.iter())
                                .map(|(&x, &y)| [x, y])
                                .collect();
                            plot_ui.line(Line::new(pts).name(&s.label));
                        }
                    });
            });
    }

    pub fn add_spectrum(
        &mut self,
        x: Vec<f64>,
        y: Vec<f64>,
        label: String,
        unit_x: String,
        unit_y: String,
    ) {
        self.series.push(PlotSeries { label, x, y, unit_x, unit_y });
        self.visible = true;
    }

    fn export_csv(&self) -> String {
        let mut out = String::new();
        for s in &self.series {
            out.push_str(&format!("# {}\n", s.label));
            out.push_str(&format!("{},{}\n", s.unit_x, s.unit_y));
            for (&x, &y) in s.x.iter().zip(s.y.iter()) {
                out.push_str(&format!("{x},{y}\n"));
            }
        }
        out
    }
}

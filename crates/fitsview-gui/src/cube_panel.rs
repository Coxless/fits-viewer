use std::time::Instant;

use fitsview_core::cube_reader::{CollapseMode, SpectralAxis};

pub struct CubePanel {
    pub visible: bool,
    pub z_index: usize,
    pub playing: bool,
    pub fps: f32,
    pub bounce: bool,
    pub collapse_mode: CollapseMode,
    last_frame: Instant,
    forward: bool,
}

impl CubePanel {
    pub fn new(depth: usize) -> Self {
        let _ = depth;
        Self {
            visible: true,
            z_index: 0,
            playing: false,
            fps: 10.0,
            bounce: false,
            collapse_mode: CollapseMode::Sum,
            last_frame: Instant::now(),
            forward: true,
        }
    }

    /// Show the cube control panel.
    ///
    /// Returns `Some(new_z)` if the slice index changed (slider drag or animation tick).
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        depth: usize,
        spectral_axis: Option<&SpectralAxis>,
    ) -> Option<usize> {
        if !self.visible || depth == 0 {
            return None;
        }

        let mut new_z: Option<usize> = None;

        // Animation tick
        if self.playing && depth > 1 {
            let elapsed = self.last_frame.elapsed().as_secs_f32();
            if elapsed >= 1.0 / self.fps.max(0.1) {
                self.last_frame = Instant::now();
                if self.bounce {
                    if self.forward {
                        if self.z_index + 1 >= depth {
                            self.forward = false;
                            self.z_index = self.z_index.saturating_sub(1);
                        } else {
                            self.z_index += 1;
                        }
                    } else if self.z_index == 0 {
                        self.forward = true;
                        self.z_index += 1;
                    } else {
                        self.z_index -= 1;
                    }
                } else {
                    self.z_index = (self.z_index + 1) % depth;
                }
                new_z = Some(self.z_index);
                ctx.request_repaint_after(std::time::Duration::from_secs_f32(1.0 / self.fps.max(0.1)));
            } else {
                let remaining = 1.0 / self.fps.max(0.1) - elapsed;
                ctx.request_repaint_after(std::time::Duration::from_secs_f32(remaining));
            }
        }

        egui::TopBottomPanel::bottom("cube_panel")
            .exact_height(58.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);

                // Spectral value display
                let spec_label = if let Some(ax) = spectral_axis {
                    let val = ax.value_at_z(self.z_index);
                    format!("z = {} / {}   {} = {:.4}", self.z_index, depth.saturating_sub(1), ax.label(), val)
                } else {
                    format!("z = {} / {}", self.z_index, depth.saturating_sub(1))
                };
                ui.label(egui::RichText::new(spec_label).monospace().size(12.0));

                ui.horizontal(|ui| {
                    // Prev / Next buttons
                    if ui.small_button("◀").clicked() && self.z_index > 0 {
                        self.z_index -= 1;
                        new_z = Some(self.z_index);
                    }

                    // Slider
                    let mut z = self.z_index;
                    let slider = egui::Slider::new(&mut z, 0..=(depth.saturating_sub(1)))
                        .show_value(false);
                    if ui.add(slider).changed() {
                        self.z_index = z;
                        new_z = Some(z);
                    }

                    if ui.small_button("▶").clicked() && self.z_index + 1 < depth {
                        self.z_index += 1;
                        new_z = Some(self.z_index);
                    }

                    ui.separator();

                    // Play / Stop
                    let play_label = if self.playing { "⏹ Stop" } else { "▶▶ Play" };
                    if ui.small_button(play_label).clicked() {
                        self.playing = !self.playing;
                        if self.playing {
                            self.last_frame = Instant::now();
                            ctx.request_repaint();
                        }
                    }

                    ui.label("FPS:");
                    ui.add(egui::DragValue::new(&mut self.fps).range(1.0..=60.0).speed(0.5));

                    ui.checkbox(&mut self.bounce, "Bounce");
                });
            });

        new_z
    }
}

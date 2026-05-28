use std::fs;
use std::path::{Path, PathBuf};

use crate::dir_watcher::{DirEvent, DirWatcher};
use crate::theme;

pub struct FileExplorer {
    pub visible: bool,
    pub root: Option<PathBuf>,
    entries: Vec<PathBuf>,
    watcher: Option<DirWatcher>,
}

impl Default for FileExplorer {
    fn default() -> Self {
        Self { visible: true, root: None, entries: Vec::new(), watcher: None }
    }
}

impl FileExplorer {
    /// Set the directory to display; starts filesystem watching.
    pub fn set_root(&mut self, path: PathBuf) {
        self.entries = scan_fits(&path);
        self.watcher = DirWatcher::new(&path)
            .map_err(|e| log::warn!("DirWatcher init failed: {e}"))
            .ok();
        self.root = Some(path);
    }

    /// Drain watcher events; call once per frame. Returns paths that should be opened.
    pub fn poll_events(&mut self) -> Vec<(FsChange, PathBuf)> {
        let Some(watcher) = &self.watcher else { return vec![] };
        let events = watcher.poll();
        let mut changes = Vec::new();
        for ev in events {
            match ev {
                DirEvent::Created(p) => {
                    if !self.entries.contains(&p) {
                        self.entries.push(p.clone());
                        self.entries.sort_unstable();
                        changes.push((FsChange::Added, p));
                    }
                }
                DirEvent::Removed(p) => {
                    self.entries.retain(|e| e != &p);
                    changes.push((FsChange::Removed, p));
                }
                DirEvent::Modified(p) => {
                    changes.push((FsChange::Modified, p));
                }
            }
        }
        changes
    }

    /// Render the left sidebar. Returns a path if the user clicked a file to open it.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<PathBuf> {
        if !self.visible {
            return None;
        }
        let mut to_open = None;
        egui::SidePanel::left("file_explorer")
            .default_width(220.0)
            .resizable(true)
            .frame(theme::side_panel_frame())
            .show(ctx, |ui| {
                // Panel header
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("EXPLORER")
                        .size(11.0)
                        .color(theme::TEXT_MUTED)
                        .strong(),
                );
                ui.add_space(2.0);

                if let Some(root) = &self.root {
                    let dir_name = root
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| root.to_string_lossy().into_owned());
                    ui.label(
                        egui::RichText::new(dir_name.to_uppercase())
                            .size(12.0)
                            .color(theme::TEXT_OVERLAY)
                            .strong(),
                    );
                    ui.add_space(4.0);
                }

                egui::ScrollArea::vertical().show(ui, |ui| {
                    for path in &self.entries {
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();

                        let is_fits = is_fits_ext(path);
                        let size_badge = if is_fits {
                            std::fs::metadata(path)
                                .ok()
                                .map(|m| {
                                    let b = m.len();
                                    if b >= 1 << 30 {
                                        format!("  {:.1}G", b as f64 / (1u64 << 30) as f64)
                                    } else if b >= 1 << 20 {
                                        format!("  {:.0}M", b as f64 / (1u64 << 20) as f64)
                                    } else {
                                        String::new()
                                    }
                                })
                                .unwrap_or_default()
                        } else {
                            String::new()
                        };

                        let row_height = 22.0;
                        let sense = if is_fits {
                            egui::Sense::click()
                        } else {
                            egui::Sense::hover()
                        };
                        let (row_rect, resp) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), row_height),
                            sense,
                        );

                        if ui.is_rect_visible(row_rect) {
                            if resp.hovered() && is_fits {
                                ui.painter().rect_filled(row_rect, 4.0, theme::BG_HOVER);
                            }

                            if is_fits {
                                // Accent left bar for FITS files
                                let bar_rect = egui::Rect::from_min_size(
                                    row_rect.left_top(),
                                    egui::vec2(2.0, row_height),
                                );
                                ui.painter().rect_filled(bar_rect, egui::Rounding::ZERO, theme::ACCENT_DIM);

                                let icon_color = if resp.hovered() { theme::ACCENT } else { theme::ACCENT_DIM };
                                ui.painter().text(
                                    egui::pos2(row_rect.left() + 10.0, row_rect.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    "◆",
                                    egui::FontId::proportional(9.0),
                                    icon_color,
                                );
                                let text_x = row_rect.left() + 22.0;
                                let text_color = if resp.hovered() { theme::TEXT_PRIMARY } else { theme::TEXT_OVERLAY };
                                ui.painter().text(
                                    egui::pos2(text_x, row_rect.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    &name,
                                    egui::FontId::monospace(13.0),
                                    text_color,
                                );
                                if !size_badge.is_empty() {
                                    let name_w = ctx.fonts(|f| {
                                        f.layout_no_wrap(
                                            name.clone(),
                                            egui::FontId::monospace(13.0),
                                            egui::Color32::WHITE,
                                        ).rect.width()
                                    });
                                    ui.painter().text(
                                        egui::pos2(text_x + name_w, row_rect.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        &size_badge,
                                        egui::FontId::monospace(11.0),
                                        theme::TEXT_MUTED,
                                    );
                                }
                            } else {
                                ui.painter().text(
                                    egui::pos2(row_rect.left() + 10.0, row_rect.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    &name,
                                    egui::FontId::monospace(13.0),
                                    theme::TEXT_MUTED,
                                );
                            }
                        }

                        if is_fits && resp.clicked() {
                            to_open = Some(path.clone());
                        }
                        resp.context_menu(|ui| {
                            if ui.button("Open").clicked() {
                                to_open = Some(path.clone());
                                ui.close_menu();
                            }
                        });
                    }
                });
            });
        to_open
    }
}

pub enum FsChange {
    Added,
    Removed,
    Modified,
}

fn scan_fits(dir: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    entries.sort_unstable();
    entries
}

fn is_fits_ext(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".fits") || lower.ends_with(".fit") || lower.ends_with(".fits.gz")
}

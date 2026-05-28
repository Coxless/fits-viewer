use std::fs;
use std::path::{Path, PathBuf};

use crate::dir_watcher::{DirEvent, DirWatcher};

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
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Explorer");
                });
                ui.separator();

                if let Some(root) = &self.root.clone() {
                    ui.label(
                        egui::RichText::new(
                            root.file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_else(|| root.to_string_lossy().into_owned()),
                        )
                        .small()
                        .weak(),
                    );
                    ui.separator();
                }

                egui::ScrollArea::vertical().show(ui, |ui| {
                    let entries = self.entries.clone();
                    for path in &entries {
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
                                        format!(" [{:.1}G]", b as f64 / (1u64 << 30) as f64)
                                    } else if b >= 1 << 20 {
                                        format!(" [{:.0}M]", b as f64 / (1u64 << 20) as f64)
                                    } else {
                                        String::new()
                                    }
                                })
                                .unwrap_or_default()
                        } else {
                            String::new()
                        };
                        let text = if is_fits {
                            egui::RichText::new(format!("📄 {name}{size_badge}")).monospace()
                        } else {
                            egui::RichText::new(format!("  {name}")).monospace().weak()
                        };

                        let resp = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
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

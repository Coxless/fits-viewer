use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::dir_watcher::{DirEvent, DirWatcher};
use crate::theme;

struct TreeNode {
    path: PathBuf,
    kind: NodeKind,
}

enum NodeKind {
    Dir(Vec<TreeNode>),
    File,
}

pub struct FileExplorer {
    pub visible: bool,
    pub root: Option<PathBuf>,
    tree: Vec<TreeNode>,
    expanded: HashSet<PathBuf>,
    watcher: Option<DirWatcher>,
}

impl Default for FileExplorer {
    fn default() -> Self {
        Self { visible: true, root: None, tree: Vec::new(), expanded: HashSet::new(), watcher: None }
    }
}

impl FileExplorer {
    pub fn set_root(&mut self, path: PathBuf) {
        self.tree = build_tree(&path, &self.expanded);
        self.watcher = DirWatcher::new(&path)
            .map_err(|e| log::warn!("DirWatcher init failed: {e}"))
            .ok();
        self.root = Some(path);
    }

    pub fn poll_events(&mut self) -> Vec<(FsChange, PathBuf)> {
        let Some(watcher) = &self.watcher else { return vec![] };
        let events = watcher.poll();
        if events.is_empty() {
            return vec![];
        }
        let mut changes = Vec::new();
        let mut rebuild = false;
        for ev in events {
            match ev {
                DirEvent::Created(p) => { changes.push((FsChange::Added, p)); rebuild = true; }
                DirEvent::Removed(p) => { changes.push((FsChange::Removed, p)); rebuild = true; }
                DirEvent::Modified(p) => { changes.push((FsChange::Modified, p)); }
            }
        }
        if rebuild {
            if let Some(root) = self.root.clone() {
                self.tree = build_tree(&root, &self.expanded);
            }
        }
        changes
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<PathBuf> {
        if !self.visible {
            return None;
        }
        let mut to_open = None;
        let mut toggle_dir: Option<PathBuf> = None;

        egui::SidePanel::left("file_explorer")
            .default_width(220.0)
            .min_width(160.0)
            .resizable(true)
            .frame(theme::side_panel_frame())
            .show(ctx, |ui| {
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
                    show_nodes(ui, ctx, &self.tree, &self.expanded, 0, &mut to_open, &mut toggle_dir);
                });
            });

        if let Some(dir) = toggle_dir {
            if self.expanded.contains(&dir) {
                self.expanded.remove(&dir);
            } else {
                self.expanded.insert(dir);
            }
            if let Some(root) = self.root.clone() {
                self.tree = build_tree(&root, &self.expanded);
            }
        }

        to_open
    }
}

fn show_nodes(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    nodes: &[TreeNode],
    expanded: &HashSet<PathBuf>,
    depth: usize,
    to_open: &mut Option<PathBuf>,
    toggle_dir: &mut Option<PathBuf>,
) {
    for node in nodes {
        let indent = depth as f32 * 14.0;
        let name = node.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let row_height = 22.0;

        match &node.kind {
            NodeKind::Dir(children) => {
                let is_expanded = expanded.contains(&node.path);
                let (row_rect, resp) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), row_height),
                    egui::Sense::click(),
                );

                if ui.is_rect_visible(row_rect) {
                    if resp.hovered() {
                        ui.painter().rect_filled(row_rect, 4.0, theme::BG_HOVER);
                    }
                    let arrow = if is_expanded { "▼" } else { "▶" };
                    ui.painter().text(
                        egui::pos2(row_rect.left() + indent + 4.0, row_rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        arrow,
                        egui::FontId::proportional(9.0),
                        theme::TEXT_MUTED,
                    );
                    let text_color = if resp.hovered() { theme::TEXT_PRIMARY } else { theme::TEXT_OVERLAY };
                    ui.painter().text(
                        egui::pos2(row_rect.left() + indent + 18.0, row_rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        &name,
                        egui::FontId::monospace(13.0),
                        text_color,
                    );
                }

                if resp.clicked() && toggle_dir.is_none() {
                    *toggle_dir = Some(node.path.clone());
                }

                if is_expanded {
                    show_nodes(ui, ctx, children, expanded, depth + 1, to_open, toggle_dir);
                }
            }
            NodeKind::File => {
                let is_fits = is_fits_ext(&node.path);
                let sense = if is_fits { egui::Sense::click() } else { egui::Sense::hover() };
                let (row_rect, resp) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), row_height),
                    sense,
                );

                if ui.is_rect_visible(row_rect) {
                    if resp.hovered() && is_fits {
                        ui.painter().rect_filled(row_rect, 4.0, theme::BG_HOVER);
                    }

                    if is_fits {
                        let bar_rect = egui::Rect::from_min_size(
                            row_rect.left_top(),
                            egui::vec2(2.0, row_height),
                        );
                        ui.painter().rect_filled(bar_rect, egui::Rounding::ZERO, theme::ACCENT_DIM);

                        let icon_color = if resp.hovered() { theme::ACCENT } else { theme::ACCENT_DIM };
                        ui.painter().text(
                            egui::pos2(row_rect.left() + indent + 10.0, row_rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            "◆",
                            egui::FontId::proportional(9.0),
                            icon_color,
                        );
                        let text_x = row_rect.left() + indent + 22.0;
                        let text_color = if resp.hovered() { theme::TEXT_PRIMARY } else { theme::TEXT_OVERLAY };
                        ui.painter().text(
                            egui::pos2(text_x, row_rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            &name,
                            egui::FontId::monospace(13.0),
                            text_color,
                        );
                        if let Some(size_badge) = file_size_badge(&node.path) {
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
                            egui::pos2(row_rect.left() + indent + 10.0, row_rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            &name,
                            egui::FontId::monospace(13.0),
                            theme::TEXT_MUTED,
                        );
                    }
                }

                if is_fits && resp.clicked() {
                    *to_open = Some(node.path.clone());
                }
                resp.context_menu(|ui| {
                    if ui.button("Open").clicked() {
                        *to_open = Some(node.path.clone());
                        ui.close_menu();
                    }
                });
            }
        }
    }
}

fn build_tree(dir: &Path, expanded: &HashSet<PathBuf>) -> Vec<TreeNode> {
    let mut nodes: Vec<TreeNode> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            if path.is_dir() {
                let children = if expanded.contains(&path) {
                    build_tree(&path, expanded)
                } else {
                    Vec::new()
                };
                Some(TreeNode { path, kind: NodeKind::Dir(children) })
            } else {
                Some(TreeNode { path, kind: NodeKind::File })
            }
        })
        .collect();

    // Directories first, then files; both groups sorted alphabetically
    nodes.sort_by(|a, b| {
        match (&a.kind, &b.kind) {
            (NodeKind::Dir(_), NodeKind::File) => std::cmp::Ordering::Less,
            (NodeKind::File, NodeKind::Dir(_)) => std::cmp::Ordering::Greater,
            _ => a.path.file_name().cmp(&b.path.file_name()),
        }
    });
    nodes
}

pub enum FsChange {
    Added,
    Removed,
    Modified,
}

fn is_fits_ext(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".fits") || lower.ends_with(".fit") || lower.ends_with(".fits.gz")
}

fn file_size_badge(path: &Path) -> Option<String> {
    let b = std::fs::metadata(path).ok()?.len();
    if b >= 1 << 30 {
        Some(format!("  {:.1}G", b as f64 / (1u64 << 30) as f64))
    } else if b >= 1 << 20 {
        Some(format!("  {:.0}M", b as f64 / (1u64 << 20) as f64))
    } else {
        None
    }
}

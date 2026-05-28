use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use egui::TextureHandle;
use fitsview_core::{
    colormap::Colormap,
    event_image::EventImage,
    fits_reader::FitsImage,
    mmap_reader::MmapFitsImage,
    scale::{ScaleMode, ScaleResult},
};

use crate::viewport::ViewState;

pub struct LargeImageState {
    pub source: Arc<MmapFitsImage>,
    pub file_id: u64,
}

pub enum FileData {
    Image(FitsImage),
    Event(EventImage),
    LargeImage(LargeImageState),
}

impl FileData {
    pub fn width(&self) -> usize {
        match self {
            FileData::Image(i) => i.width,
            FileData::Event(e) => e.width,
            FileData::LargeImage(l) => l.source.width,
        }
    }

    pub fn height(&self) -> usize {
        match self {
            FileData::Image(i) => i.height,
            FileData::Event(e) => e.height,
            FileData::LargeImage(l) => l.source.height,
        }
    }

    pub fn pixel_data(&self) -> &[f32] {
        match self {
            FileData::Image(i) => &i.data,
            FileData::Event(e) => &e.data,
            FileData::LargeImage(_) => &[],
        }
    }

    pub fn header(&self) -> &HashMap<String, String> {
        match self {
            FileData::Image(i) => &i.header,
            FileData::Event(e) => &e.header,
            FileData::LargeImage(l) => &l.source.header,
        }
    }

    pub fn is_event(&self) -> bool {
        matches!(self, FileData::Event(_))
    }

    pub fn is_large(&self) -> bool {
        matches!(self, FileData::LargeImage(_))
    }
}

pub struct Tab {
    pub id: u64,
    pub path: PathBuf,
    pub data: FileData,
    pub view: ViewState,
    pub scale_mode: ScaleMode,
    pub colormap: Colormap,
    pub scale_result: Option<ScaleResult>,
    pub hdu_index: usize,
    pub texture: Option<TextureHandle>,
    pub needs_retexture: bool,
    pub needs_fit: bool,
    pub error: Option<String>,
    /// Per-tile textures for LargeImage rendering: key = (tx, ty)
    pub tile_textures: HashMap<(usize, usize), TextureHandle>,
}

impl Tab {
    pub fn new(id: u64, path: PathBuf, data: FileData, hdu_index: usize) -> Self {
        Self {
            id,
            path,
            data,
            view: ViewState::default(),
            scale_mode: ScaleMode::ZScale,
            colormap: Colormap::Gray,
            scale_result: None,
            hdu_index,
            texture: None,
            needs_retexture: true,
            needs_fit: true,
            error: None,
            tile_textures: HashMap::new(),
        }
    }

    pub fn title(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unknown".to_owned())
    }
}

pub struct TabManager {
    pub tabs: Vec<Tab>,
    pub active_id: Option<u64>,
    next_id: u64,
}

impl Default for TabManager {
    fn default() -> Self {
        Self { tabs: Vec::new(), active_id: None, next_id: 1 }
    }
}

impl TabManager {
    pub fn open(&mut self, path: PathBuf, data: FileData) {
        // If a tab for this path already exists, activate it
        if let Some(t) = self.tabs.iter().find(|t| t.path == path) {
            let id = t.id;
            self.active_id = Some(id);
            return;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.tabs.push(Tab::new(id, path, data, 0));
        self.active_id = Some(id);
    }

    pub fn close_active(&mut self) {
        if let Some(active) = self.active_id {
            if let Some(pos) = self.tabs.iter().position(|t| t.id == active) {
                self.tabs.remove(pos);
                self.active_id = if self.tabs.is_empty() {
                    None
                } else {
                    let new_pos = pos.min(self.tabs.len() - 1);
                    Some(self.tabs[new_pos].id)
                };
            }
        }
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        let id = self.active_id?;
        self.tabs.iter().find(|t| t.id == id)
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        let id = self.active_id?;
        self.tabs.iter_mut().find(|t| t.id == id)
    }

    pub fn next_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        let pos = self
            .active_id
            .and_then(|id| self.tabs.iter().position(|t| t.id == id))
            .unwrap_or(0);
        let next = (pos + 1) % self.tabs.len();
        self.active_id = Some(self.tabs[next].id);
    }

    pub fn prev_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        let pos = self
            .active_id
            .and_then(|id| self.tabs.iter().position(|t| t.id == id))
            .unwrap_or(0);
        let prev = if pos == 0 { self.tabs.len() - 1 } else { pos - 1 };
        self.active_id = Some(self.tabs[prev].id);
    }

    pub fn set_active(&mut self, id: u64) {
        if self.tabs.iter().any(|t| t.id == id) {
            self.active_id = Some(id);
        }
    }

    /// Draw the tab bar; returns the id that was clicked (if any).
    pub fn show_tab_bar(&mut self, ctx: &egui::Context) -> Option<u64> {
        let mut clicked = None;
        let mut close_id = None;

        egui::TopBottomPanel::top("tab_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let tab_ids: Vec<(u64, String)> =
                    self.tabs.iter().map(|t| (t.id, t.title())).collect();

                for (id, title) in tab_ids {
                    let is_active = Some(id) == self.active_id;
                    let label = if is_active {
                        egui::RichText::new(&title).strong()
                    } else {
                        egui::RichText::new(&title)
                    };

                    ui.visuals_mut().widgets.inactive.bg_fill = if is_active {
                        ui.visuals().selection.bg_fill
                    } else {
                        ui.visuals().widgets.inactive.bg_fill
                    };

                    if ui.add(egui::Button::new(label)).clicked() {
                        clicked = Some(id);
                    }

                    if ui
                        .add(
                            egui::Button::new("×")
                                .min_size(egui::vec2(16.0, 0.0))
                                .frame(false),
                        )
                        .clicked()
                    {
                        close_id = Some(id);
                    }
                    ui.separator();
                }
            });
        });

        if let Some(id) = close_id {
            if let Some(pos) = self.tabs.iter().position(|t| t.id == id) {
                self.tabs.remove(pos);
                if Some(id) == self.active_id {
                    self.active_id = if self.tabs.is_empty() {
                        None
                    } else {
                        let new_pos = pos.min(self.tabs.len() - 1);
                        Some(self.tabs[new_pos].id)
                    };
                }
            }
            return None;
        }

        if let Some(id) = clicked {
            self.active_id = Some(id);
        }

        clicked
    }
}

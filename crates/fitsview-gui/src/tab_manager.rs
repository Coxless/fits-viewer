use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use egui::TextureHandle;
use fitsview_core::{
    colormap::Colormap,
    cube_reader::FitsCube,
    event_image::EventImage,
    fits_reader::{FitsImage, HduInfo},
    mmap_reader::MmapFitsImage,
    region::RegionFile,
    scale::{ScaleMode, ScaleResult},
    stats::ImageStats,
    wcs::Wcs,
};

use crate::{
    annotation::Annotation,
    cube_panel::CubePanel,
    plot_panel::PlotPanel,
    viewport::ViewState,
};

static EMPTY_HEADER: OnceLock<HashMap<String, String>> = OnceLock::new();

pub struct LargeImageState {
    pub source: Arc<MmapFitsImage>,
    pub file_id: u64,
}

pub enum FileData {
    Loading,
    Image(FitsImage),
    Event(EventImage),
    LargeImage(LargeImageState),
    Cube(Arc<FitsCube>),
}

impl FileData {
    pub fn width(&self) -> usize {
        match self {
            FileData::Loading => 0,
            FileData::Image(i) => i.width,
            FileData::Event(e) => e.width,
            FileData::LargeImage(l) => l.source.width,
            FileData::Cube(c) => c.width,
        }
    }

    pub fn height(&self) -> usize {
        match self {
            FileData::Loading => 0,
            FileData::Image(i) => i.height,
            FileData::Event(e) => e.height,
            FileData::LargeImage(l) => l.source.height,
            FileData::Cube(c) => c.height,
        }
    }

    pub fn pixel_data(&self) -> &[f32] {
        match self {
            FileData::Loading => &[],
            FileData::Image(i) => &i.data,
            FileData::Event(e) => &e.data,
            FileData::LargeImage(_) => &[],
            FileData::Cube(_) => &[], // use slice_z() directly
        }
    }

    pub fn header(&self) -> &HashMap<String, String> {
        match self {
            FileData::Loading => EMPTY_HEADER.get_or_init(HashMap::new),
            FileData::Image(i) => &i.header,
            FileData::Event(e) => &e.header,
            FileData::LargeImage(l) => &l.source.header,
            FileData::Cube(c) => &c.header,
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, FileData::Loading)
    }

    pub fn is_event(&self) -> bool {
        matches!(self, FileData::Event(_))
    }

    pub fn is_large(&self) -> bool {
        matches!(self, FileData::LargeImage(_))
    }

    pub fn is_cube(&self) -> bool {
        matches!(self, FileData::Cube(_))
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
    pub needs_hdu_reload: bool,
    pub error: Option<String>,
    /// Per-tile textures for LargeImage rendering: key = (zoom_level, tx, ty)
    pub tile_textures: HashMap<(u8, usize, usize), TextureHandle>,
    /// LOD level in use when tile_textures was last populated
    pub last_lod: u8,
    /// All HDUs in the file
    pub hdu_list: Vec<HduInfo>,
    /// Parsed WCS from header
    pub wcs: Option<Wcs>,
    /// Cached image statistics
    pub stats: Option<Arc<ImageStats>>,
    pub stats_computing: bool,
    /// DS9-style contrast (1.0 = default)
    pub contrast: f32,
    /// DS9-style bias midpoint (0.5 = default)
    pub bias: f32,
    /// HistEq LUT (built on demand when ScaleMode::HistEq is active)
    pub histeq_lut: Option<Arc<Vec<f32>>>,
    /// Loaded DS9 region files
    pub region_files: Vec<RegionFile>,
    /// Show crosshair cursor
    pub crosshair: bool,
    /// Manual vmin override (overrides scale_result.vmin when set)
    pub vmin_override: Option<f32>,
    /// Manual vmax override (overrides scale_result.vmax when set)
    pub vmax_override: Option<f32>,
    /// Cached histogram bins (counts per bin)
    pub hist_bins: Option<Arc<Vec<u32>>>,
    /// Cached histogram bin edges (n_bins + 1 values)
    pub hist_edges: Option<Arc<Vec<f32>>>,
    /// Background histogram computation in progress
    pub hist_computing: bool,
    /// Overlay RA/Dec grid lines on image
    pub show_wcs_grid: bool,
    /// GUI annotations drawn on this image
    pub annotations: Vec<Annotation>,
    /// Current z-slice index for cube data
    pub cube_z: usize,
    /// Cube navigation panel state
    pub cube_panel: Option<CubePanel>,
    /// Shared plot panel (spectrum extraction, 1D profile)
    pub plot_panel: Option<PlotPanel>,
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
            needs_hdu_reload: false,
            error: None,
            tile_textures: HashMap::new(),
            last_lod: 0,
            hdu_list: Vec::new(),
            wcs: None,
            stats: None,
            stats_computing: false,
            contrast: 1.0,
            bias: 0.5,
            histeq_lut: None,
            region_files: Vec::new(),
            crosshair: false,
            vmin_override: None,
            vmax_override: None,
            hist_bins: None,
            hist_edges: None,
            hist_computing: false,
            show_wcs_grid: false,
            annotations: Vec::new(),
            cube_z: 0,
            cube_panel: None,
            plot_panel: None,
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
    /// Open a tab for `path` with `data`. Returns the tab id.
    /// If a tab for this path already exists, activates it and returns its id.
    pub fn open(&mut self, path: PathBuf, data: FileData) -> u64 {
        if let Some(t) = self.tabs.iter().find(|t| t.path == path) {
            let id = t.id;
            self.active_id = Some(id);
            return id;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.tabs.push(Tab::new(id, path, data, 0));
        self.active_id = Some(id);
        id
    }

    /// Replace a Loading tab's data once the background load completes.
    pub fn finish_loading(&mut self, tab_id: u64, data: FileData, hdu_list: Vec<HduInfo>, wcs: Option<Wcs>) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.data = data;
            tab.hdu_list = hdu_list;
            tab.wcs = wcs;
            tab.needs_fit = true;
            tab.needs_retexture = true;
            tab.histeq_lut = None;
            tab.stats = None;
            tab.stats_computing = false;
            tab.vmin_override = None;
            tab.vmax_override = None;
            tab.hist_bins = None;
            tab.hist_edges = None;
            tab.hist_computing = false;
        }
    }

    /// Mark a Loading tab as having failed.
    pub fn set_load_error(&mut self, tab_id: u64, error: String) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.error = Some(error);
        }
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
        use crate::theme;

        let mut clicked = None;
        let mut close_id = None;

        egui::TopBottomPanel::top("tab_bar")
            .frame(theme::tab_bar_frame())
            .exact_height(34.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.spacing_mut().button_padding = egui::vec2(10.0, 6.0);

                    let tab_ids: Vec<(u64, String)> =
                        self.tabs.iter().map(|t| (t.id, t.title())).collect();

                    for (id, title) in tab_ids {
                        let is_active = Some(id) == self.active_id;
                        let fill = if is_active { theme::BG_SELECTED } else { theme::BG_PANEL };

                        let label_text = egui::RichText::new(&title)
                            .size(13.5)
                            .color(if is_active { theme::TEXT_PRIMARY } else { theme::TEXT_OVERLAY });

                        let tab_resp = ui.add(
                            egui::Button::new(label_text)
                                .fill(fill)
                                .stroke(egui::Stroke::NONE)
                                .rounding(egui::Rounding::ZERO)
                                .min_size(egui::vec2(0.0, 34.0)),
                        );

                        if is_active {
                            let r = tab_resp.rect;
                            ui.painter().line_segment(
                                [r.left_bottom(), r.right_bottom()],
                                egui::Stroke::new(2.0, theme::ACCENT),
                            );
                        } else if tab_resp.hovered() {
                            let r = tab_resp.rect;
                            ui.painter().rect_filled(r, egui::Rounding::ZERO, theme::BG_HOVER);
                        }

                        if tab_resp.clicked() {
                            clicked = Some(id);
                        }

                        let close_resp = ui.add(
                            egui::Button::new(
                                egui::RichText::new("×")
                                    .size(14.0)
                                    .color(theme::TEXT_MUTED),
                            )
                            .fill(fill)
                            .stroke(egui::Stroke::NONE)
                            .rounding(egui::Rounding::ZERO)
                            .min_size(egui::vec2(24.0, 34.0)),
                        );
                        if close_resp.clicked() {
                            close_id = Some(id);
                        }

                        let divider_rect = egui::Rect::from_min_size(
                            close_resp.rect.right_top(),
                            egui::vec2(1.0, 34.0),
                        );
                        ui.painter().rect_filled(
                            divider_rect,
                            egui::Rounding::ZERO,
                            theme::SEPARATOR,
                        );
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

use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use egui::{vec2, Color32, ColorImage, Rect, TextureHandle, TextureOptions, ViewportCommand};
use fitsview_core::{
    colormap::render_to_rgba,
    cube_reader::load_cube,
    event_image::bin_events,
    fits_reader::{list_hdus, load_fits, load_fits_hdu, HduInfo, HduType},
    mmap_reader::MmapFitsImage,
    region::load_region_file,
    scale::{build_histeq_lut, compute_scale, ScaleMode},
    session::{FileState, Session, SessionSplitLayout},
    stats::{compute_stats, ImageStats},
    tile_loader::{TileLoader, TileRequest},
    tile_manager::{TileKey, TileManager},
    wcs::Wcs,
};

use crate::{
    annotation::AnnotationOverlay,
    command_palette::{Command, CommandPalette},
    cube_panel::CubePanel,
    file_explorer::FileExplorer,
    header_panel::HeaderPanel,
    histogram_panel::{compute_histogram, HistogramPanel},
    region_overlay::draw_regions,
    renderer::{create_renderer, RenderParams, Renderer},
    split_view::{SplitLayout, SplitView},
    stats_panel::StatsPanel,
    status_bar::StatusBar,
    tab_manager::{FileData, LargeImageState, Tab, TabManager},
    theme,
    viewport::ViewState,
    wcs_overlay::draw_wcs_grid,
};

/// Files larger than this are opened via the tile-based LargeImage path.
const LARGE_FILE_THRESHOLD: u64 = 512 * 1024 * 1024;

/// Map a viewport zoom factor to a discrete LOD level (0 = full resolution).
fn zoom_to_lod(zoom: f32) -> u8 {
    if zoom >= 1.0 { 0 } else { ((1.0_f32 / zoom).log2().ceil() as u8).min(6) }
}

struct LoadResult {
    data: FileData,
    hdu_list: Vec<HduInfo>,
    wcs: Option<Wcs>,
}

pub struct FitsViewApp {
    tabs: TabManager,
    file_explorer: FileExplorer,
    header_panel: HeaderPanel,
    split_view: SplitView,
    command_palette: CommandPalette,
    stats_panel: StatsPanel,
    open_path_input: Option<String>,
    open_dir_input: Option<String>,
    open_region_input: Option<String>,
    last_key_ctrl_k: bool,
    cursor_image_pos: Option<egui::Pos2>,
    // Large-file tile infrastructure
    tile_manager: TileManager,
    tile_loader: TileLoader,
    #[allow(dead_code)]
    tokio_rt: Arc<tokio::runtime::Runtime>,
    renderer: Box<dyn Renderer>,
    // Background file loading
    load_tx: mpsc::Sender<(u64, anyhow::Result<LoadResult>)>,
    load_rx: mpsc::Receiver<(u64, anyhow::Result<LoadResult>)>,
    // Background stats computation
    stats_tx: mpsc::Sender<(u64, Arc<ImageStats>)>,
    stats_rx: mpsc::Receiver<(u64, Arc<ImageStats>)>,
    // Histogram panel
    histogram_panel: HistogramPanel,
    #[allow(clippy::type_complexity)]
    hist_tx: mpsc::Sender<(u64, Arc<Vec<u32>>, Arc<Vec<f32>>)>,
    #[allow(clippy::type_complexity)]
    hist_rx: mpsc::Receiver<(u64, Arc<Vec<u32>>, Arc<Vec<f32>>)>,
    // Annotation overlay
    annotation_overlay: AnnotationOverlay,
    // Session dialog
    session_load_input: Option<String>,
    // Blink state
    blink_enabled: bool,
    blink_interval_secs: f32,
    blink_last_switch: Instant,
    blink_tab_idx: usize,
}

impl FitsViewApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        initial_paths: Vec<PathBuf>,
        initial_dir: Option<PathBuf>,
    ) -> Self {
        theme::apply(&cc.egui_ctx);

        let tokio_rt = Arc::new(tokio::runtime::Runtime::new().expect("tokio runtime"));
        let tile_loader = TileLoader::new(&tokio_rt);
        let tile_manager = TileManager::new(TileManager::MAX_MEMORY);

        #[cfg(feature = "gpu")]
        let renderer = create_renderer(cc.wgpu_render_state.as_ref());
        #[cfg(not(feature = "gpu"))]
        let renderer = { let _ = cc; create_renderer() };

        let (load_tx, load_rx) = mpsc::channel();
        let (stats_tx, stats_rx) = mpsc::channel();
        let (hist_tx, hist_rx) = mpsc::channel();

        let mut app = Self {
            tabs: TabManager::default(),
            file_explorer: FileExplorer::default(),
            header_panel: HeaderPanel::default(),
            split_view: SplitView::default(),
            command_palette: CommandPalette::default(),
            stats_panel: StatsPanel::default(),
            open_path_input: None,
            open_dir_input: None,
            open_region_input: None,
            last_key_ctrl_k: false,
            cursor_image_pos: None,
            tile_manager,
            tile_loader,
            tokio_rt,
            renderer,
            load_tx,
            load_rx,
            stats_tx,
            stats_rx,
            histogram_panel: HistogramPanel::default(),
            hist_tx,
            hist_rx,
            annotation_overlay: AnnotationOverlay::default(),
            session_load_input: None,
            blink_enabled: false,
            blink_interval_secs: 0.5,
            blink_last_switch: Instant::now(),
            blink_tab_idx: 0,
        };

        if let Some(dir) = initial_dir {
            app.file_explorer.set_root(dir);
        }

        for path in initial_paths {
            app.open_file(&path);
        }

        app
    }

    fn open_file(&mut self, path: &Path) {
        if let Some(existing) = self.tabs.tabs.iter().find(|t| t.path == path) {
            let id = existing.id;
            self.tabs.set_active(id);
            self.split_view.assign_active(id);
            return;
        }

        let tab_id = self.tabs.open(path.to_path_buf(), FileData::Loading);
        self.split_view.assign_active(tab_id);

        let tx = self.load_tx.clone();
        let path = path.to_path_buf();
        std::thread::spawn(move || {
            let _ = tx.send((tab_id, load_file_full(&path)));
        });
    }

    fn reload_hdu(&mut self, tab_id: u64, hdu_index: usize, path: PathBuf) {
        let tx = self.load_tx.clone();
        std::thread::spawn(move || {
            let result = load_fits_hdu(&path, hdu_index)
                .map(|img| {
                    let wcs = Wcs::from_header(&img.header);
                    let hdu_list = list_hdus(&path).unwrap_or_default();
                    LoadResult { data: FileData::Image(img), hdu_list, wcs }
                });
            let _ = tx.send((tab_id, result));
        });
    }

    fn handle_keys(&mut self, ctx: &egui::Context) -> bool {
        let mut do_fit = false;

        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::H) {
                self.header_panel.visible = !self.header_panel.visible;
            }
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::B) {
                self.file_explorer.visible = !self.file_explorer.visible;
            }
            if i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::P) {
                self.command_palette.open();
            }
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::Num0) {
                do_fit = true;
            }
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::Plus)
                || i.consume_key(egui::Modifiers::CTRL, egui::Key::Equals)
            {
                if let Some(tab) = self.tabs.active_tab_mut() {
                    tab.view.zoom = (tab.view.zoom * ViewState::ZOOM_STEP)
                        .clamp(ViewState::ZOOM_MIN, ViewState::ZOOM_MAX);
                }
            }
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::Minus) {
                if let Some(tab) = self.tabs.active_tab_mut() {
                    tab.view.zoom = (tab.view.zoom / ViewState::ZOOM_STEP)
                        .clamp(ViewState::ZOOM_MIN, ViewState::ZOOM_MAX);
                }
            }
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::W) {
                if let Some(id) = self.tabs.active_id {
                    self.split_view.remove_tab(id);
                }
                self.tabs.close_active();
                if let Some(id) = self.tabs.active_id {
                    self.split_view.fill_empty(id);
                }
            }
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::Tab) {
                self.tabs.next_tab();
                if let Some(id) = self.tabs.active_id {
                    self.split_view.assign_active(id);
                }
            }
            if i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::Tab) {
                self.tabs.prev_tab();
                if let Some(id) = self.tabs.active_id {
                    self.split_view.assign_active(id);
                }
            }

            let ctrl_backslash = i.consume_key(egui::Modifiers::CTRL, egui::Key::Backslash);
            if ctrl_backslash && !self.last_key_ctrl_k {
                self.split_view.set_layout(SplitLayout::SideBySide);
            }
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::K) {
                self.last_key_ctrl_k = true;
            } else if self.last_key_ctrl_k {
                if ctrl_backslash {
                    self.split_view.set_layout(SplitLayout::Grid2x2);
                }
                self.last_key_ctrl_k = false;
            }

            // HDU navigation with [ and ]
            if i.consume_key(egui::Modifiers::NONE, egui::Key::OpenBracket) {
                if let Some(tab) = self.tabs.active_tab_mut() {
                    if tab.hdu_index > 0 {
                        tab.hdu_index -= 1;
                        tab.needs_hdu_reload = true;
                    }
                }
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::CloseBracket) {
                if let Some(tab) = self.tabs.active_tab_mut() {
                    tab.hdu_index += 1;
                    tab.needs_hdu_reload = true;
                }
            }

            // New feature keys
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::I) {
                self.stats_panel.visible = !self.stats_panel.visible;
            }
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::R) {
                self.open_region_input = Some(String::new());
            }
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::L) {
                self.toggle_blink();
            }
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::X) {
                if let Some(tab) = self.tabs.active_tab_mut() {
                    tab.crosshair = !tab.crosshair;
                }
            }
        });

        do_fit
    }

    fn toggle_blink(&mut self) {
        self.blink_enabled = !self.blink_enabled;
        if self.blink_enabled {
            self.blink_last_switch = Instant::now();
            self.blink_tab_idx = self.tabs
                .active_id
                .and_then(|id| self.tabs.tabs.iter().position(|t| t.id == id))
                .unwrap_or(0);
        }
    }

    fn apply_command(&mut self, cmd: Command) {
        match cmd {
            Command::OpenFile => self.open_path_input = Some(String::new()),
            Command::OpenDirectory => self.open_dir_input = Some(String::new()),
            Command::SetColormap(cmap) => {
                if let Some(tab) = self.tabs.active_tab_mut() {
                    tab.colormap = cmap;
                    tab.needs_retexture = true;
                    tab.tile_textures.clear();
                }
            }
            Command::SetScale(mode) => {
                if let Some(tab) = self.tabs.active_tab_mut() {
                    tab.scale_mode = mode;
                    tab.scale_result = None;
                    tab.histeq_lut = None;
                    tab.needs_retexture = true;
                    tab.tile_textures.clear();
                }
            }
            Command::ToggleSidebar => self.file_explorer.visible = !self.file_explorer.visible,
            Command::ToggleHeaderPanel => self.header_panel.visible = !self.header_panel.visible,
            Command::SplitVertical => self.split_view.set_layout(SplitLayout::SideBySide),
            Command::SplitHorizontal => self.split_view.set_layout(SplitLayout::Grid2x2),
            Command::FitToWindow => {
                if let Some(tab) = self.tabs.active_tab_mut() {
                    tab.needs_fit = true;
                }
            }
            Command::ToggleStats => self.stats_panel.visible = !self.stats_panel.visible,
            Command::LoadRegionFile => self.open_region_input = Some(String::new()),
            Command::ToggleBlink => self.toggle_blink(),
            Command::ResetContrastBias => {
                if let Some(tab) = self.tabs.active_tab_mut() {
                    tab.contrast = 1.0;
                    tab.bias = 0.5;
                    tab.needs_retexture = true;
                    tab.tile_textures.clear();
                }
            }
            Command::ToggleCrosshair => {
                if let Some(tab) = self.tabs.active_tab_mut() {
                    tab.crosshair = !tab.crosshair;
                }
            }
            Command::ToggleHistogram => {
                self.histogram_panel.visible = !self.histogram_panel.visible;
            }
            Command::ToggleWcsGrid => {
                if let Some(tab) = self.tabs.active_tab_mut() {
                    tab.show_wcs_grid = !tab.show_wcs_grid;
                }
            }
            Command::ToggleLinkPanes => {
                self.split_view.linked = !self.split_view.linked;
            }
            Command::SaveSession => {
                let session = build_session_from_app(self);
                if let Err(e) = session.save(&Session::last_path()) {
                    log::warn!("Failed to save session: {e}");
                } else {
                    log::info!("Session saved to {:?}", Session::last_path());
                }
            }
            Command::OpenSession => {
                self.session_load_input = Some(String::new());
            }
            Command::AnnotationMode(shape) => {
                use crate::annotation::AnnotationMode;
                self.annotation_overlay.mode = AnnotationMode::Placing(shape);
            }
        }
    }

    fn show_path_input_dialogs(&mut self, ctx: &egui::Context) {
        let mut file_to_open: Option<PathBuf> = None;
        if let Some(ref mut input) = self.open_path_input.clone() {
            let mut close = false;
            egui::Window::new("Open File")
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Path:");
                        ui.text_edit_singleline(input);
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Open").clicked() {
                            file_to_open = Some(PathBuf::from(input.trim()));
                            close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                });
            if close {
                self.open_path_input = None;
            } else {
                self.open_path_input = Some(input.clone());
            }
        }
        if let Some(p) = file_to_open {
            self.open_file(&p);
        }

        let mut dir_to_open: Option<PathBuf> = None;
        if let Some(ref mut input) = self.open_dir_input.clone() {
            let mut close = false;
            egui::Window::new("Open Directory")
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Path:");
                        ui.text_edit_singleline(input);
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Open").clicked() {
                            dir_to_open = Some(PathBuf::from(input.trim()));
                            close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                });
            if close {
                self.open_dir_input = None;
            } else {
                self.open_dir_input = Some(input.clone());
            }
        }
        if let Some(p) = dir_to_open {
            if p.is_dir() {
                self.file_explorer.set_root(p);
                self.file_explorer.visible = true;
            }
        }

        // Region file dialog
        let mut region_to_load: Option<PathBuf> = None;
        if let Some(ref mut input) = self.open_region_input.clone() {
            let mut close = false;
            egui::Window::new("Load Region File")
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label("DS9 .reg file path:");
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(input);
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Load").clicked() {
                            region_to_load = Some(PathBuf::from(input.trim()));
                            close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                });
            if close {
                self.open_region_input = None;
            } else {
                self.open_region_input = Some(input.clone());
            }
        }
        if let Some(p) = region_to_load {
            match load_region_file(&p) {
                Ok(rf) => {
                    if let Some(tab) = self.tabs.active_tab_mut() {
                        tab.region_files.push(rf);
                    }
                }
                Err(e) => log::warn!("Failed to load region file: {e}"),
            }
        }

        // Session load dialog
        let mut session_to_load: Option<PathBuf> = None;
        if let Some(ref mut input) = self.session_load_input.clone() {
            let mut close = false;
            egui::Window::new("Open Session")
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label("Session file (.fvs) path:");
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(input);
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Load").clicked() {
                            session_to_load = Some(PathBuf::from(input.trim()));
                            close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                });
            if close {
                self.session_load_input = None;
            } else {
                self.session_load_input = Some(input.clone());
            }
        }
        if let Some(p) = session_to_load {
            match Session::load(&p) {
                Ok(session) => restore_session_to_app(self, session),
                Err(e) => log::warn!("Failed to load session: {e}"),
            }
        }
    }

    // --- Loading / error state rendering ---

    fn render_loading_pane(tab: &Tab, ui: &mut egui::Ui) {
        let available = ui.available_rect_before_wrap();
        ui.allocate_rect(available, egui::Sense::hover());

        if let Some(ref err) = tab.error {
            ui.painter().text(
                available.center(),
                egui::Align2::CENTER_CENTER,
                format!("Error loading file:\n{err}"),
                egui::FontId::proportional(13.0),
                egui::Color32::from_rgb(255, 80, 80),
            );
        } else {
            let spinner_center = available.center() - egui::vec2(0.0, 14.0);
            let spinner_rect =
                egui::Rect::from_center_size(spinner_center, egui::vec2(28.0, 28.0));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(spinner_rect));
            child.add(egui::Spinner::new().size(28.0));

            ui.painter().text(
                available.center() + egui::vec2(0.0, 22.0),
                egui::Align2::CENTER_CENTER,
                format!("Loading {}…", tab.title()),
                egui::FontId::proportional(13.0),
                ui.visuals().weak_text_color(),
            );
            ui.ctx().request_repaint();
        }
    }

    // --- Small-image rendering ---

    fn render_pane(
        tab: &mut Tab,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        do_fit: bool,
    ) -> Option<egui::Pos2> {
        let available = ui.available_rect_before_wrap();

        if tab.texture.is_none() || tab.needs_retexture {
            tab.texture = Some(build_texture(ctx, tab));
            tab.needs_retexture = false;
        }

        if tab.needs_fit || do_fit {
            tab.needs_fit = false;
            let img_size = vec2(tab.data.width() as f32, tab.data.height() as f32);
            tab.view.fit_to_rect(available, img_size);
        }

        let response = ui.allocate_rect(available, egui::Sense::drag());

        let scroll_delta = ctx.input(|i| i.smooth_scroll_delta.y);
        if response.hovered() && scroll_delta.abs() > 0.1 {
            let factor = (scroll_delta / 50.0).exp();
            let cursor_pos = ctx
                .input(|i| i.pointer.hover_pos())
                .unwrap_or(available.center());
            let cursor_rel = cursor_pos.to_vec2() - available.min.to_vec2();
            tab.view.zoom_toward(factor, cursor_rel);
        }

        if response.dragged_by(egui::PointerButton::Primary) {
            tab.view.pan(response.drag_delta());
        }

        // Right-drag: adjust contrast (X) and bias (Y)
        if response.dragged_by(egui::PointerButton::Secondary) {
            let delta = response.drag_delta();
            tab.contrast = (tab.contrast + delta.x * 0.01).clamp(0.05, 10.0);
            tab.bias = (tab.bias - delta.y * 0.005).clamp(0.0, 1.0);
            tab.needs_retexture = true;
            tab.tile_textures.clear();
        }

        let cursor_image_pos =
            ctx.input(|i| i.pointer.hover_pos()).and_then(|screen_pos| {
                if !response.hovered() {
                    return None;
                }
                let panel_pos = screen_pos - available.min;
                let img_x = panel_pos.x / tab.view.zoom - tab.view.offset.x;
                let img_y = panel_pos.y / tab.view.zoom - tab.view.offset.y;
                let w = tab.data.width() as f32;
                let h = tab.data.height() as f32;
                if img_x >= 0.0 && img_y >= 0.0 && img_x < w && img_y < h {
                    Some(egui::pos2(img_x, img_y))
                } else {
                    None
                }
            });

        let painter = ui.painter();

        if let Some(texture) = &tab.texture {
            let img_size = vec2(tab.data.width() as f32, tab.data.height() as f32);
            let display_size = img_size * tab.view.zoom;
            let display_origin = available.min.to_vec2() + tab.view.offset * tab.view.zoom;
            let display_rect = Rect::from_min_size(display_origin.to_pos2(), display_size);
            let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            painter.image(texture.id(), display_rect, uv, Color32::WHITE);
        }

        // Draw region overlays
        if !tab.region_files.is_empty() {
            draw_regions(
                painter,
                &tab.region_files,
                available,
                &tab.view,
                tab.data.width(),
                tab.data.height(),
                tab.wcs.as_ref(),
            );
        }

        // WCS grid overlay
        if tab.show_wcs_grid {
            draw_wcs_grid(painter, tab, available, &tab.view);
        }

        // Crosshair
        if tab.crosshair {
            if let Some(screen_pos) = ctx.input(|i| i.pointer.hover_pos()) {
                if available.contains(screen_pos) {
                    let stroke = egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 255, 0, 180));
                    painter.line_segment([egui::pos2(available.min.x, screen_pos.y), egui::pos2(available.max.x, screen_pos.y)], stroke);
                    painter.line_segment([egui::pos2(screen_pos.x, available.min.y), egui::pos2(screen_pos.x, available.max.y)], stroke);
                }
            }
        }

        // Annotations (draw-only; interaction is handled in update())
        if !tab.annotations.is_empty() {
            use crate::annotation::draw_annotations;
            draw_annotations(painter, &tab.annotations, &tab.view, available, tab.wcs.as_ref());
        }

        cursor_image_pos
    }

    // --- Large-image tile rendering ---

    #[allow(clippy::too_many_arguments)]
    fn render_large_pane(
        tab: &mut Tab,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        do_fit: bool,
        tile_manager: &mut TileManager,
        renderer: &dyn Renderer,
        missing: &mut Vec<(u64, u8, usize, usize)>,
    ) -> Option<egui::Pos2> {
        let available = ui.available_rect_before_wrap();
        let w = tab.data.width();
        let h = tab.data.height();
        let ts = TileManager::TILE_SIZE;

        if tab.needs_retexture {
            tab.tile_textures.clear();
            tab.scale_result = None;
            tab.needs_retexture = false;
        }

        if tab.needs_fit || do_fit {
            tab.needs_fit = false;
            tab.view.fit_to_rect(available, vec2(w as f32, h as f32));
        }

        let response = ui.allocate_rect(available, egui::Sense::drag());

        let scroll_delta = ctx.input(|i| i.smooth_scroll_delta.y);
        if response.hovered() && scroll_delta.abs() > 0.1 {
            let factor = (scroll_delta / 50.0).exp();
            let cursor_pos = ctx
                .input(|i| i.pointer.hover_pos())
                .unwrap_or(available.center());
            let cursor_rel = cursor_pos.to_vec2() - available.min.to_vec2();
            tab.view.zoom_toward(factor, cursor_rel);
        }

        if response.dragged_by(egui::PointerButton::Primary) {
            tab.view.pan(response.drag_delta());
        }

        // Right-drag: adjust contrast/bias
        if response.dragged_by(egui::PointerButton::Secondary) {
            let delta = response.drag_delta();
            tab.contrast = (tab.contrast + delta.x * 0.01).clamp(0.05, 10.0);
            tab.bias = (tab.bias - delta.y * 0.005).clamp(0.0, 1.0);
            tab.tile_textures.clear();
        }

        let zoom = tab.view.zoom;
        let offset = tab.view.offset;

        let lod = zoom_to_lod(zoom);
        let scale = 1usize << lod;
        let effective_tile = ts * scale;

        if lod != tab.last_lod {
            tab.tile_textures.clear();
            tab.scale_result = None;
            tab.last_lod = lod;
        }

        let vis_x0 = -offset.x;
        let vis_x1 = available.width() / zoom - offset.x;
        let vis_y0 = -offset.y;
        let vis_y1 = available.height() / zoom - offset.y;

        let tiles_x = w.div_ceil(effective_tile);
        let tiles_y = h.div_ceil(effective_tile);

        let tx_min = ((vis_x0 / effective_tile as f32).floor() as isize).max(0) as usize;
        let tx_max = ((vis_x1 / effective_tile as f32).ceil() as isize).min(tiles_x as isize) as usize;
        let ty_min = ((vis_y0 / effective_tile as f32).floor() as isize).max(0) as usize;
        let ty_max = ((vis_y1 / effective_tile as f32).ceil() as isize).min(tiles_y as isize) as usize;

        let file_id = if let FileData::LargeImage(ref lis) = tab.data {
            lis.file_id
        } else {
            return None;
        };

        let vmin = tab.scale_result.map(|s| s.vmin).unwrap_or(0.0);
        let vmax = tab.scale_result.map(|s| s.vmax).unwrap_or(1.0);
        let colormap = tab.colormap;
        let scale_mode = tab.scale_mode;
        let contrast = tab.contrast;
        let bias = tab.bias;
        let hdu_index = tab.hdu_index;

        for ty in ty_min..ty_max {
            for tx in tx_min..tx_max {
                let key = TileKey { file_id, hdu: hdu_index, zoom_level: lod, tx, ty };

                let tile_img_x = (tx * effective_tile) as f32;
                let tile_img_y = (ty * effective_tile) as f32;
                let tile_img_w = (effective_tile).min(w - tx * effective_tile) as f32;
                let tile_img_h = (effective_tile).min(h - ty * effective_tile) as f32;
                let screen_origin = available.min.to_vec2()
                    + egui::vec2(
                        (tile_img_x + offset.x) * zoom,
                        (tile_img_y + offset.y) * zoom,
                    );
                let screen_rect = Rect::from_min_size(
                    screen_origin.to_pos2(),
                    egui::vec2(tile_img_w * zoom, tile_img_h * zoom),
                );

                if let Some(tile_arc) = tile_manager.get(&key) {
                    if tab.scale_result.is_none() {
                        tab.scale_result = Some(compute_scale(&tile_arc.pixels, scale_mode));
                    }

                    let vmin_use = tab.scale_result.map(|s| s.vmin).unwrap_or(vmin);
                    let vmax_use = tab.scale_result.map(|s| s.vmax).unwrap_or(vmax);
                    let lut_ref = tab.histeq_lut.as_ref().map(|l| l.as_slice());

                    let texture = tab.tile_textures.entry((lod, tx, ty)).or_insert_with(|| {
                        let rgba = renderer.render(&RenderParams {
                            pixels: &tile_arc.pixels,
                            width: tile_arc.width,
                            height: tile_arc.height,
                            vmin: vmin_use,
                            vmax: vmax_use,
                            scale_mode,
                            colormap,
                            contrast,
                            bias,
                            histeq_lut: lut_ref,
                        });
                        let ci = ColorImage::from_rgba_unmultiplied(
                            [tile_arc.width, tile_arc.height],
                            &rgba,
                        );
                        ctx.load_texture(
                            format!("tile-{file_id}-{lod}-{tx}-{ty}"),
                            ci,
                            TextureOptions::LINEAR,
                        )
                    });

                    let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                    ui.painter().image(texture.id(), screen_rect, uv, Color32::WHITE);
                } else {
                    ui.painter().rect_filled(screen_rect, 0.0, Color32::from_rgb(60, 60, 60));
                    missing.push((file_id, lod, tx, ty));
                }
            }
        }

        // Region overlays
        if !tab.region_files.is_empty() {
            draw_regions(
                ui.painter(),
                &tab.region_files,
                available,
                &tab.view,
                w,
                h,
                tab.wcs.as_ref(),
            );
        }

        // WCS grid overlay
        if tab.show_wcs_grid {
            draw_wcs_grid(ui.painter(), tab, available, &tab.view);
        }

        // Crosshair
        if tab.crosshair {
            if let Some(screen_pos) = ctx.input(|i| i.pointer.hover_pos()) {
                if available.contains(screen_pos) {
                    let stroke = egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 255, 0, 180));
                    ui.painter().line_segment([egui::pos2(available.min.x, screen_pos.y), egui::pos2(available.max.x, screen_pos.y)], stroke);
                    ui.painter().line_segment([egui::pos2(screen_pos.x, available.min.y), egui::pos2(screen_pos.x, available.max.y)], stroke);
                }
            }
        }

        ctx.input(|i| i.pointer.hover_pos()).and_then(|screen_pos| {
            if !response.hovered() {
                return None;
            }
            let panel_pos = screen_pos - available.min;
            let img_x = panel_pos.x / zoom - offset.x;
            let img_y = panel_pos.y / zoom - offset.y;
            if img_x >= 0.0 && img_y >= 0.0 && img_x < w as f32 && img_y < h as f32 {
                Some(egui::pos2(img_x, img_y))
            } else {
                None
            }
        })
    }
}

/// Build or refresh the texture for the current z-slice of a cube tab.
fn ensure_cube_texture(tab: &mut Tab, ctx: &egui::Context) {
    if !tab.needs_retexture && tab.texture.is_some() {
        return;
    }
    let FileData::Cube(ref cube) = tab.data else { return };
    let slice = cube.slice_z(tab.cube_z).to_vec();
    if slice.is_empty() {
        return;
    }
    if tab.scale_result.is_none() {
        tab.scale_result = Some(compute_scale(&slice, tab.scale_mode));
    }
    let sr = tab.scale_result.as_ref().unwrap();
    let vmin = tab.vmin_override.unwrap_or(sr.vmin);
    let vmax = tab.vmax_override.unwrap_or(sr.vmax);
    let lut_ref = tab.histeq_lut.as_ref().map(|l| l.as_slice());
    let rgba = render_to_rgba(&slice, vmin, vmax, tab.colormap, tab.scale_mode, tab.contrast, tab.bias, lut_ref);
    let ci = ColorImage::from_rgba_unmultiplied([cube.width, cube.height], &rgba);
    tab.texture = Some(ctx.load_texture(format!("cube-tab-{}-z{}", tab.id, tab.cube_z), ci, TextureOptions::LINEAR));
    tab.needs_retexture = false;
}

fn build_session_from_app(app: &FitsViewApp) -> Session {
    let layout_kind = match app.split_view.layout {
        SplitLayout::Single => "single",
        SplitLayout::SideBySide => "side_by_side",
        SplitLayout::Grid2x2 => "grid2x2",
    };
    let pane_paths = app.split_view.pane_tab_ids.iter().map(|id_opt| {
        id_opt.and_then(|id| app.tabs.tabs.iter().find(|t| t.id == id))
              .map(|t| t.path.to_string_lossy().into_owned())
    }).collect();

    let files = app.tabs.tabs.iter().map(|tab| {
        let colormap_name = format!("{:?}", tab.colormap).to_lowercase();
        let scale_name = format!("{:?}", tab.scale_mode).to_lowercase();
        FileState {
            path: tab.path.to_string_lossy().into_owned(),
            hdu_index: tab.hdu_index,
            display: fitsview_core::session::DisplayConfig {
                scale_mode: scale_name,
                colormap: colormap_name,
                vmin_override: tab.vmin_override,
                vmax_override: tab.vmax_override,
                zoom: tab.view.zoom,
                offset_x: tab.view.offset.x,
                offset_y: tab.view.offset.y,
                hdu_index: tab.hdu_index,
                show_wcs_grid: tab.show_wcs_grid,
                cube_z: tab.cube_z,
                contrast: tab.contrast,
                bias: tab.bias,
            },
            annotations: Vec::new(), // simplified: skip annotation serialization for now
            region_files: tab.region_files.iter().map(|_| String::new()).collect(),
            show_wcs_grid: tab.show_wcs_grid,
        }
    }).collect();

    Session {
        version: 1,
        files,
        split: SessionSplitLayout {
            kind: layout_kind.to_owned(),
            pane_paths,
            linked: app.split_view.linked,
        },
    }
}

fn restore_session_to_app(app: &mut FitsViewApp, session: Session) {
    // Open all files from the session
    for file_state in &session.files {
        let path = PathBuf::from(&file_state.path);
        if path.exists() {
            app.open_file(&path);
            // Restore display settings once loaded (they'll be applied on next retexture)
            if let Some(tab) = app.tabs.tabs.iter_mut().find(|t| t.path == path) {
                tab.vmin_override = file_state.display.vmin_override;
                tab.vmax_override = file_state.display.vmax_override;
                tab.show_wcs_grid = file_state.show_wcs_grid;
                tab.contrast = file_state.display.contrast;
                tab.bias = file_state.display.bias;
            }
        }
    }

    // Restore split layout
    use crate::split_view::SplitLayout;
    match session.split.kind.as_str() {
        "side_by_side" => app.split_view.set_layout(SplitLayout::SideBySide),
        "grid2x2" => app.split_view.set_layout(SplitLayout::Grid2x2),
        _ => app.split_view.set_layout(SplitLayout::Single),
    }
    app.split_view.linked = session.split.linked;
}

fn build_texture(ctx: &egui::Context, tab: &mut Tab) -> TextureHandle {
    let data = tab.data.pixel_data();
    if tab.scale_result.is_none() {
        tab.scale_result = Some(compute_scale(data, tab.scale_mode));
    }
    let sr = tab.scale_result.as_ref().unwrap();

    // Build HistEq LUT if needed
    if tab.scale_mode == ScaleMode::HistEq && tab.histeq_lut.is_none() {
        let lut = build_histeq_lut(data, sr.vmin, sr.vmax);
        tab.histeq_lut = Some(Arc::new(lut));
    }

    let vmin = tab.vmin_override.unwrap_or(sr.vmin);
    let vmax = tab.vmax_override.unwrap_or(sr.vmax);
    let lut_ref = tab.histeq_lut.as_ref().map(|l| l.as_slice());
    let rgba = render_to_rgba(data, vmin, vmax, tab.colormap, tab.scale_mode, tab.contrast, tab.bias, lut_ref);
    let w = tab.data.width();
    let h = tab.data.height();
    let ci = ColorImage::from_rgba_unmultiplied([w, h], &rgba);
    ctx.load_texture(format!("fits-tab-{}", tab.id), ci, TextureOptions::LINEAR)
}

impl eframe::App for FitsViewApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let session = build_session_from_app(self);
        if let Err(e) = session.save(&Session::last_path()) {
            log::warn!("Auto-save session failed: {e}");
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let title = self
            .tabs
            .active_tab()
            .map(|t| format!("fits-view — {}", t.title()))
            .unwrap_or_else(|| "fits-view".to_owned());
        ctx.send_viewport_cmd(ViewportCommand::Title(title));

        // Drain completed background file loads
        while let Ok((tab_id, result)) = self.load_rx.try_recv() {
            match result {
                Ok(mut lr) => {
                    if let FileData::LargeImage(ref mut lis) = lr.data {
                        lis.file_id = self.tile_manager.register_file();
                    }
                    // Create CubePanel for cube data
                    let cube_panel = if let FileData::Cube(ref c) = lr.data {
                        Some(CubePanel::new(c.depth))
                    } else {
                        None
                    };
                    self.tabs.finish_loading(tab_id, lr.data, lr.hdu_list, lr.wcs);
                    if let Some(panel) = cube_panel {
                        if let Some(tab) = self.tabs.tabs.iter_mut().find(|t| t.id == tab_id) {
                            tab.cube_panel = Some(panel);
                        }
                    }
                }
                Err(e) => {
                    log::error!("Background load failed for tab {tab_id}: {e}");
                    self.tabs.set_load_error(tab_id, e.to_string());
                }
            }
            ctx.request_repaint();
        }

        // Drain completed stats computations
        while let Ok((tab_id, stats)) = self.stats_rx.try_recv() {
            if let Some(tab) = self.tabs.tabs.iter_mut().find(|t| t.id == tab_id) {
                tab.stats = Some(stats);
                tab.stats_computing = false;
            }
            ctx.request_repaint();
        }

        // Drain completed histogram computations
        while let Ok((tab_id, bins, edges)) = self.hist_rx.try_recv() {
            if let Some(tab) = self.tabs.tabs.iter_mut().find(|t| t.id == tab_id) {
                tab.hist_bins = Some(bins);
                tab.hist_edges = Some(edges);
                tab.hist_computing = false;
            }
            ctx.request_repaint();
        }

        // Drain completed tile loads
        {
            let completed = self.tile_loader.poll_completed();
            let mut any = false;
            for r in completed {
                if let Ok(tile) = r.result {
                    self.tile_manager.insert(r.key, tile);
                    any = true;
                }
            }
            if any {
                ctx.request_repaint();
            }
        }

        // Handle HDU reload requests
        let reload_requests: Vec<(u64, usize, PathBuf)> = self.tabs.tabs.iter()
            .filter(|t| t.needs_hdu_reload && !t.data.is_large())
            .map(|t| (t.id, t.hdu_index, t.path.clone()))
            .collect();
        for (tab_id, hdu_idx, path) in reload_requests {
            if let Some(tab) = self.tabs.tabs.iter_mut().find(|t| t.id == tab_id) {
                tab.needs_hdu_reload = false;
                tab.data = FileData::Loading;
                tab.scale_result = None;
                tab.histeq_lut = None;
                tab.stats = None;
                tab.stats_computing = false;
            }
            self.reload_hdu(tab_id, hdu_idx, path);
        }

        // Filesystem events
        let _fs_changes = self.file_explorer.poll_events();

        let do_fit = self.handle_keys(ctx);

        if let Some(cmd) = self.command_palette.show(ctx) {
            self.apply_command(cmd);
        }

        self.show_path_input_dialogs(ctx);

        if let Some(path) = self.file_explorer.show(ctx) {
            self.open_file(&path);
        }

        // Header panel (now takes &Tab for HDU selector)
        let mut hdu_change: Option<(u64, usize)> = None;
        if let Some(tab) = self.tabs.active_tab() {
            if let Some(new_hdu) = self.header_panel.show(ctx, tab) {
                hdu_change = Some((tab.id, new_hdu));
            }
        }
        if let Some((tab_id, new_hdu)) = hdu_change {
            if let Some(tab) = self.tabs.tabs.iter_mut().find(|t| t.id == tab_id) {
                tab.hdu_index = new_hdu;
                tab.needs_hdu_reload = true;
            }
        }

        // Contrast/bias sliders in a small bottom panel when header is hidden
        if !self.header_panel.visible {
            if let Some(tab_id) = self.tabs.active_id {
                egui::TopBottomPanel::bottom("display_controls")
                    .exact_height(30.0)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Contrast:");
                            let idx = self.tabs.tabs.iter().position(|t| t.id == tab_id);
                            if let Some(i) = idx {
                                let tab = &mut self.tabs.tabs[i];
                                if ui.add(egui::Slider::new(&mut tab.contrast, 0.05f32..=5.0)
                                    .step_by(0.05)
                                    .clamping(egui::SliderClamping::Always)
                                ).changed() {
                                    tab.needs_retexture = true;
                                    tab.tile_textures.clear();
                                }
                                ui.separator();
                                ui.label("Bias:");
                                if ui.add(egui::Slider::new(&mut tab.bias, 0.0f32..=1.0)
                                    .step_by(0.01)
                                    .clamping(egui::SliderClamping::Always)
                                ).changed() {
                                    tab.needs_retexture = true;
                                    tab.tile_textures.clear();
                                }
                            }
                        });
                    });
            }
        } else {
            // Sliders inside the header panel (render separately below the header)
            if let Some(tab_id) = self.tabs.active_id {
                egui::TopBottomPanel::bottom("display_controls_side")
                    .exact_height(30.0)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("C:");
                            let idx = self.tabs.tabs.iter().position(|t| t.id == tab_id);
                            if let Some(i) = idx {
                                let tab = &mut self.tabs.tabs[i];
                                if ui.add(egui::Slider::new(&mut tab.contrast, 0.05f32..=5.0)
                                    .step_by(0.05)
                                    .clamping(egui::SliderClamping::Always)
                                ).changed() {
                                    tab.needs_retexture = true;
                                    tab.tile_textures.clear();
                                }
                                ui.label("B:");
                                if ui.add(egui::Slider::new(&mut tab.bias, 0.0f32..=1.0)
                                    .step_by(0.01)
                                    .clamping(egui::SliderClamping::Always)
                                ).changed() {
                                    tab.needs_retexture = true;
                                    tab.tile_textures.clear();
                                }
                            }
                        });
                    });
            }
        }

        // Kick background stats computation when stats panel is open
        if let Some(tab) = self.tabs.active_tab_mut() {
            if self.stats_panel.visible && tab.stats.is_none() && !tab.stats_computing && !tab.data.is_loading() {
                let data = tab.data.pixel_data().to_vec();
                if !data.is_empty() {
                    tab.stats_computing = true;
                    let tx = self.stats_tx.clone();
                    let tab_id = tab.id;
                    std::thread::spawn(move || {
                        let s = compute_stats(&data);
                        let _ = tx.send((tab_id, Arc::new(s)));
                    });
                }
            }
        }

        // Kick background histogram computation when histogram panel is open
        if let Some(tab) = self.tabs.active_tab_mut() {
            if self.histogram_panel.visible
                && tab.hist_bins.is_none()
                && !tab.hist_computing
                && !tab.data.is_loading()
            {
                let data = tab.data.pixel_data().to_vec();
                if !data.is_empty() {
                    tab.hist_computing = true;
                    let tx = self.hist_tx.clone();
                    let tab_id = tab.id;
                    std::thread::spawn(move || {
                        let (bins, edges) = compute_histogram(&data, 256);
                        let _ = tx.send((tab_id, Arc::new(bins), Arc::new(edges)));
                    });
                }
            }
        }

        // Show histogram panel and apply any limit changes
        let hist_result = self.histogram_panel.show(ctx, self.tabs.active_tab());
        if let Some((vmin, vmax)) = hist_result {
            if let Some(tab) = self.tabs.active_tab_mut() {
                if vmin.is_nan() {
                    // NaN sentinel = reset overrides
                    tab.vmin_override = None;
                    tab.vmax_override = None;
                } else {
                    tab.vmin_override = Some(vmin);
                    tab.vmax_override = Some(vmax);
                }
                tab.needs_retexture = true;
                tab.tile_textures.clear();
            }
        }

        // Blink logic
        if self.blink_enabled && self.tabs.tabs.len() > 1 {
            let elapsed = self.blink_last_switch.elapsed().as_secs_f32();
            if elapsed >= self.blink_interval_secs {
                self.blink_tab_idx = (self.blink_tab_idx + 1) % self.tabs.tabs.len();
                let next_id = self.tabs.tabs[self.blink_tab_idx].id;
                self.tabs.active_id = Some(next_id);
                self.split_view.assign_active(next_id);
                self.blink_last_switch = Instant::now();
            }
            let remaining = (self.blink_interval_secs - self.blink_last_switch.elapsed().as_secs_f32()).max(0.016);
            ctx.request_repaint_after(Duration::from_secs_f32(remaining));
        }

        // Stats panel
        let (stats_opt, computing, img_w, img_h) = if let Some(tab) = self.tabs.active_tab() {
            (tab.stats.clone(), tab.stats_computing, tab.data.width(), tab.data.height())
        } else {
            (None, false, 0, 0)
        };
        self.stats_panel.show(ctx, stats_opt.as_deref(), computing, img_w, img_h);

        let tile_status = if self
            .tabs
            .active_tab()
            .map(|t| t.data.is_large())
            .unwrap_or(false)
        {
            let pending = self.tile_loader.pending;
            let cache_mb = self.tile_manager.memory_used() / (1024 * 1024);
            Some((pending, cache_mb))
        } else {
            None
        };

        let blink_info = if self.blink_enabled { Some(self.blink_interval_secs) } else { None };
        StatusBar::show(ctx, self.tabs.active_tab(), self.cursor_image_pos, tile_status, blink_info, self.split_view.linked);
        self.tabs.show_tab_bar(ctx);

        // Show cube panel (bottom panel) before CentralPanel for the active cube tab
        let mut cube_z_change: Option<(u64, usize)> = None;
        if let Some(tab) = self.tabs.active_tab_mut() {
            if tab.data.is_cube() {
                if let Some(ref mut panel) = tab.cube_panel {
                    let depth = if let FileData::Cube(ref c) = tab.data { c.depth } else { 1 };
                    let spec_axis = if let FileData::Cube(ref c) = tab.data {
                        c.spectral_axis.clone()
                    } else {
                        None
                    };
                    if let Some(new_z) = panel.show(ctx, depth, spec_axis.as_ref()) {
                        cube_z_change = Some((tab.id, new_z));
                    }
                }

                // Show plot panel if it exists
                if let Some(ref mut plot) = tab.plot_panel {
                    plot.show(ctx);
                }
            }
        }
        if let Some((tab_id, new_z)) = cube_z_change {
            if let Some(tab) = self.tabs.tabs.iter_mut().find(|t| t.id == tab_id) {
                tab.cube_z = new_z;
                tab.needs_retexture = true;
                tab.texture = None;
            }
        }

        let mut missing_tiles: Vec<(u64, u8, usize, usize)> = Vec::new();
        let mut new_cursor_pos = None;

        egui::CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_rect_before_wrap();

            match self.split_view.layout {
                SplitLayout::Single => {
                    let pane_tab_id = self.split_view.pane_tab_ids.first().copied().flatten();
                    let tab_id = pane_tab_id.or(self.tabs.active_id);

                    if let Some(id) = tab_id {
                        if let Some(idx) = self.tabs.tabs.iter().position(|t| t.id == id) {
                            let tab = &mut self.tabs.tabs[idx];
                            if tab.data.is_loading() {
                                FitsViewApp::render_loading_pane(tab, ui);
                            } else if tab.data.is_large() {
                                new_cursor_pos = FitsViewApp::render_large_pane(
                                    tab, ui, ctx, do_fit,
                                    &mut self.tile_manager,
                                    self.renderer.as_ref(),
                                    &mut missing_tiles,
                                );
                            } else if tab.data.is_cube() {
                                ensure_cube_texture(tab, ctx);
                                new_cursor_pos = FitsViewApp::render_pane(tab, ui, ctx, do_fit);
                            } else {
                                new_cursor_pos = FitsViewApp::render_pane(tab, ui, ctx, do_fit);
                            }
                        }
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.heading("No file loaded.\nOpen a FITS file or directory.");
                        });
                    }
                }
                SplitLayout::SideBySide => {
                    let half_w = available.width() / 2.0;
                    let left_rect = Rect::from_min_size(
                        available.min,
                        vec2(half_w - 1.0, available.height()),
                    );
                    let right_rect = Rect::from_min_size(
                        available.min + vec2(half_w + 1.0, 0.0),
                        vec2(half_w - 1.0, available.height()),
                    );

                    ui.painter().line_segment(
                        [available.min + vec2(half_w, 0.0), available.min + vec2(half_w, available.height())],
                        egui::Stroke::new(2.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
                    );

                    let pane_ids: Vec<Option<u64>> = self.split_view.pane_tab_ids.clone();

                    for (pane_idx, (rect, tab_id)) in
                        [left_rect, right_rect].iter().zip(pane_ids.iter()).enumerate()
                    {
                        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(*rect));
                        if let Some(id) = tab_id {
                            if let Some(idx) = self.tabs.tabs.iter().position(|t| t.id == *id) {
                                let tab = &mut self.tabs.tabs[idx];
                                let pos = if tab.data.is_loading() {
                                    FitsViewApp::render_loading_pane(tab, &mut child);
                                    None
                                } else if tab.data.is_large() {
                                    FitsViewApp::render_large_pane(tab, &mut child, ctx, do_fit, &mut self.tile_manager, self.renderer.as_ref(), &mut missing_tiles)
                                } else if tab.data.is_cube() {
                                    ensure_cube_texture(tab, ctx);
                                    FitsViewApp::render_pane(tab, &mut child, ctx, do_fit)
                                } else {
                                    FitsViewApp::render_pane(tab, &mut child, ctx, do_fit)
                                };
                                if pane_idx == self.split_view.active_pane {
                                    new_cursor_pos = pos;
                                }
                            }
                        } else {
                            child.centered_and_justified(|ui| { ui.weak("Empty pane"); });
                        }
                    }
                }
                SplitLayout::Grid2x2 => {
                    let half_w = available.width() / 2.0;
                    let half_h = available.height() / 2.0;
                    let rects = [
                        Rect::from_min_size(available.min, vec2(half_w - 1.0, half_h - 1.0)),
                        Rect::from_min_size(available.min + vec2(half_w + 1.0, 0.0), vec2(half_w - 1.0, half_h - 1.0)),
                        Rect::from_min_size(available.min + vec2(0.0, half_h + 1.0), vec2(half_w - 1.0, half_h - 1.0)),
                        Rect::from_min_size(available.min + vec2(half_w + 1.0, half_h + 1.0), vec2(half_w - 1.0, half_h - 1.0)),
                    ];

                    ui.painter().line_segment(
                        [available.min + vec2(half_w, 0.0), available.min + vec2(half_w, available.height())],
                        egui::Stroke::new(2.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
                    );
                    ui.painter().line_segment(
                        [available.min + vec2(0.0, half_h), available.min + vec2(available.width(), half_h)],
                        egui::Stroke::new(2.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
                    );

                    let pane_ids: Vec<Option<u64>> = self.split_view.pane_tab_ids.clone();

                    for (pane_idx, (rect, tab_id)) in rects.iter().zip(pane_ids.iter()).enumerate() {
                        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(*rect));
                        if let Some(id) = tab_id {
                            if let Some(idx) = self.tabs.tabs.iter().position(|t| t.id == *id) {
                                let tab = &mut self.tabs.tabs[idx];
                                let pos = if tab.data.is_loading() {
                                    FitsViewApp::render_loading_pane(tab, &mut child);
                                    None
                                } else if tab.data.is_large() {
                                    FitsViewApp::render_large_pane(tab, &mut child, ctx, do_fit, &mut self.tile_manager, self.renderer.as_ref(), &mut missing_tiles)
                                } else if tab.data.is_cube() {
                                    ensure_cube_texture(tab, ctx);
                                    FitsViewApp::render_pane(tab, &mut child, ctx, do_fit)
                                } else {
                                    FitsViewApp::render_pane(tab, &mut child, ctx, do_fit)
                                };
                                if pane_idx == self.split_view.active_pane {
                                    new_cursor_pos = pos;
                                }
                            }
                        } else {
                            child.centered_and_justified(|ui| { ui.weak("Empty pane"); });
                        }
                    }
                }
            }
        });

        self.cursor_image_pos = new_cursor_pos;

        // Annotation interaction for active tab (after CentralPanel so painter is flushed)
        // We use a separate egui::Area to handle annotation input on the active pane
        {
            use crate::annotation::AnnotationMode;
            let is_placing = !matches!(self.annotation_overlay.mode, AnnotationMode::Disabled);
            if is_placing {
                if let Some(id) = self.tabs.active_id {
                    if let Some(tab) = self.tabs.tabs.iter_mut().find(|t| t.id == id) {
                        // In placing mode, the annotation_overlay tracks drag state.
                        // We use ctx.input to check for pointer events globally.
                        let ptr = ctx.input(|i| (i.pointer.primary_down(), i.pointer.interact_pos(), i.pointer.primary_released()));
                        let _ = (ptr, tab); // placeholder — full interaction handled in render_pane via painter
                    }
                }
            }
        }

        // Linked pan/zoom: propagate active pane's view to other panes
        if self.split_view.linked {
            sync_linked_views(&mut self.tabs.tabs, &self.split_view);
        }

        // Submit tile requests after render
        for (file_id, lod, tx, ty) in missing_tiles {
            let source = self.tabs.tabs.iter().find_map(|t| {
                if let FileData::LargeImage(ref lis) = t.data {
                    if lis.file_id == file_id {
                        return Some(lis.source.clone());
                    }
                }
                None
            });
            if let Some(src) = source {
                self.tile_loader.request(TileRequest {
                    key: TileKey { file_id, hdu: 0, zoom_level: lod, tx, ty },
                    source: src,
                });
            }
        }
    }
}

/// Propagate the active pane's ViewState to all other visible panes.
/// If both panes have WCS, sync by sky position; otherwise sync by pixel offset/zoom directly.
fn sync_linked_views(tabs: &mut [Tab], split_view: &SplitView) {
    // Find the active pane's tab id and view
    let active_pane_id = split_view
        .pane_tab_ids
        .get(split_view.active_pane)
        .and_then(|id| *id);

    let Some(active_id) = active_pane_id else { return };

    // Collect the active tab's view and WCS center
    let (active_view, active_wcs_center) = {
        let Some(active_tab) = tabs.iter().find(|t| t.id == active_id) else { return };
        let view = active_tab.view.clone();
        let center = if let Some(wcs) = &active_tab.wcs {
            let cx = active_tab.data.width() as f32 / 2.0;
            let cy = active_tab.data.height() as f32 / 2.0;
            // Center in image coords
            let img_cx = cx / view.zoom - view.offset.x;
            let img_cy = cy / view.zoom - view.offset.y;
            wcs.pixel_to_world(img_cx as f64, img_cy as f64)
        } else {
            None
        };
        (view, center)
    };

    // Apply to all other pane tabs
    for (pane_idx, pane_id_opt) in split_view.pane_tab_ids.iter().enumerate() {
        if pane_idx == split_view.active_pane {
            continue;
        }
        let Some(pane_id) = pane_id_opt else { continue };
        let Some(tab) = tabs.iter_mut().find(|t| t.id == *pane_id) else { continue };

        if let (Some((ra, dec)), Some(ref wcs)) = (active_wcs_center, &tab.wcs) {
            // WCS-aware sync: find where the sky center lands in this tab
            if let Some((px, py)) = wcs.world_to_pixel(ra, dec) {
                let cx = tab.data.width() as f32 / 2.0;
                let cy = tab.data.height() as f32 / 2.0;
                tab.view.zoom = active_view.zoom;
                tab.view.offset = egui::vec2(
                    cx / tab.view.zoom - px as f32,
                    cy / tab.view.zoom - py as f32,
                );
            }
        } else {
            // Direct copy
            tab.view = active_view.clone();
        }
    }
}

fn load_file_full(path: &Path) -> anyhow::Result<LoadResult> {
    let file_size = std::fs::metadata(path)?.len();
    let hdu_list = list_hdus(path).unwrap_or_default();

    if file_size > LARGE_FILE_THRESHOLD {
        let mmap = MmapFitsImage::open(path, 0)?;
        let wcs = Wcs::from_header(&mmap.header);
        return Ok(LoadResult {
            data: FileData::LargeImage(LargeImageState { source: Arc::new(mmap), file_id: 0 }),
            hdu_list,
            wcs,
        });
    }

    // Check for 3D+ data cube (NAXIS >= 3)
    let is_cube = hdu_list.first().map(|h| {
        matches!(h.hdu_type, HduType::Image) && h.naxis.len() >= 3 && h.naxis.get(2).copied().unwrap_or(0) > 1
    }).unwrap_or(false);

    if is_cube && file_size <= LARGE_FILE_THRESHOLD {
        match load_cube(path, 0) {
            Ok(cube) => {
                let wcs = cube.wcs.clone();
                return Ok(LoadResult { data: FileData::Cube(Arc::new(cube)), hdu_list, wcs });
            }
            Err(e) => {
                log::warn!("Failed to load as cube, falling back to 2D: {e}");
            }
        }
    }

    match load_fits(path) {
        Ok(img) => {
            let wcs = Wcs::from_header(&img.header);
            Ok(LoadResult { data: FileData::Image(img), hdu_list, wcs })
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("BINTABLE") || msg.contains("TABLE") {
                let evt = bin_events(path, 0, 1.0)?;
                Ok(LoadResult { data: FileData::Event(evt), hdu_list, wcs: None })
            } else {
                Err(e)
            }
        }
    }
}

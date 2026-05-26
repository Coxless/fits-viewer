use std::path::{Path, PathBuf};

use egui::{vec2, Color32, ColorImage, Rect, TextureHandle, TextureOptions, ViewportCommand};
use fitsview_core::{
    colormap::render_to_rgba,
    event_image::bin_events,
    fits_reader::load_fits,
    scale::compute_scale,
};

use crate::{
    command_palette::{Command, CommandPalette},
    file_explorer::FileExplorer,
    header_panel::HeaderPanel,
    split_view::{SplitLayout, SplitView},
    status_bar::StatusBar,
    tab_manager::{FileData, Tab, TabManager},
    viewport::ViewState,
};

pub struct FitsViewApp {
    tabs: TabManager,
    file_explorer: FileExplorer,
    header_panel: HeaderPanel,
    split_view: SplitView,
    command_palette: CommandPalette,
    /// Pending path input for "Open File" via command palette
    open_path_input: Option<String>,
    /// Pending path input for "Open Directory" via command palette
    open_dir_input: Option<String>,
    /// Chord state: was Ctrl+K pressed last frame?
    last_key_ctrl_k: bool,
    /// Image-space cursor position for status bar (updated each frame)
    cursor_image_pos: Option<egui::Pos2>,
}

impl FitsViewApp {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        initial_paths: Vec<PathBuf>,
        initial_dir: Option<PathBuf>,
    ) -> Self {
        let mut app = Self {
            tabs: TabManager::default(),
            file_explorer: FileExplorer::default(),
            header_panel: HeaderPanel::default(),
            split_view: SplitView::default(),
            command_palette: CommandPalette::default(),
            open_path_input: None,
            open_dir_input: None,
            last_key_ctrl_k: false,
            cursor_image_pos: None,
        };

        // Open initial directory in explorer
        if let Some(dir) = initial_dir {
            app.file_explorer.set_root(dir);
        }

        // Open initial files as tabs
        for path in initial_paths {
            app.open_file(&path);
        }

        app
    }

    /// Load a FITS file (image or event list) and open it as a new tab.
    fn open_file(&mut self, path: &Path) {
        match load_file(path) {
            Ok(data) => {
                let tab_id = {
                    self.tabs.open(path.to_path_buf(), data);
                    // get the new tab's id
                    self.tabs.active_id.unwrap()
                };
                self.split_view.assign_active(tab_id);
            }
            Err(e) => {
                log::error!("Failed to open {}: {e}", path.display());
            }
        }
    }

    fn handle_keys(&mut self, ctx: &egui::Context) -> bool {
        let mut do_fit = false;

        ctx.input_mut(|i| {
            // Ctrl+H — toggle header panel
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::H) {
                self.header_panel.visible = !self.header_panel.visible;
            }
            // Ctrl+B — toggle file explorer
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::B) {
                self.file_explorer.visible = !self.file_explorer.visible;
            }
            // Ctrl+Shift+P — command palette
            if i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::P) {
                self.command_palette.open();
            }
            // Ctrl+0 — fit to window
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::Num0) {
                do_fit = true;
            }
            // Ctrl++ / Ctrl+= — zoom in
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::Plus)
                || i.consume_key(egui::Modifiers::CTRL, egui::Key::Equals)
            {
                if let Some(tab) = self.tabs.active_tab_mut() {
                    tab.view.zoom =
                        (tab.view.zoom * ViewState::ZOOM_STEP).clamp(ViewState::ZOOM_MIN, ViewState::ZOOM_MAX);
                }
            }
            // Ctrl+- — zoom out
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::Minus) {
                if let Some(tab) = self.tabs.active_tab_mut() {
                    tab.view.zoom =
                        (tab.view.zoom / ViewState::ZOOM_STEP).clamp(ViewState::ZOOM_MIN, ViewState::ZOOM_MAX);
                }
            }
            // Ctrl+W — close tab
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::W) {
                if let Some(id) = self.tabs.active_id {
                    self.split_view.remove_tab(id);
                }
                self.tabs.close_active();
                // Assign next active tab to split view
                if let Some(id) = self.tabs.active_id {
                    self.split_view.fill_empty(id);
                }
            }
            // Ctrl+Tab / Ctrl+Shift+Tab
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

            // Ctrl+\ — vertical split
            let ctrl_backslash = i.consume_key(egui::Modifiers::CTRL, egui::Key::Backslash);
            if ctrl_backslash && !self.last_key_ctrl_k {
                self.split_view.set_layout(SplitLayout::SideBySide);
            }
            // Ctrl+K chord tracking
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::K) {
                self.last_key_ctrl_k = true;
            } else if self.last_key_ctrl_k {
                // Ctrl+K Ctrl+\ — horizontal split
                if ctrl_backslash {
                    self.split_view.set_layout(SplitLayout::Grid2x2);
                }
                self.last_key_ctrl_k = false;
            }

            // [ / ] — HDU navigation
            if i.consume_key(egui::Modifiers::NONE, egui::Key::OpenBracket) {
                if let Some(tab) = self.tabs.active_tab_mut() {
                    if tab.hdu_index > 0 {
                        tab.hdu_index -= 1;
                        tab.needs_retexture = true;
                    }
                }
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::CloseBracket) {
                if let Some(tab) = self.tabs.active_tab_mut() {
                    tab.hdu_index += 1;
                    tab.needs_retexture = true;
                }
            }
        });

        do_fit
    }

    fn apply_command(&mut self, cmd: Command) {
        match cmd {
            Command::OpenFile => {
                self.open_path_input = Some(String::new());
            }
            Command::OpenDirectory => {
                self.open_dir_input = Some(String::new());
            }
            Command::SetColormap(cmap) => {
                if let Some(tab) = self.tabs.active_tab_mut() {
                    tab.colormap = cmap;
                    tab.needs_retexture = true;
                }
            }
            Command::SetScale(mode) => {
                if let Some(tab) = self.tabs.active_tab_mut() {
                    tab.scale_mode = mode;
                    tab.scale_result = None;
                    tab.needs_retexture = true;
                }
            }
            Command::ToggleSidebar => {
                self.file_explorer.visible = !self.file_explorer.visible;
            }
            Command::ToggleHeaderPanel => {
                self.header_panel.visible = !self.header_panel.visible;
            }
            Command::SplitVertical => {
                self.split_view.set_layout(SplitLayout::SideBySide);
            }
            Command::SplitHorizontal => {
                self.split_view.set_layout(SplitLayout::Grid2x2);
            }
            Command::FitToWindow => {
                if let Some(tab) = self.tabs.active_tab_mut() {
                    tab.needs_fit = true;
                }
            }
        }
    }

    fn show_path_input_dialogs(&mut self, ctx: &egui::Context) {
        // Simple path-input overlay for "Open File"
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

        // Simple path-input overlay for "Open Directory"
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
    }

    fn render_pane(
        tab: &mut Tab,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        do_fit: bool,
    ) -> Option<egui::Pos2> {
        let available = ui.available_rect_before_wrap();

        // Build or rebuild texture
        if tab.texture.is_none() || tab.needs_retexture {
            tab.texture = Some(build_texture(ctx, tab));
            tab.needs_retexture = false;
        }

        // Initial fit
        if tab.needs_fit || do_fit {
            tab.needs_fit = false;
            let img_size = vec2(tab.data.width() as f32, tab.data.height() as f32);
            tab.view.fit_to_rect(available, img_size);
        }

        let response = ui.allocate_rect(available, egui::Sense::drag());

        // Mouse wheel zoom
        let scroll_delta = ctx.input(|i| i.smooth_scroll_delta.y);
        if response.hovered() && scroll_delta.abs() > 0.1 {
            let factor = (scroll_delta / 50.0).exp();
            let cursor_pos = ctx
                .input(|i| i.pointer.hover_pos())
                .unwrap_or(available.center());
            let cursor_rel = cursor_pos.to_vec2() - available.min.to_vec2();
            tab.view.zoom_toward(factor, cursor_rel);
        }

        // Mouse drag pan
        if response.dragged() {
            tab.view.pan(response.drag_delta());
        }

        // Compute cursor image position for status bar
        let cursor_image_pos = ctx.input(|i| i.pointer.hover_pos()).and_then(|screen_pos| {
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

        // Draw image
        if let Some(ref texture) = &tab.texture.clone() {
            let img_size = vec2(tab.data.width() as f32, tab.data.height() as f32);
            let display_size = img_size * tab.view.zoom;
            let display_origin = available.min.to_vec2() + tab.view.offset * tab.view.zoom;
            let display_rect = Rect::from_min_size(display_origin.to_pos2(), display_size);
            let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            ui.painter().image(texture.id(), display_rect, uv, Color32::WHITE);
        }

        cursor_image_pos
    }
}

fn build_texture(ctx: &egui::Context, tab: &mut Tab) -> TextureHandle {
    let data = tab.data.pixel_data();
    if tab.scale_result.is_none() {
        tab.scale_result = Some(compute_scale(data, tab.scale_mode));
    }
    let sr = tab.scale_result.as_ref().unwrap();
    let rgba = render_to_rgba(data, sr.vmin, sr.vmax, tab.colormap);
    let w = tab.data.width();
    let h = tab.data.height();
    let ci = ColorImage::from_rgba_unmultiplied([w, h], &rgba);
    ctx.load_texture(format!("fits-tab-{}", tab.id), ci, TextureOptions::LINEAR)
}

impl eframe::App for FitsViewApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Update window title
        let title = self
            .tabs
            .active_tab()
            .map(|t| format!("fits-view — {}", t.title()))
            .unwrap_or_else(|| "fits-view".to_owned());
        ctx.send_viewport_cmd(ViewportCommand::Title(title));

        // Poll filesystem events
        let fs_changes = self.file_explorer.poll_events();
        for _ in fs_changes {
            // Explorer already updated its internal list; just repaint
        }

        // Handle keyboard shortcuts
        let do_fit = self.handle_keys(ctx);

        // Show command palette
        if let Some(cmd) = self.command_palette.show(ctx) {
            self.apply_command(cmd);
        }

        // Show file-open dialogs
        self.show_path_input_dialogs(ctx);

        // File explorer (left panel)
        if let Some(path) = self.file_explorer.show(ctx) {
            self.open_file(&path);
        }

        // Header panel (right panel) — based on active tab
        if let Some(tab) = self.tabs.active_tab() {
            self.header_panel.show(ctx, tab.data.header());
        }

        // Status bar (bottom panel)
        StatusBar::show(ctx, self.tabs.active_tab(), self.cursor_image_pos);

        // Tab bar (top panel, below any menu bar)
        self.tabs.show_tab_bar(ctx);

        // Central panel — split view rendering
        egui::CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_rect_before_wrap();
            let mut new_cursor_pos = None;

            match self.split_view.layout {
                crate::split_view::SplitLayout::Single => {
                    let pane_tab_id = self.split_view.pane_tab_ids.first().copied().flatten();
                    let active_id = self.tabs.active_id;
                    let tab_id = pane_tab_id.or(active_id);

                    if let Some(id) = tab_id {
                        if let Some(tab) = self.tabs.tabs.iter_mut().find(|t| t.id == id) {
                            new_cursor_pos =
                                FitsViewApp::render_pane(tab, ui, ctx, do_fit);
                        }
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.heading("No file loaded.\nOpen a FITS file or directory.");
                        });
                    }
                }
                crate::split_view::SplitLayout::SideBySide => {
                    let half_w = available.width() / 2.0;
                    let left_rect =
                        Rect::from_min_size(available.min, vec2(half_w - 1.0, available.height()));
                    let right_rect = Rect::from_min_size(
                        available.min + vec2(half_w + 1.0, 0.0),
                        vec2(half_w - 1.0, available.height()),
                    );

                    // Draw divider
                    ui.painter().line_segment(
                        [
                            available.min + vec2(half_w, 0.0),
                            available.min + vec2(half_w, available.height()),
                        ],
                        egui::Stroke::new(2.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
                    );

                    let pane_ids: Vec<Option<u64>> = self.split_view.pane_tab_ids.clone();

                    for (pane_idx, (rect, tab_id)) in
                        [left_rect, right_rect].iter().zip(pane_ids.iter()).enumerate()
                    {
                        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(*rect));
                        if let Some(id) = tab_id {
                            if let Some(tab) = self.tabs.tabs.iter_mut().find(|t| t.id == *id) {
                                let pos = FitsViewApp::render_pane(tab, &mut child, ctx, do_fit);
                                if pane_idx == self.split_view.active_pane {
                                    new_cursor_pos = pos;
                                }
                            }
                        } else {
                            child.centered_and_justified(|ui| {
                                ui.weak("Empty pane");
                            });
                        }
                    }
                }
                crate::split_view::SplitLayout::Grid2x2 => {
                    let half_w = available.width() / 2.0;
                    let half_h = available.height() / 2.0;
                    let rects = [
                        Rect::from_min_size(available.min, vec2(half_w - 1.0, half_h - 1.0)),
                        Rect::from_min_size(
                            available.min + vec2(half_w + 1.0, 0.0),
                            vec2(half_w - 1.0, half_h - 1.0),
                        ),
                        Rect::from_min_size(
                            available.min + vec2(0.0, half_h + 1.0),
                            vec2(half_w - 1.0, half_h - 1.0),
                        ),
                        Rect::from_min_size(
                            available.min + vec2(half_w + 1.0, half_h + 1.0),
                            vec2(half_w - 1.0, half_h - 1.0),
                        ),
                    ];

                    // Dividers
                    ui.painter().line_segment(
                        [
                            available.min + vec2(half_w, 0.0),
                            available.min + vec2(half_w, available.height()),
                        ],
                        egui::Stroke::new(2.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
                    );
                    ui.painter().line_segment(
                        [
                            available.min + vec2(0.0, half_h),
                            available.min + vec2(available.width(), half_h),
                        ],
                        egui::Stroke::new(2.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
                    );

                    let pane_ids: Vec<Option<u64>> = self.split_view.pane_tab_ids.clone();

                    for (pane_idx, (rect, tab_id)) in rects.iter().zip(pane_ids.iter()).enumerate() {
                        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(*rect));
                        if let Some(id) = tab_id {
                            if let Some(tab) = self.tabs.tabs.iter_mut().find(|t| t.id == *id) {
                                let pos = FitsViewApp::render_pane(tab, &mut child, ctx, do_fit);
                                if pane_idx == self.split_view.active_pane {
                                    new_cursor_pos = pos;
                                }
                            }
                        } else {
                            child.centered_and_justified(|ui| {
                                ui.weak("Empty pane");
                            });
                        }
                    }
                }
            }

            self.cursor_image_pos = new_cursor_pos;
        });
    }
}

/// Dispatch: try image load first, fall back to event-list binning.
fn load_file(path: &Path) -> anyhow::Result<FileData> {
    match load_fits(path) {
        Ok(img) => Ok(FileData::Image(img)),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("BINTABLE") || msg.contains("TABLE") {
                // Try event-list binning on the first BINTABLE (index 0)
                let evt = bin_events(path, 0, 1.0)?;
                Ok(FileData::Event(evt))
            } else {
                Err(e)
            }
        }
    }
}

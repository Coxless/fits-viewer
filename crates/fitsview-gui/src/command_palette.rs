use crate::{annotation::ShapeType, theme};
use fitsview_core::{colormap::Colormap, scale::ScaleMode};

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    OpenFile,
    OpenDirectory,
    SetColormap(Colormap),
    SetScale(ScaleMode),
    ToggleSidebar,
    ToggleHeaderPanel,
    SplitVertical,
    SplitHorizontal,
    FitToWindow,
    ToggleStats,
    LoadRegionFile,
    ToggleBlink,
    ResetContrastBias,
    ToggleCrosshair,
    ToggleHistogram,
    ToggleWcsGrid,
    ToggleLinkPanes,
    SaveSession,
    OpenSession,
    AnnotationMode(ShapeType),
    // ── Step 8 ──
    RgbComposite,
    AddContourOverlay,
    QuerySimbad,
    QueryVizier,
    QueryGaiaDr3,
    LoadLocalVotable,
    // ── Step 9 ──
    LineProfileTool,
    PhotometryMode,
    ImageArithmetic,
    LoadMask,
    // ── Step 10 ──
    SampConnect,
    SampDisconnect,
    ToggleScriptConsole,
}

impl Command {
    pub fn label(&self) -> &'static str {
        match self {
            Command::OpenFile => "Open File...",
            Command::OpenDirectory => "Open Directory...",
            Command::SetColormap(Colormap::Gray)    => "Set Colormap: Gray",
            Command::SetColormap(Colormap::Viridis) => "Set Colormap: Viridis",
            Command::SetColormap(Colormap::Plasma)  => "Set Colormap: Plasma",
            Command::SetColormap(Colormap::Inferno) => "Set Colormap: Inferno",
            Command::SetColormap(Colormap::Hot)     => "Set Colormap: Hot",
            Command::SetColormap(Colormap::Rainbow) => "Set Colormap: Rainbow",
            Command::SetScale(ScaleMode::ZScale)  => "Set Scale: ZScale",
            Command::SetScale(ScaleMode::Linear)  => "Set Scale: Linear",
            Command::SetScale(ScaleMode::Log)     => "Set Scale: Log",
            Command::SetScale(ScaleMode::Sqrt)    => "Set Scale: Sqrt",
            Command::SetScale(ScaleMode::Asinh)   => "Set Scale: ASinh",
            Command::SetScale(ScaleMode::MinMax)  => "Set Scale: MinMax",
            Command::SetScale(ScaleMode::HistEq)  => "Set Scale: HistEq",
            Command::ToggleSidebar      => "Toggle Sidebar",
            Command::ToggleHeaderPanel  => "Toggle Header Panel",
            Command::SplitVertical      => "Split View: Vertical",
            Command::SplitHorizontal    => "Split View: Horizontal",
            Command::FitToWindow        => "Fit to Window",
            Command::ToggleStats        => "Toggle Statistics Panel (Ctrl+I)",
            Command::LoadRegionFile     => "Load Region File... (Ctrl+R)",
            Command::ToggleBlink        => "Toggle Blink (Ctrl+L)",
            Command::ResetContrastBias  => "Reset Contrast/Bias",
            Command::ToggleCrosshair    => "Toggle Crosshair (Ctrl+X)",
            Command::ToggleHistogram    => "Toggle Histogram Panel",
            Command::ToggleWcsGrid      => "Toggle WCS Grid",
            Command::ToggleLinkPanes    => "Toggle Linked Pan/Zoom",
            Command::SaveSession        => "Save Session",
            Command::OpenSession        => "Open Session...",
            Command::AnnotationMode(ShapeType::Circle) => "Annotate: Draw Circle",
            Command::AnnotationMode(ShapeType::Box)    => "Annotate: Draw Box",
            Command::AnnotationMode(ShapeType::Line)   => "Annotate: Draw Line",
            Command::AnnotationMode(ShapeType::Text)   => "Annotate: Place Text",
            Command::RgbComposite       => "RGB Composite...",
            Command::AddContourOverlay  => "Add Contour Overlay...",
            Command::QuerySimbad        => "Query SIMBAD Catalog",
            Command::QueryVizier        => "Query VizieR Catalog...",
            Command::QueryGaiaDr3       => "Query Gaia DR3",
            Command::LoadLocalVotable   => "Load Local VOTable...",
            Command::LineProfileTool    => "Line Profile Tool",
            Command::PhotometryMode     => "Aperture Photometry Mode",
            Command::ImageArithmetic    => "Image Arithmetic...",
            Command::LoadMask           => "Load Mask File...",
            Command::SampConnect        => "SAMP: Connect to Hub",
            Command::SampDisconnect     => "SAMP: Disconnect",
            Command::ToggleScriptConsole => "Toggle Script Console (Ctrl+Shift+C)",
        }
    }
}

const ALL_COMMANDS: &[Command] = &[
    Command::OpenFile,
    Command::OpenDirectory,
    Command::SetColormap(Colormap::Gray),
    Command::SetColormap(Colormap::Viridis),
    Command::SetColormap(Colormap::Plasma),
    Command::SetColormap(Colormap::Inferno),
    Command::SetColormap(Colormap::Hot),
    Command::SetColormap(Colormap::Rainbow),
    Command::SetScale(ScaleMode::ZScale),
    Command::SetScale(ScaleMode::Linear),
    Command::SetScale(ScaleMode::Log),
    Command::SetScale(ScaleMode::Sqrt),
    Command::SetScale(ScaleMode::Asinh),
    Command::SetScale(ScaleMode::MinMax),
    Command::SetScale(ScaleMode::HistEq),
    Command::ToggleSidebar,
    Command::ToggleHeaderPanel,
    Command::SplitVertical,
    Command::SplitHorizontal,
    Command::FitToWindow,
    Command::ToggleStats,
    Command::LoadRegionFile,
    Command::ToggleBlink,
    Command::ResetContrastBias,
    Command::ToggleCrosshair,
    Command::ToggleHistogram,
    Command::ToggleWcsGrid,
    Command::ToggleLinkPanes,
    Command::SaveSession,
    Command::OpenSession,
    Command::AnnotationMode(ShapeType::Circle),
    Command::AnnotationMode(ShapeType::Box),
    Command::AnnotationMode(ShapeType::Line),
    Command::AnnotationMode(ShapeType::Text),
    Command::RgbComposite,
    Command::AddContourOverlay,
    Command::QuerySimbad,
    Command::QueryVizier,
    Command::QueryGaiaDr3,
    Command::LoadLocalVotable,
    Command::LineProfileTool,
    Command::PhotometryMode,
    Command::ImageArithmetic,
    Command::LoadMask,
    Command::SampConnect,
    Command::SampDisconnect,
    Command::ToggleScriptConsole,
];

#[derive(Default)]
pub struct CommandPalette {
    pub visible: bool,
    query: String,
    selected: usize,
    just_opened: bool,
}

impl CommandPalette {
    pub fn open(&mut self) {
        self.visible = true;
        self.query.clear();
        self.selected = 0;
        self.just_opened = true;
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<Command> {
        if !self.visible {
            return None;
        }

        let mut executed = None;
        let mut close = false;

        egui::Window::new("Command Palette")
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 72.0))
            .fixed_size(egui::vec2(520.0, 340.0))
            .frame(theme::modal_frame())
            .show(ctx, |ui| {
                let input_resp = ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text("Search commands…")
                        .desired_width(f32::INFINITY)
                        .font(egui::FontId::new(15.0, egui::FontFamily::Proportional)),
                );

                if self.just_opened {
                    input_resp.request_focus();
                    self.just_opened = false;
                }

                let ir = input_resp.rect;
                ui.painter().line_segment(
                    [ir.left_bottom(), ir.right_bottom()],
                    egui::Stroke::new(2.0, theme::ACCENT),
                );

                if input_resp.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                    close = true;
                }
                ctx.input(|i| { if i.key_pressed(egui::Key::Escape) { close = true; } });

                ui.add_space(6.0);

                let query_lower = self.query.to_ascii_lowercase();
                let filtered: Vec<&Command> = ALL_COMMANDS
                    .iter()
                    .filter(|c| {
                        query_lower.is_empty()
                            || c.label().to_ascii_lowercase().contains(&query_lower)
                    })
                    .collect();

                self.selected = self.selected.min(filtered.len().saturating_sub(1));

                ctx.input(|i| {
                    if i.key_pressed(egui::Key::ArrowDown) && !filtered.is_empty() {
                        self.selected = (self.selected + 1) % filtered.len();
                    }
                    if i.key_pressed(egui::Key::ArrowUp) && !filtered.is_empty() {
                        self.selected = if self.selected == 0 { filtered.len() - 1 } else { self.selected - 1 };
                    }
                    if i.key_pressed(egui::Key::Enter) {
                        if let Some(&cmd) = filtered.get(self.selected) {
                            executed = Some(cmd.clone());
                        }
                    }
                });

                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    for (i, &cmd) in filtered.iter().enumerate() {
                        let is_selected = i == self.selected;
                        let bg = if is_selected { theme::BG_HOVER } else { egui::Color32::TRANSPARENT };
                        let text_color = if is_selected { theme::TEXT_PRIMARY } else { theme::TEXT_OVERLAY };
                        let label = egui::RichText::new(cmd.label()).size(13.5).color(text_color);
                        let resp = egui::Frame::none()
                            .fill(bg)
                            .rounding(egui::Rounding::same(4.0))
                            .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.add(egui::Label::new(label).sense(egui::Sense::click()))
                            })
                            .inner;
                        if resp.hovered() { self.selected = i; }
                        if resp.clicked() { executed = Some(cmd.clone()); }
                        if is_selected { resp.scroll_to_me(None); }
                    }
                });
            });

        if close || executed.is_some() {
            self.visible = false;
        }

        executed
    }
}

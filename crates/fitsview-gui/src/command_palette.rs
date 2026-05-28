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

    /// Render the command palette overlay. Returns the selected command if executed.
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
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 60.0))
            .fixed_size(egui::vec2(480.0, 320.0))
            .show(ctx, |ui| {
                let input_resp = ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text("Type a command...")
                        .desired_width(f32::INFINITY),
                );

                if self.just_opened {
                    input_resp.request_focus();
                    self.just_opened = false;
                }

                if input_resp.lost_focus()
                    && ctx.input(|i| i.key_pressed(egui::Key::Escape))
                {
                    close = true;
                }

                ctx.input(|i| {
                    if i.key_pressed(egui::Key::Escape) {
                        close = true;
                    }
                });

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
                        self.selected = if self.selected == 0 {
                            filtered.len() - 1
                        } else {
                            self.selected - 1
                        };
                    }
                    if i.key_pressed(egui::Key::Enter) {
                        if let Some(&cmd) = filtered.get(self.selected) {
                            executed = Some(cmd.clone());
                        }
                    }
                });

                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (i, &cmd) in filtered.iter().enumerate() {
                        let is_selected = i == self.selected;
                        let label = egui::RichText::new(cmd.label()).monospace();
                        let label = if is_selected { label.strong() } else { label };
                        let resp = ui.add(egui::Label::new(label).sense(egui::Sense::click()));
                        if resp.hovered() {
                            self.selected = i;
                        }
                        if resp.clicked() {
                            executed = Some(cmd.clone());
                        }
                        if is_selected {
                            resp.scroll_to_me(None);
                        }
                    }
                });
            });

        if close || executed.is_some() {
            self.visible = false;
        }

        executed
    }
}

/// Lua script console for interactive pixel data manipulation.
///
/// This module is a no-op stub when the `scripting` feature is disabled.
/// With `scripting`, it embeds a Lua 5.4 interpreter via `mlua`.

#[cfg(feature = "scripting")]
mod inner {
    use mlua::{Lua, Result as LuaResult};

    pub struct ScriptConsole {
        pub visible: bool,
        lua: Lua,
        input: String,
        output: Vec<(String, bool)>, // (text, is_error)
        history: Vec<String>,
        history_idx: Option<usize>,
    }

    impl ScriptConsole {
        pub fn new() -> LuaResult<Self> {
            let lua = Lua::new();
            Ok(Self {
                visible: false,
                lua,
                input: String::new(),
                output: Vec::new(),
                history: Vec::new(),
                history_idx: None,
            })
        }

        /// Execute a Lua snippet.  Results are appended to `self.output`.
        pub fn execute(&mut self, code: &str) {
            match self.lua.load(code).eval::<mlua::MultiValue>() {
                Ok(vals) => {
                    let s: Vec<String> = vals.iter()
                        .map(|v| format!("{v:?}"))
                        .collect();
                    if !s.is_empty() {
                        self.output.push((s.join("\t"), false));
                    }
                }
                Err(e) => {
                    self.output.push((format!("[error] {e}"), true));
                }
            }
        }

        pub fn show(&mut self, ctx: &egui::Context) -> Option<ScriptAction> {
            if !self.visible {
                return None;
            }

            let mut action = None;

            egui::Window::new("Script Console")
                .resizable(true)
                .default_size([520.0, 320.0])
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.heading("Lua Console");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Clear").clicked() {
                                self.output.clear();
                            }
                        });
                    });
                    ui.separator();

                    // Output area
                    egui::ScrollArea::vertical()
                        .max_height(200.0)
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for (text, is_err) in &self.output {
                                if *is_err {
                                    ui.colored_label(egui::Color32::from_rgb(255, 90, 90), text);
                                } else {
                                    ui.label(text);
                                }
                            }
                        });

                    ui.separator();

                    // Input
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.input)
                            .desired_width(f32::INFINITY)
                            .hint_text("Enter Lua code…"),
                    );

                    // History navigation
                    if resp.has_focus() {
                        if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                            let idx = self.history_idx
                                .map(|i| i.saturating_sub(1))
                                .unwrap_or(self.history.len().saturating_sub(1));
                            self.history_idx = Some(idx);
                            if let Some(h) = self.history.get(idx) {
                                self.input = h.clone();
                            }
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                            let idx = self.history_idx.map(|i| i + 1);
                            if let Some(i) = idx {
                                if i < self.history.len() {
                                    self.history_idx = Some(i);
                                    self.input = self.history[i].clone();
                                } else {
                                    self.history_idx = None;
                                    self.input.clear();
                                }
                            }
                        }
                    }

                    let submitted = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if submitted || ui.button("Run").clicked() {
                        let code = self.input.trim().to_owned();
                        if !code.is_empty() {
                            self.history.push(code.clone());
                            self.history_idx = None;
                            self.output.push((format!("> {code}"), false));
                            self.execute(&code);
                            // Check if the script wants to open a new tab
                            if let Ok(val) = self.lua.globals().get::<mlua::Value>("_fv_new_tab") {
                                if let mlua::Value::Table(_) = val {
                                    action = Some(ScriptAction::NewTab);
                                    let _ = self.lua.globals().set("_fv_new_tab", mlua::Nil);
                                }
                            }
                        }
                        self.input.clear();
                    }
                });

            action
        }

        /// Register the `fv` global API.
        ///
        /// Called once after the active tab's pixel data is available.
        pub fn setup_api(&mut self, data: Vec<f32>, width: usize, height: usize) -> LuaResult<()> {
            let fv = self.lua.create_table()?;

            // fv.data() → list of floats
            let data_clone = data.clone();
            fv.set("data", self.lua.create_function(move |_, ()| {
                Ok(data_clone.clone())
            })?)?;

            // fv.width() / fv.height()
            fv.set("width", width)?;
            fv.set("height", height)?;

            // fv.show(data_table) → sets _fv_new_tab
            let lua = &self.lua;
            let show_fn = lua.create_function(|lua_ctx, (vals, w, h): (Vec<f32>, usize, usize)| {
                let t = lua_ctx.create_table()?;
                t.set("data", vals)?;
                t.set("width", w)?;
                t.set("height", h)?;
                lua_ctx.globals().set("_fv_new_tab", t)?;
                Ok(())
            })?;
            fv.set("show", show_fn)?;

            self.lua.globals().set("fv", fv)?;
            Ok(())
        }

        /// Retrieve image data from `_fv_new_tab` if the script set it.
        pub fn take_new_tab(&mut self) -> Option<(Vec<f32>, usize, usize)> {
            let val = self.lua.globals().get::<mlua::Value>("_fv_new_tab").ok()?;
            if let mlua::Value::Table(t) = val {
                let data: Vec<f32> = t.get("data").ok()?;
                let w: usize = t.get("width").ok()?;
                let h: usize = t.get("height").ok()?;
                let _ = self.lua.globals().set("_fv_new_tab", mlua::Nil);
                Some((data, w, h))
            } else {
                None
            }
        }
    }

    pub enum ScriptAction {
        NewTab,
    }
}

// ── Public re-exports / stubs ────────────────────────────────────────────────

#[cfg(feature = "scripting")]
pub use inner::{ScriptAction, ScriptConsole};

/// Stub used when the `scripting` feature is disabled.
#[cfg(not(feature = "scripting"))]
pub struct ScriptConsole {
    pub visible: bool,
}

#[cfg(not(feature = "scripting"))]
impl ScriptConsole {
    pub fn new() -> Self {
        Self { visible: false }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<()> {
        if self.visible {
            egui::Window::new("Script Console")
                .show(ctx, |ui| {
                    ui.label("Enable the 'scripting' feature to use the Lua console.");
                });
        }
        None
    }
}

#[cfg(not(feature = "scripting"))]
impl Default for ScriptConsole {
    fn default() -> Self { Self::new() }
}

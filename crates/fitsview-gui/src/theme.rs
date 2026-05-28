use egui::{Color32, FontDefinitions, FontFamily, Rounding, Shadow, Stroke, Style, Visuals};

// --- Catppuccin Mocha palette + astronomy accents ---

pub const BG_PRIMARY: Color32   = Color32::from_rgb(30,  30,  46);   // #1e1e2e
pub const BG_SECONDARY: Color32 = Color32::from_rgb(24,  24,  37);   // #181825
pub const BG_PANEL: Color32     = Color32::from_rgb(17,  17,  27);   // #11111b
pub const BG_HOVER: Color32     = Color32::from_rgb(49,  50,  68);   // #313244
pub const BG_SELECTED: Color32  = Color32::from_rgb(69,  71,  90);   // #45475a

pub const ACCENT: Color32       = Color32::from_rgb(137, 180, 250);  // #89b4fa (blue)
pub const ACCENT_DIM: Color32   = Color32::from_rgb(116, 199, 236);  // #74c7ec (sapphire)

pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(205, 214, 244);  // #cdd6f4
pub const TEXT_MUTED: Color32   = Color32::from_rgb(108, 112, 134);  // #6c7086
pub const TEXT_OVERLAY: Color32 = Color32::from_rgb(166, 173, 200);  // #a6adc8

pub const BORDER: Color32       = Color32::from_rgb(69,  71,  90);   // #45475a
pub const SEPARATOR: Color32    = Color32::from_rgb(49,  50,  68);   // #313244

// Status bar semantic colors
pub const HDU_COLOR: Color32     = ACCENT;
pub const EVENTS_COLOR: Color32  = Color32::from_rgb(137, 220, 235); // #89dceb
pub const LOADING_COLOR: Color32 = Color32::from_rgb(249, 226, 175); // #f9e2af
pub const BLINK_COLOR: Color32   = Color32::from_rgb(250, 179, 135); // #fab387
pub const CURSOR_COLOR: Color32  = Color32::from_rgb(166, 227, 161); // #a6e3a1
pub const SCALE_COLOR: Color32   = Color32::from_rgb(249, 226, 175); // #f9e2af
pub const ERROR_COLOR: Color32   = Color32::from_rgb(243, 139, 168); // #f38ba8

/// Apply the modern theme to the egui context. Call once during app init.
pub fn apply(ctx: &egui::Context) {
    setup_fonts(ctx);

    let mut visuals = Visuals::dark();

    // Window / panel backgrounds
    visuals.window_fill    = BG_SECONDARY;
    visuals.panel_fill     = BG_PRIMARY;
    visuals.faint_bg_color = BG_SECONDARY;
    visuals.extreme_bg_color = BG_PANEL;
    visuals.code_bg_color  = BG_PANEL;

    // Window borders and rounding
    visuals.window_rounding = Rounding::same(8.0);
    visuals.window_stroke   = Stroke::new(1.0, BORDER);
    visuals.window_shadow   = Shadow {
        offset: egui::vec2(0.0, 4.0),
        blur: 16.0,
        spread: 0.0,
        color: Color32::from_black_alpha(120),
    };

    visuals.menu_rounding = Rounding::same(6.0);
    visuals.popup_shadow  = Shadow {
        offset: egui::vec2(0.0, 2.0),
        blur: 8.0,
        spread: 0.0,
        color: Color32::from_black_alpha(100),
    };

    // Widget visuals
    let rounding = Rounding::same(4.0);

    visuals.widgets.noninteractive.bg_fill    = BG_PRIMARY;
    visuals.widgets.noninteractive.weak_bg_fill = BG_PRIMARY;
    visuals.widgets.noninteractive.bg_stroke  = Stroke::new(1.0, SEPARATOR);
    visuals.widgets.noninteractive.fg_stroke  = Stroke::new(1.0, TEXT_MUTED);
    visuals.widgets.noninteractive.rounding   = rounding;

    visuals.widgets.inactive.bg_fill    = BG_SECONDARY;
    visuals.widgets.inactive.weak_bg_fill = BG_SECONDARY;
    visuals.widgets.inactive.bg_stroke  = Stroke::NONE;
    visuals.widgets.inactive.fg_stroke  = Stroke::new(1.0, TEXT_OVERLAY);
    visuals.widgets.inactive.rounding   = rounding;

    visuals.widgets.hovered.bg_fill    = BG_HOVER;
    visuals.widgets.hovered.weak_bg_fill = BG_HOVER;
    visuals.widgets.hovered.bg_stroke  = Stroke::new(1.0, BORDER);
    visuals.widgets.hovered.fg_stroke  = Stroke::new(1.5, TEXT_PRIMARY);
    visuals.widgets.hovered.rounding   = rounding;

    visuals.widgets.active.bg_fill    = BG_SELECTED;
    visuals.widgets.active.weak_bg_fill = BG_SELECTED;
    visuals.widgets.active.bg_stroke  = Stroke::new(1.0, ACCENT);
    visuals.widgets.active.fg_stroke  = Stroke::new(2.0, TEXT_PRIMARY);
    visuals.widgets.active.rounding   = rounding;

    visuals.widgets.open.bg_fill    = BG_HOVER;
    visuals.widgets.open.weak_bg_fill = BG_HOVER;
    visuals.widgets.open.bg_stroke  = Stroke::new(1.0, BORDER);
    visuals.widgets.open.fg_stroke  = Stroke::new(1.5, TEXT_PRIMARY);
    visuals.widgets.open.rounding   = rounding;

    // Selection highlight
    visuals.selection.bg_fill = Color32::from_rgb(137, 180, 250).gamma_multiply(0.30);
    visuals.selection.stroke  = Stroke::new(1.0, ACCENT);

    // Text
    visuals.override_text_color = Some(TEXT_PRIMARY);
    visuals.hyperlink_color     = ACCENT;
    visuals.warn_fg_color       = LOADING_COLOR;
    visuals.error_fg_color      = ERROR_COLOR;

    visuals.indent_has_left_vline = true;
    visuals.slider_trailing_fill  = true;

    ctx.set_visuals(visuals);

    // Style / spacing
    ctx.style_mut(|style| {
        style.spacing.item_spacing   = egui::vec2(6.0, 4.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style.spacing.menu_margin    = egui::Margin::same(4.0);
        style.spacing.indent         = 16.0;
        style.spacing.scroll.bar_width = 6.0;
    });
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    // Bump the default proportional font size to 14px
    fonts.families.entry(FontFamily::Proportional).or_default();
    fonts.families.entry(FontFamily::Monospace).or_default();

    ctx.set_fonts(fonts);

    // Scale text up from egui's default 12px base
    ctx.style_mut(|style: &mut Style| {
        style.text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::new(14.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::new(14.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Heading,
            egui::FontId::new(16.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Small,
            egui::FontId::new(12.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Monospace,
            egui::FontId::new(13.0, FontFamily::Monospace),
        );
    });
}

/// A frame suitable for sidebar panels with the darkest background.
pub fn side_panel_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(BG_PANEL)
        .inner_margin(egui::Margin::symmetric(8.0, 6.0))
}

/// A frame suitable for the tab bar strip.
pub fn tab_bar_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(BG_PANEL)
        .inner_margin(egui::Margin { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 })
}

/// A frame suitable for floating/modal windows (command palette, dialogs).
pub fn modal_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(BG_SECONDARY)
        .stroke(Stroke::new(1.0, BORDER))
        .rounding(Rounding::same(8.0))
        .shadow(Shadow {
            offset: egui::vec2(0.0, 8.0),
            blur: 24.0,
            spread: 0.0,
            color: Color32::from_black_alpha(160),
        })
        .inner_margin(egui::Margin::same(12.0))
}

/// A frame for the status bar.
pub fn status_bar_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(BG_PANEL)
        .inner_margin(egui::Margin::symmetric(10.0, 0.0))
        .stroke(Stroke::new(1.0, SEPARATOR))
}

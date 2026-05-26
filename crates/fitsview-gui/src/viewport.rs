use egui::{Rect, Vec2};

#[derive(Debug, Clone)]
pub struct ViewState {
    /// Image-space offset: the image origin position relative to panel top-left, in image pixels.
    pub offset: Vec2,
    /// Screen pixels per image pixel.
    pub zoom: f32,
}

impl Default for ViewState {
    fn default() -> Self {
        Self { offset: Vec2::ZERO, zoom: 1.0 }
    }
}

impl ViewState {
    pub const ZOOM_MIN: f32 = 0.05;
    pub const ZOOM_MAX: f32 = 64.0;
    pub const ZOOM_STEP: f32 = 1.2;

    /// Pan by a screen-space delta.
    pub fn pan(&mut self, screen_delta: Vec2) {
        self.offset += screen_delta / self.zoom;
    }

    /// Zoom toward a cursor position (screen-space, relative to panel origin).
    pub fn zoom_toward(&mut self, factor: f32, cursor_screen: Vec2) {
        let old_zoom = self.zoom;
        self.zoom = (self.zoom * factor).clamp(Self::ZOOM_MIN, Self::ZOOM_MAX);
        // Keep image coordinate under cursor fixed:
        // cursor_img = cursor_screen / old_zoom - offset
        // offset_new = cursor_screen / new_zoom - cursor_img
        let cursor_img = cursor_screen / old_zoom - self.offset;
        self.offset = cursor_screen / self.zoom - cursor_img;
    }

    /// Fit the entire image inside the available rect, centered.
    pub fn fit_to_rect(&mut self, available: Rect, image_size: Vec2) {
        if image_size.x <= 0.0 || image_size.y <= 0.0 {
            return;
        }
        let scale_x = available.width() / image_size.x;
        let scale_y = available.height() / image_size.y;
        self.zoom = scale_x.min(scale_y);
        let display_size = image_size * self.zoom;
        // offset is in image-space, so convert centering offset back
        self.offset = (available.size() - display_size) * 0.5 / self.zoom;
    }
}

pub enum Colormap {
    Gray,
    Viridis,
    Plasma,
    Inferno,
    Hot,
    Rainbow,
}

pub fn apply_colormap(_value: f32, _cmap: Colormap) -> [u8; 4] {
    todo!("implemented in Step 1")
}

pub fn render_to_rgba(_data: &[f32], _vmin: f32, _vmax: f32, _cmap: Colormap) -> Vec<u8> {
    todo!("implemented in Step 1")
}

#[cfg(test)]
mod tests {
    #[test]
    fn placeholder() {}
}

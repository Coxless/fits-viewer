pub enum ScaleMode {
    ZScale,
    Linear,
    Log,
    Sqrt,
    Asinh,
    MinMax,
}

pub struct ScaleResult {
    pub vmin: f32,
    pub vmax: f32,
}

pub fn compute_scale(_data: &[f32], _mode: ScaleMode) -> ScaleResult {
    todo!("implemented in Step 1")
}

#[cfg(test)]
mod tests {
    #[test]
    fn placeholder() {}
}

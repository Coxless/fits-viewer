use std::collections::HashMap;
use std::path::Path;

pub struct FitsImage {
    pub data: Vec<f32>,
    pub width: usize,
    pub height: usize,
    pub header: HashMap<String, String>,
    pub bitpix: i32,
}

pub fn load_fits(_path: &Path) -> anyhow::Result<FitsImage> {
    todo!("implemented in Step 1")
}

#[cfg(test)]
mod tests {
    #[test]
    fn placeholder() {}
}

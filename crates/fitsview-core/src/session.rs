use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayConfig {
    pub scale_mode: String,
    pub colormap: String,
    pub vmin_override: Option<f32>,
    pub vmax_override: Option<f32>,
    pub zoom: f32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub hdu_index: usize,
    pub show_wcs_grid: bool,
    pub cube_z: usize,
    pub contrast: f32,
    pub bias: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotationRecord {
    pub shape: String,
    pub params: Vec<f64>,
    pub label: String,
    pub color: [u8; 3],
    pub use_wcs: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileState {
    pub path: String,
    pub hdu_index: usize,
    pub display: DisplayConfig,
    pub annotations: Vec<AnnotationRecord>,
    pub region_files: Vec<String>,
    pub show_wcs_grid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSplitLayout {
    /// "single", "side_by_side", or "grid2x2"
    pub kind: String,
    pub pane_paths: Vec<Option<String>>,
    pub linked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub version: u32,
    pub files: Vec<FileState>,
    pub split: SessionSplitLayout,
    #[serde(default = "default_blink_interval")]
    pub blink_interval_secs: f32,
}

fn default_blink_interval() -> f32 {
    0.5
}

impl Session {
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        let session: Self = serde_json::from_str(&json)?;
        Ok(session)
    }

    /// Returns the path to the auto-saved "last session" file.
    pub fn last_path() -> PathBuf {
        let base = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("fits-view").join("last_session.fvs")
    }
}

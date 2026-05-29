use std::collections::HashMap;
use std::path::Path;

use crate::{fits_reader::load_fits_hdu, wcs::Wcs};

#[derive(Debug, Clone)]
pub struct SpectralAxis {
    pub ctype: String,
    pub crval: f64,
    pub cdelt: f64,
    pub crpix: f64,
    pub unit: String,
}

impl SpectralAxis {
    /// Return the physical value (wavelength, frequency, velocity, time, …) at slice index z.
    pub fn value_at_z(&self, z: usize) -> f64 {
        self.crval + (z as f64 - (self.crpix - 1.0)) * self.cdelt
    }

    /// Short display label including unit.
    pub fn label(&self) -> String {
        if self.unit.is_empty() {
            self.ctype.clone()
        } else {
            format!("{} [{}]", self.ctype, self.unit)
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum CollapseMode {
    Sum,
    Mean,
}

pub struct FitsCube {
    /// Pixel data in row-major order: data[z * width * height + y * width + x]
    pub data: Vec<f32>,
    pub width: usize,
    pub height: usize,
    /// Number of slices along the spectral / time axis (NAXIS3).
    pub depth: usize,
    pub header: HashMap<String, String>,
    pub wcs: Option<Wcs>,
    pub spectral_axis: Option<SpectralAxis>,
}

impl FitsCube {
    /// Borrow the 2-D slice at index `z` (0-based).
    pub fn slice_z(&self, z: usize) -> &[f32] {
        let n = self.width * self.height;
        let start = z.min(self.depth.saturating_sub(1)) * n;
        &self.data[start..start + n]
    }

    /// Collapse slices z_min..=z_max into a single 2-D frame using the given mode.
    pub fn collapse_z(&self, z_min: usize, z_max: usize, mode: CollapseMode) -> Vec<f32> {
        let n = self.width * self.height;
        let z0 = z_min.min(self.depth.saturating_sub(1));
        let z1 = z_max.min(self.depth.saturating_sub(1));
        let count = (z1 + 1).saturating_sub(z0);
        if count == 0 || n == 0 {
            return vec![0.0; n];
        }

        let mut out = vec![0.0f32; n];
        for z in z0..=z1 {
            let slice = self.slice_z(z);
            for (o, &v) in out.iter_mut().zip(slice.iter()) {
                *o += v;
            }
        }
        if mode == CollapseMode::Mean {
            let inv = 1.0 / count as f32;
            for v in &mut out {
                *v *= inv;
            }
        }
        out
    }

    /// Extract the spectrum at pixel (x, y) across all z slices.
    pub fn extract_spectrum(&self, x: usize, y: usize) -> Vec<f64> {
        let n = self.width * self.height;
        (0..self.depth)
            .map(|z| {
                let idx = z * n + y * self.width + x;
                if idx < self.data.len() { self.data[idx] as f64 } else { f64::NAN }
            })
            .collect()
    }
}

/// Load a FITS cube (NAXIS ≥ 3) from disk.
///
/// For very large files this may allocate a large amount of memory.
/// Callers should gate on file size before calling this function.
pub fn load_cube(path: &Path, hdu_index: usize) -> anyhow::Result<FitsCube> {
    let img = load_fits_hdu(path, hdu_index)?;
    let header = &img.header;

    // Determine dimensions from NAXIS keywords.
    let naxis1: usize = header.get("NAXIS1").and_then(|v| v.trim().parse().ok()).unwrap_or(img.width);
    let naxis2: usize = header.get("NAXIS2").and_then(|v| v.trim().parse().ok()).unwrap_or(img.height);
    let naxis3: usize = header.get("NAXIS3").and_then(|v| v.trim().parse().ok()).unwrap_or(1);

    let expected = naxis1 * naxis2 * naxis3;
    anyhow::ensure!(
        img.data.len() >= expected,
        "cube data too small: expected {} pixels, got {}",
        expected, img.data.len()
    );

    let spectral_axis = parse_spectral_axis(header);
    let wcs = Wcs::from_header(header);

    Ok(FitsCube {
        data: img.data,
        width: naxis1,
        height: naxis2,
        depth: naxis3,
        header: img.header,
        wcs,
        spectral_axis,
    })
}

fn parse_spectral_axis(header: &HashMap<String, String>) -> Option<SpectralAxis> {
    let ctype = header.get("CTYPE3")?.trim().to_owned();
    let crval: f64 = header.get("CRVAL3")?.trim().parse().ok()?;
    let cdelt: f64 = header.get("CDELT3")?.trim().parse().ok()?;
    let crpix: f64 = header.get("CRPIX3").and_then(|v| v.trim().parse().ok()).unwrap_or(1.0);
    let unit = header.get("CUNIT3").map(|v| v.trim().to_owned()).unwrap_or_default();
    Some(SpectralAxis { ctype, crval, cdelt, crpix, unit })
}

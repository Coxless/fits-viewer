/// Result of aperture photometry on a single source.
#[derive(Clone, Debug)]
pub struct ApertureResult {
    /// Center in image pixels (0-based).
    pub x: f64,
    pub y: f64,
    /// Sky coordinates (if WCS available).
    pub ra: Option<f64>,
    pub dec: Option<f64>,
    pub aperture_radius: f64,
    pub sky_inner: f64,
    pub sky_outer: f64,
    /// Raw sum inside aperture.
    pub target_sum: f32,
    /// Mean sky background per pixel.
    pub sky_mean: f32,
    /// Net flux = target_sum − sky_mean × n_aperture_pixels.
    pub net_flux: f32,
    /// Signal-to-noise ratio (Poisson noise only).
    pub snr: f32,
    /// Calibrated AB magnitude (if MAGZPT / PHOTZP header present).
    pub magnitude: Option<f32>,
}

/// Perform circular aperture photometry.
///
/// - `aperture_radius`: inner circle radius in pixels.
/// - `sky_inner` / `sky_outer`: annulus radii in pixels for background estimation.
#[allow(clippy::too_many_arguments)]
pub fn aperture_photometry(
    data: &[f32],
    width: usize,
    height: usize,
    x: f64,
    y: f64,
    aperture_radius: f64,
    sky_inner: f64,
    sky_outer: f64,
    gain: Option<f32>,
    zero_point: Option<f32>,
) -> ApertureResult {
    let ap2 = aperture_radius * aperture_radius;
    let sky_in2 = sky_inner * sky_inner;
    let sky_out2 = sky_outer * sky_outer;

    let x0 = (x - sky_outer).floor() as isize;
    let x1 = (x + sky_outer).ceil() as isize;
    let y0 = (y - sky_outer).floor() as isize;
    let y1 = (y + sky_outer).ceil() as isize;

    let mut target_sum = 0.0f64;
    let mut n_target = 0usize;
    let mut sky_vals: Vec<f32> = Vec::new();

    for row in y0..=y1 {
        if row < 0 || row >= height as isize { continue; }
        for col in x0..=x1 {
            if col < 0 || col >= width as isize { continue; }
            let v = data[row as usize * width + col as usize];
            if !v.is_finite() { continue; }

            let dx = col as f64 + 0.5 - x;
            let dy = row as f64 + 0.5 - y;
            let r2 = dx * dx + dy * dy;

            if r2 <= ap2 {
                target_sum += v as f64;
                n_target += 1;
            } else if r2 >= sky_in2 && r2 <= sky_out2 {
                sky_vals.push(v);
            }
        }
    }

    // Robust sky: median
    sky_vals.sort_unstable_by(|a, b| a.total_cmp(b));
    let sky_mean = if sky_vals.is_empty() {
        0.0f32
    } else {
        let mid = sky_vals.len() / 2;
        if sky_vals.len().is_multiple_of(2) {
            (sky_vals[mid - 1] + sky_vals[mid]) / 2.0
        } else {
            sky_vals[mid]
        }
    };

    let net_flux = (target_sum - sky_mean as f64 * n_target as f64) as f32;
    let g = gain.unwrap_or(1.0);
    let snr = if net_flux > 0.0 && n_target > 0 {
        let signal = net_flux.abs();
        let noise = ((signal / g + n_target as f32 * sky_mean.abs() / g).sqrt()).max(1.0);
        signal / noise
    } else {
        0.0
    };

    let magnitude = zero_point.map(|zp| zp - 2.5 * net_flux.abs().log10());

    ApertureResult {
        x,
        y,
        ra: None,
        dec: None,
        aperture_radius,
        sky_inner,
        sky_outer,
        target_sum: target_sum as f32,
        sky_mean,
        net_flux,
        snr,
        magnitude,
    }
}

/// Extract zero-point magnitude from FITS header keywords.
pub fn header_zero_point(header: &std::collections::HashMap<String, String>) -> Option<f32> {
    for key in ["MAGZPT", "PHOTZP", "MAGZERO", "ZPT"] {
        if let Some(v) = header.get(key) {
            if let Ok(f) = v.trim().parse::<f32>() {
                return Some(f);
            }
        }
    }
    None
}

/// Extract gain from FITS header keywords.
pub fn header_gain(header: &std::collections::HashMap<String, String>) -> Option<f32> {
    for key in ["GAIN", "EGAIN"] {
        if let Some(v) = header.get(key) {
            if let Ok(f) = v.trim().parse::<f32>() {
                return Some(f);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aperture_simple() {
        // 11x11 image: star at center (5, 5) with value 100, background 10
        let mut data = vec![10.0f32; 121];
        // Mark center pixels as bright
        for row in 3..=7 {
            for col in 3..=7 {
                data[row * 11 + col] = 100.0;
            }
        }
        let result = aperture_photometry(&data, 11, 11, 5.0, 5.0, 2.5, 3.5, 5.5, None, None);
        assert!(result.net_flux > 0.0, "net flux should be positive");
        assert!(result.snr > 1.0);
    }

    #[test]
    fn test_zero_point_header() {
        let mut h = std::collections::HashMap::new();
        h.insert("MAGZPT".to_owned(), "25.5".to_owned());
        let zp = header_zero_point(&h);
        assert_eq!(zp, Some(25.5));
    }
}

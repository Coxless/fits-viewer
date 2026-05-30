/// A 1D profile extracted along a line through an image.
#[derive(Clone, Debug)]
pub struct Profile {
    /// Distance from the start of the line (pixels, or arcsec if WCS is available).
    pub distances: Vec<f32>,
    /// Pixel values sampled along the line.
    pub values: Vec<f32>,
    /// Start point in image coordinates (0-based).
    pub x0: f64,
    pub y0: f64,
    /// End point in image coordinates (0-based).
    pub x1: f64,
    pub y1: f64,
}

/// Result of a 1D Gaussian fit to a profile.
#[derive(Clone, Debug)]
pub struct GaussianFit {
    pub amplitude: f32,
    /// Center position in the same units as `Profile::distances`.
    pub center: f32,
    pub sigma: f32,
    pub fwhm: f32,
    pub offset: f32,
}

/// Extract a 1D profile along the line from (x0, y0) to (x1, y1) using
/// bilinear interpolation with `n_samples` evenly-spaced sample points.
#[allow(clippy::too_many_arguments)]
pub fn extract_profile(
    data: &[f32],
    width: usize,
    height: usize,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    n_samples: usize,
) -> Profile {
    let n = n_samples.max(2);
    let dx = x1 - x0;
    let dy = y1 - y0;
    let total_dist = (dx * dx + dy * dy).sqrt() as f32;

    let mut distances = Vec::with_capacity(n);
    let mut values = Vec::with_capacity(n);

    for i in 0..n {
        let t = i as f64 / (n - 1) as f64;
        let ix = x0 + t * dx;
        let iy = y0 + t * dy;
        let v = bilinear(data, width, height, ix, iy);
        distances.push(t as f32 * total_dist);
        values.push(v);
    }

    Profile { distances, values, x0, y0, x1, y1 }
}

fn bilinear(data: &[f32], width: usize, height: usize, x: f64, y: f64) -> f32 {
    let x0 = x.floor() as isize;
    let y0 = y.floor() as isize;
    let fx = (x - x.floor()) as f32;
    let fy = (y - y.floor()) as f32;

    let get = |col: isize, row: isize| -> f32 {
        if col < 0 || row < 0 || col >= width as isize || row >= height as isize {
            return f32::NAN;
        }
        data[row as usize * width + col as usize]
    };

    let v00 = get(x0, y0);
    let v10 = get(x0 + 1, y0);
    let v01 = get(x0, y0 + 1);
    let v11 = get(x0 + 1, y0 + 1);

    // Handle NaN edges
    let interp = |a: f32, b: f32, t: f32| -> f32 {
        if a.is_nan() { b } else if b.is_nan() { a } else { a + t * (b - a) }
    };

    let top = interp(v00, v10, fx);
    let bot = interp(v01, v11, fx);
    interp(top, bot, fy)
}

/// Fit a 1D Gaussian + constant background to a profile using iterative least squares.
///
/// Returns `None` if the profile is too short or has no valid data.
pub fn fit_gaussian_1d(profile: &Profile) -> Option<GaussianFit> {
    let n = profile.values.len();
    if n < 5 {
        return None;
    }

    // Initial parameter estimate
    let offset = profile.values.iter().copied().filter(|v| v.is_finite())
        .fold(f32::INFINITY, f32::min);
    let peak_idx = profile.values.iter().enumerate()
        .filter(|(_, v)| v.is_finite())
        .max_by(|(_, a), (_, b)| a.total_cmp(b))?.0;

    let amplitude = profile.values[peak_idx] - offset;
    if amplitude <= 0.0 {
        return None;
    }

    let center = profile.distances[peak_idx];

    // Estimate sigma from half-max points
    let half_max = offset + amplitude * 0.5;
    let left = profile.distances.iter().zip(profile.values.iter())
        .take(peak_idx)
        .filter(|(_, &v)| v >= half_max)
        .map(|(&d, _)| d)
        .next()
        .unwrap_or(center - 1.0);
    let right = profile.distances.iter().zip(profile.values.iter())
        .skip(peak_idx)
        .filter(|(_, &v)| v < half_max)
        .map(|(&d, _)| d)
        .next()
        .unwrap_or(center + 1.0);

    let fwhm_est = (right - left).abs().max(f32::EPSILON);
    let sigma_est = fwhm_est / 2.355;

    // Levenberg-Marquardt style: simple gradient descent with 50 iterations
    let mut amp = amplitude;
    let mut ctr = center;
    let mut sig = sigma_est;
    let mut off = offset;
    let lr = 0.01;

    for _ in 0..50 {
        let (mut da, mut dc, mut ds, mut do_) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        let mut sse = 0.0f32;
        let mut count = 0usize;

        for (&d, &y) in profile.distances.iter().zip(profile.values.iter()) {
            if !y.is_finite() { continue; }
            let z = (d - ctr) / sig.max(f32::EPSILON);
            let gauss = amp * (-0.5 * z * z).exp();
            let pred = gauss + off;
            let err = y - pred;
            sse += err * err;
            count += 1;

            da += err * gauss / amp.max(f32::EPSILON);
            dc += err * gauss * z / sig.max(f32::EPSILON);
            ds += err * gauss * (z * z - 1.0) / sig.max(f32::EPSILON);
            do_ += err;
        }

        if count == 0 { break; }
        let scale = 2.0 / count as f32;
        amp = (amp + lr * scale * da).max(f32::EPSILON);
        ctr += lr * scale * dc;
        sig = (sig + lr * scale * ds).max(f32::EPSILON);
        off += lr * scale * do_;
        let _ = sse;
    }

    let fwhm = sig * 2.355;
    Some(GaussianFit { amplitude: amp, center: ctr, sigma: sig, fwhm, offset: off })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_profile_horizontal() {
        let data: Vec<f32> = (0..25).map(|v| v as f32).collect();
        let p = extract_profile(&data, 5, 5, 0.0, 2.0, 4.0, 2.0, 5);
        assert_eq!(p.values.len(), 5);
        assert!((p.values[0] - 10.0).abs() < 1.0); // row 2, col 0 = 10
    }

    #[test]
    fn test_gaussian_fit() {
        // Create a synthetic Gaussian
        let center = 10.0f32;
        let sigma = 2.0f32;
        let amp = 100.0f32;
        let distances: Vec<f32> = (0..21).map(|i| i as f32).collect();
        let values: Vec<f32> = distances.iter().map(|&d| {
            let z = (d - center) / sigma;
            amp * (-0.5 * z * z).exp() + 5.0
        }).collect();
        let p = Profile { distances, values, x0: 0.0, y0: 0.0, x1: 20.0, y1: 0.0 };
        let fit = fit_gaussian_1d(&p).expect("fit should succeed");
        assert!((fit.center - center).abs() < 2.0);
        assert!(fit.fwhm > 0.0);
    }
}

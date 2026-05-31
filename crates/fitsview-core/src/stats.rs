/// Statistics computed over a 2-D FITS image's pixel data.
#[derive(Debug, Clone)]
pub struct ImageStats {
    pub npix: usize,
    pub n_finite: usize,
    pub n_saturated: usize,
    pub min: f32,
    pub max: f32,
    pub mean: f64,
    pub median: f32,
    pub std_dev: f64,
    pub sum: f64,
}

/// Compute statistics over a pixel buffer. NaN and Inf values are excluded.
pub fn compute_stats(data: &[f32]) -> ImageStats {
    if data.is_empty() {
        return ImageStats { npix: 0, n_finite: 0, n_saturated: 0, min: 0.0, max: 0.0, mean: 0.0, median: 0.0, std_dev: 0.0, sum: 0.0 };
    }

    let mut finite: Vec<f32> = data.iter().copied().filter(|v| v.is_finite()).collect();
    let npix = data.len();
    let n_finite = finite.len();

    if n_finite == 0 {
        return ImageStats { npix, n_finite: 0, n_saturated: 0, min: f32::NAN, max: f32::NAN, mean: f64::NAN, median: f32::NAN, std_dev: f64::NAN, sum: 0.0 };
    }

    // Min/max
    let mut vmin = finite[0];
    let mut vmax = finite[0];
    let mut sum = 0.0f64;
    for &v in &finite {
        if v < vmin { vmin = v; }
        if v > vmax { vmax = v; }
        sum += v as f64;
    }
    let mean = sum / n_finite as f64;

    // Std deviation (two-pass for numerical stability)
    let mut sum_sq_dev = 0.0f64;
    for &v in &finite {
        let d = v as f64 - mean;
        sum_sq_dev += d * d;
    }
    let std_dev = (sum_sq_dev / n_finite as f64).sqrt();

    // Median via sort
    finite.sort_unstable_by(|a, b| a.total_cmp(b));
    let median = if n_finite % 2 == 1 {
        finite[n_finite / 2]
    } else {
        (finite[n_finite / 2 - 1] as f64 + finite[n_finite / 2] as f64) as f32 / 2.0
    };

    let n_saturated = data.iter().filter(|&&v| v == vmax).count();

    ImageStats { npix, n_finite, n_saturated, min: vmin, max: vmax, mean, median, std_dev, sum }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_stats() {
        let data: Vec<f32> = (0..=4).map(|i| i as f32).collect(); // [0,1,2,3,4]
        let s = compute_stats(&data);
        assert_eq!(s.npix, 5);
        assert_eq!(s.n_finite, 5);
        assert_eq!(s.min, 0.0);
        assert_eq!(s.max, 4.0);
        assert!((s.mean - 2.0).abs() < 1e-10);
        assert_eq!(s.median, 2.0);
        assert!((s.std_dev - 2.0f64.sqrt()).abs() < 1e-6, "stddev: {}", s.std_dev);
        assert_eq!(s.n_saturated, 1); // only 4.0 is at max
    }

    #[test]
    fn test_nan_exclusion() {
        let data = vec![1.0f32, f32::NAN, 3.0, f32::INFINITY, 5.0];
        let s = compute_stats(&data);
        assert_eq!(s.npix, 5);
        assert_eq!(s.n_finite, 3);
        assert_eq!(s.min, 1.0);
        assert_eq!(s.max, 5.0);
        assert!((s.mean - 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_empty() {
        let s = compute_stats(&[]);
        assert_eq!(s.npix, 0);
    }

    #[test]
    fn test_single() {
        let s = compute_stats(&[42.0f32]);
        assert_eq!(s.min, 42.0);
        assert_eq!(s.max, 42.0);
        assert_eq!(s.median, 42.0);
        assert_eq!(s.std_dev, 0.0);
    }
}

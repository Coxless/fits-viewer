#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScaleMode {
    ZScale,
    Linear,
    Log,
    Sqrt,
    Asinh,
    MinMax,
    HistEq,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScaleResult {
    pub vmin: f32,
    pub vmax: f32,
}

/// Apply the non-linear transfer function for the given scale mode.
/// Returns a value in [0, 1] for input in [0, 1].
/// For `HistEq`, returns `t` unchanged — the caller must apply the LUT separately.
pub fn apply_transfer(t: f32, mode: ScaleMode) -> f32 {
    match mode {
        ScaleMode::Linear | ScaleMode::ZScale | ScaleMode::MinMax | ScaleMode::HistEq => t,
        ScaleMode::Log => (t * 999.0 + 1.0).log10() / 3.0,
        ScaleMode::Sqrt => t.sqrt(),
        ScaleMode::Asinh => (t.asinh() / std::f32::consts::PI).clamp(0.0, 1.0),
    }
}

/// Apply DS9-compatible contrast and bias to a normalized value in [0, 1].
///
/// - `bias`: midpoint of the output range [0, 1], default 0.5.
/// - `contrast`: slope multiplier around the midpoint (1.0 = no change).
pub fn apply_contrast_bias(t: f32, contrast: f32, bias: f32) -> f32 {
    (0.5 + (t - bias) * contrast).clamp(0.0, 1.0)
}

/// Build a 65536-entry histogram equalization LUT.
///
/// The LUT maps normalized intensity index → equalized normalized intensity [0, 1].
/// Caller normalizes a pixel value to `[0, 65535]` via `((v - vmin) / range * 65535)` to index.
pub fn build_histeq_lut(data: &[f32], vmin: f32, vmax: f32) -> Vec<f32> {
    const BINS: usize = 65536;
    let mut hist = vec![0u32; BINS];
    let range = (vmax - vmin).max(f32::EPSILON);
    let mut n_finite = 0usize;

    for &v in data {
        if v.is_finite() {
            let idx = (((v - vmin) / range) * (BINS as f32 - 1.0)).clamp(0.0, BINS as f32 - 1.0) as usize;
            hist[idx] += 1;
            n_finite += 1;
        }
    }

    // Compute CDF and normalize to [0, 1]
    let mut lut = vec![0.0f32; BINS];
    if n_finite == 0 {
        return lut;
    }
    let mut cumsum = 0u64;
    let n = n_finite as f64;
    for i in 0..BINS {
        cumsum += hist[i] as u64;
        lut[i] = (cumsum as f64 / n) as f32;
    }
    lut
}

pub fn compute_scale(data: &[f32], mode: ScaleMode) -> ScaleResult {
    let finite: Vec<f32> = data.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        return ScaleResult { vmin: 0.0, vmax: 1.0 };
    }

    match mode {
        ScaleMode::ZScale => zscale(&finite),
        ScaleMode::MinMax | ScaleMode::Linear | ScaleMode::HistEq => minmax(&finite),
        // Log/Sqrt/Asinh use MinMax range; transfer function applied at render time
        ScaleMode::Log | ScaleMode::Sqrt | ScaleMode::Asinh => minmax(&finite),
    }
}

fn minmax(finite: &[f32]) -> ScaleResult {
    let vmin = finite.iter().copied().fold(f32::INFINITY, f32::min);
    let vmax = finite.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    ScaleResult { vmin, vmax }
}

fn zscale(finite: &[f32]) -> ScaleResult {
    let n = finite.len().min(600);
    if n < 2 {
        return ScaleResult { vmin: finite[0], vmax: finite[0] };
    }

    // Uniform stride sampling
    let stride = (finite.len() / n).max(1);
    let mut samples: Vec<f32> = finite.iter().copied().step_by(stride).take(n).collect();
    samples.sort_unstable_by(|a, b| a.total_cmp(b));

    let n = samples.len();
    if n < 2 {
        return ScaleResult { vmin: samples[0], vmax: samples[0] };
    }

    // Least-squares linear fit: x[i] = i/(n-1), y[i] = samples[i]
    let nf = n as f32;
    let mean_x = 0.5_f32; // sum of i/(n-1) averages to 0.5
    let mean_y: f32 = samples.iter().sum::<f32>() / nf;

    let mut sum_xx = 0.0_f32;
    let mut sum_xy = 0.0_f32;
    for (i, &y) in samples.iter().enumerate() {
        let x = i as f32 / (n - 1) as f32;
        sum_xx += x * x;
        sum_xy += x * y;
    }
    let denom = sum_xx - nf * mean_x * mean_x;

    if denom.abs() < f32::EPSILON {
        return ScaleResult { vmin: samples[0], vmax: samples[n - 1] };
    }

    let slope = (sum_xy - nf * mean_x * mean_y) / denom;

    const CONTRAST: f32 = 0.25;
    let median = samples[n / 2];
    let half_span = (slope / CONTRAST) * 0.5 * (n - 1) as f32;

    let vmin = (median - half_span).clamp(samples[0], samples[n - 1]);
    let vmax = (median + half_span).clamp(samples[0], samples[n - 1]);

    // Guard: if slope is near-zero, fall back to full range
    if vmin >= vmax {
        return ScaleResult { vmin: samples[0], vmax: samples[n - 1] };
    }

    ScaleResult { vmin, vmax }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minmax_scale() {
        let data = vec![0.0_f32, 1.0, 2.0, 100.0];
        let r = compute_scale(&data, ScaleMode::MinMax);
        assert!((r.vmin - 0.0).abs() < 1e-5);
        assert!((r.vmax - 100.0).abs() < 1e-5);
    }

    #[test]
    fn test_zscale_monotonic() {
        let data: Vec<f32> = (0..1000).map(|i| i as f32).collect();
        let r = compute_scale(&data, ScaleMode::ZScale);
        assert!(r.vmin < r.vmax);
        assert!(r.vmin >= 0.0);
        assert!(r.vmax <= 999.0);
    }

    #[test]
    fn test_zscale_uniform() {
        let data = vec![5.0_f32; 100];
        let r = compute_scale(&data, ScaleMode::ZScale);
        assert!(r.vmin <= r.vmax);
    }

    #[test]
    fn test_empty_data() {
        let r = compute_scale(&[], ScaleMode::ZScale);
        assert_eq!(r.vmin, 0.0);
        assert_eq!(r.vmax, 1.0);
    }

    #[test]
    fn test_all_nan() {
        let data = vec![f32::NAN; 50];
        let r = compute_scale(&data, ScaleMode::MinMax);
        assert_eq!(r.vmin, 0.0);
        assert_eq!(r.vmax, 1.0);
    }

    #[test]
    fn test_contrast_bias_identity() {
        let t = apply_contrast_bias(0.5, 1.0, 0.5);
        assert!((t - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_contrast_bias_clamp() {
        assert_eq!(apply_contrast_bias(0.75, 2.0, 0.5), 1.0);
        assert_eq!(apply_contrast_bias(0.25, 2.0, 0.5), 0.0);
    }

    #[test]
    fn test_histeq_lut_size() {
        let data: Vec<f32> = (0..256).map(|i| i as f32).collect();
        let lut = build_histeq_lut(&data, 0.0, 255.0);
        assert_eq!(lut.len(), 65536);
        assert!(lut.last().copied().unwrap_or(0.0) > 0.99);
    }

    #[test]
    fn test_histeq_scale_mode() {
        let data: Vec<f32> = (0..100).map(|i| i as f32).collect();
        let r = compute_scale(&data, ScaleMode::HistEq);
        assert!(r.vmin < r.vmax);
    }
}

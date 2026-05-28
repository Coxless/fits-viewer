#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScaleMode {
    ZScale,
    Linear,
    Log,
    Sqrt,
    Asinh,
    MinMax,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScaleResult {
    pub vmin: f32,
    pub vmax: f32,
}

pub fn apply_transfer(t: f32, mode: ScaleMode) -> f32 {
    match mode {
        ScaleMode::Linear | ScaleMode::ZScale | ScaleMode::MinMax => t,
        ScaleMode::Log => (t * 999.0 + 1.0).log10() / 3.0,
        ScaleMode::Sqrt => t.sqrt(),
        ScaleMode::Asinh => (t.asinh() / std::f32::consts::PI).clamp(0.0, 1.0),
    }
}

pub fn compute_scale(data: &[f32], mode: ScaleMode) -> ScaleResult {
    let finite: Vec<f32> = data.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        return ScaleResult { vmin: 0.0, vmax: 1.0 };
    }

    match mode {
        ScaleMode::ZScale => zscale(&finite),
        ScaleMode::MinMax | ScaleMode::Linear => minmax(&finite),
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
    samples.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());

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
}

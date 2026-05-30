/// A single contour level with its display color.
#[derive(Clone, Debug)]
pub struct ContourLevel {
    pub value: f32,
    pub color: [u8; 4],
}

/// A set of contour polylines for one image.
#[derive(Clone, Debug, Default)]
pub struct ContourLines {
    pub levels: Vec<ContourLevel>,
    /// polylines[i] contains polylines for levels[i].
    pub polylines: Vec<Vec<Vec<(f32, f32)>>>,
}

/// Compute contour lines for the given pixel data using the Marching Squares algorithm.
///
/// Returns pixel-coordinate polylines.  `levels` must be sorted ascending.
pub fn compute_contours(
    data: &[f32],
    width: usize,
    height: usize,
    levels: &[ContourLevel],
) -> ContourLines {
    if width < 2 || height < 2 || data.len() < width * height {
        return ContourLines {
            levels: levels.to_vec(),
            polylines: vec![Vec::new(); levels.len()],
        };
    }

    let mut polylines_per_level: Vec<Vec<Vec<(f32, f32)>>> = vec![Vec::new(); levels.len()];

    for (li, level) in levels.iter().enumerate() {
        let threshold = level.value;
        // Marching squares: iterate over 2×2 cells
        let mut segments: Vec<((f32, f32), (f32, f32))> = Vec::new();

        for row in 0..height - 1 {
            for col in 0..width - 1 {
                let v00 = data[row * width + col];
                let v10 = data[row * width + (col + 1)];
                let v01 = data[(row + 1) * width + col];
                let v11 = data[(row + 1) * width + (col + 1)];

                let case = ((v00 >= threshold) as u8)
                    | (((v10 >= threshold) as u8) << 1)
                    | (((v11 >= threshold) as u8) << 2)
                    | (((v01 >= threshold) as u8) << 3);

                let x = col as f32;
                let y = row as f32;

                // Interpolate crossing point on an edge
                let lerp = |a: f32, b: f32| -> f32 {
                    if (b - a).abs() < f32::EPSILON { 0.5 } else { (threshold - a) / (b - a) }
                };

                // Edge midpoints (interpolated)
                let top    = (x + lerp(v00, v10), y);
                let right  = (x + 1.0, y + lerp(v10, v11));
                let bottom = (x + lerp(v01, v11), y + 1.0);
                let left   = (x, y + lerp(v00, v01));

                match case {
                    0 | 15 => {}
                    1 | 14 => segments.push((top, left)),
                    2 | 13 => segments.push((top, right)),
                    3 | 12 => segments.push((left, right)),
                    4 | 11 => segments.push((bottom, right)),
                    5 => { segments.push((top, right)); segments.push((bottom, left)); }
                    6 | 9  => segments.push((top, bottom)),
                    7 | 8  => segments.push((bottom, left)),
                    10 => { segments.push((top, left)); segments.push((bottom, right)); }
                    _ => {}
                }
            }
        }

        polylines_per_level[li] = chain_segments(segments);
    }

    ContourLines {
        levels: levels.to_vec(),
        polylines: polylines_per_level,
    }
}

/// Chain disconnected line segments into polylines for smoother rendering.
fn chain_segments(mut segments: Vec<((f32, f32), (f32, f32))>) -> Vec<Vec<(f32, f32)>> {
    let mut polylines: Vec<Vec<(f32, f32)>> = Vec::new();

    while !segments.is_empty() {
        let (start, end) = segments.remove(0);
        let mut polyline = vec![start, end];

        loop {
            let tail = *polyline.last().unwrap();
            let next = segments.iter().position(|(a, b)| {
                close(tail, *a) || close(tail, *b)
            });
            match next {
                None => break,
                Some(i) => {
                    let (a, b) = segments.remove(i);
                    if close(tail, a) {
                        polyline.push(b);
                    } else {
                        polyline.push(a);
                    }
                }
            }
        }

        polylines.push(polyline);
    }

    polylines
}

fn close(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3
}

/// Helper: generate sigma-based contour levels from image statistics.
pub fn sigma_levels(mean: f32, std: f32, sigmas: &[f32], color: [u8; 4]) -> Vec<ContourLevel> {
    sigmas.iter().map(|&s| ContourLevel { value: mean + s * std, color }).collect()
}

/// Helper: generate percentile-based contour levels.
pub fn percentile_levels(data: &[f32], percentiles: &[f32], color: [u8; 4]) -> Vec<ContourLevel> {
    let mut finite: Vec<f32> = data.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        return Vec::new();
    }
    finite.sort_unstable_by(|a, b| a.total_cmp(b));
    let n = finite.len();
    percentiles.iter().map(|&p| {
        let idx = ((p / 100.0) * n as f32).clamp(0.0, (n - 1) as f32) as usize;
        ContourLevel { value: finite[idx], color }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contour_empty() {
        let levels = vec![ContourLevel { value: 0.5, color: [255, 255, 255, 255] }];
        let result = compute_contours(&[], 0, 0, &levels);
        assert!(result.polylines[0].is_empty());
    }

    #[test]
    fn test_contour_uniform() {
        let data = vec![1.0f32; 9];
        let levels = vec![ContourLevel { value: 0.5, color: [255, 255, 255, 255] }];
        let result = compute_contours(&data, 3, 3, &levels);
        // All above threshold: case 15 → no segments
        assert!(result.polylines[0].is_empty());
    }

    #[test]
    fn test_contour_half() {
        // Left half 0, right half 1
        let data = vec![
            0.0, 0.0, 1.0,
            0.0, 0.0, 1.0,
            0.0, 0.0, 1.0,
        ];
        let levels = vec![ContourLevel { value: 0.5, color: [255, 255, 255, 255] }];
        let result = compute_contours(&data, 3, 3, &levels);
        assert!(!result.polylines[0].is_empty());
    }
}

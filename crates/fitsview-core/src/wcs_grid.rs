use crate::wcs::Wcs;

/// One grid line (RA or Dec) represented as a polyline in image-pixel coords (0-based).
pub struct WcsGridLine {
    /// Polyline points in image-pixel coordinates (0-based, f32 for egui).
    pub points: Vec<(f32, f32)>,
    /// Axis label text (e.g. "12h 30m" or "+45°").
    pub label: String,
    /// True = RA line (constant RA), False = Dec line (constant Dec).
    pub is_ra: bool,
}

pub struct WcsGridLines {
    pub lines: Vec<WcsGridLine>,
}

/// Candidate grid spacings in degrees, from coarsest to finest.
const GRID_SPACINGS: &[f64] = &[
    60.0, 30.0, 15.0, 10.0, 5.0, 2.0, 1.0, 0.5, 0.25, 0.1,
    1.0 / 6.0,   // 10'
    1.0 / 12.0,  // 5'
    1.0 / 30.0,  // 2'
    1.0 / 60.0,  // 1'
    1.0 / 360.0, // 10"
    1.0 / 600.0, // 6"
    1.0 / 1800.0, // 2"
    1.0 / 3600.0, // 1"
];

/// Compute WCS grid lines in image-pixel coordinates.
///
/// Returns polylines that can be drawn directly on the viewport.
/// Points are in 0-based image-pixel space; caller converts to screen using `ViewState`.
pub fn compute_wcs_grid(wcs: &Wcs, img_width: usize, img_height: usize) -> WcsGridLines {
    let w = img_width as f64;
    let h = img_height as f64;

    // Sample corners + center to determine RA/Dec range of the field of view.
    let sample_pts = [
        (0.0, 0.0),
        (w - 1.0, 0.0),
        (0.0, h - 1.0),
        (w - 1.0, h - 1.0),
        (w / 2.0, h / 2.0),
    ];

    let mut ra_min = f64::MAX;
    let mut ra_max = f64::MIN;
    let mut dec_min = f64::MAX;
    let mut dec_max = f64::MIN;

    for &(px, py) in &sample_pts {
        if let Some((ra, dec)) = wcs.pixel_to_world(px, py) {
            ra_min = ra_min.min(ra);
            ra_max = ra_max.max(ra);
            dec_min = dec_min.min(dec);
            dec_max = dec_max.max(dec);
        }
    }

    if ra_min > ra_max || dec_min > dec_max {
        return WcsGridLines { lines: vec![] };
    }

    // Choose grid spacing: the largest that gives 3–8 lines per axis.
    let ra_span = ra_max - ra_min;
    let dec_span = dec_max - dec_min;

    let ra_step = choose_step(ra_span);
    let dec_step = choose_step(dec_span);

    let mut lines = Vec::new();

    // --- RA grid lines (constant RA, varying Dec) ---
    let ra_first = (ra_min / ra_step).ceil() * ra_step;
    let mut ra = ra_first;
    while ra <= ra_max + ra_step * 0.01 {
        let mut pts: Vec<(f32, f32)> = Vec::new();
        let steps = 40usize;
        for j in 0..=steps {
            let dec = dec_min + dec_span * j as f64 / steps as f64;
            if let Some((px, py)) = wcs.world_to_pixel(ra, dec) {
                pts.push((px as f32, py as f32));
            } else if !pts.is_empty() {
                // Gap in line — start a new segment next time
                if pts.len() >= 2 {
                    lines.push(WcsGridLine {
                        points: pts.clone(),
                        label: format_ra_label(ra),
                        is_ra: true,
                    });
                }
                pts.clear();
            }
        }
        if pts.len() >= 2 {
            lines.push(WcsGridLine {
                points: pts,
                label: format_ra_label(ra),
                is_ra: true,
            });
        }
        ra += ra_step;
    }

    // --- Dec grid lines (constant Dec, varying RA) ---
    let dec_first = (dec_min / dec_step).ceil() * dec_step;
    let mut dec = dec_first;
    while dec <= dec_max + dec_step * 0.01 {
        let mut pts: Vec<(f32, f32)> = Vec::new();
        let steps = 40usize;
        for i in 0..=steps {
            let ra = ra_min + ra_span * i as f64 / steps as f64;
            if let Some((px, py)) = wcs.world_to_pixel(ra, dec) {
                pts.push((px as f32, py as f32));
            } else if !pts.is_empty() {
                if pts.len() >= 2 {
                    lines.push(WcsGridLine {
                        points: pts.clone(),
                        label: format_dec_label(dec),
                        is_ra: false,
                    });
                }
                pts.clear();
            }
        }
        if pts.len() >= 2 {
            lines.push(WcsGridLine {
                points: pts,
                label: format_dec_label(dec),
                is_ra: false,
            });
        }
        dec += dec_step;
    }

    WcsGridLines { lines }
}

fn choose_step(span: f64) -> f64 {
    for &step in GRID_SPACINGS {
        let n = span / step;
        if (2.5..=9.0).contains(&n) {
            return step;
        }
    }
    // Fallback: smallest step
    *GRID_SPACINGS.last().unwrap()
}

fn format_ra_label(ra_deg: f64) -> String {
    let ra = ra_deg.rem_euclid(360.0);
    let total_sec = ra * 3600.0 / 15.0;
    let h = (total_sec / 3600.0).floor() as u32;
    let rem = total_sec - h as f64 * 3600.0;
    let m = (rem / 60.0).floor() as u32;
    format!("{h:02}h{m:02}m")
}

fn format_dec_label(dec_deg: f64) -> String {
    let sign = if dec_deg < 0.0 { '-' } else { '+' };
    let abs = dec_deg.abs();
    let d = abs.floor() as u32;
    let rem = (abs - d as f64) * 60.0;
    let m = rem.floor() as u32;
    format!("{sign}{d:02}°{m:02}′")
}

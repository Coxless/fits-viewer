use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq)]
pub enum CoordFrame {
    #[default]
    Image,
    Physical,
    Fk5,
    Icrs,
    Galactic,
}

#[derive(Debug, Clone)]
pub struct RegionStyle {
    /// RGB color, default DS9 green
    pub color: [u8; 3],
    pub line_width: f32,
    pub dashed: bool,
}

impl Default for RegionStyle {
    fn default() -> Self {
        Self { color: [0, 255, 0], line_width: 1.0, dashed: false }
    }
}

#[derive(Debug, Clone)]
pub enum RegionShape {
    Circle { x: f64, y: f64, r: f64 },
    Ellipse { x: f64, y: f64, rx: f64, ry: f64, angle_deg: f64 },
    Box { x: f64, y: f64, w: f64, h: f64, angle_deg: f64 },
    Polygon(Vec<(f64, f64)>),
    Line { x1: f64, y1: f64, x2: f64, y2: f64 },
    Text { x: f64, y: f64, text: String },
    Point { x: f64, y: f64 },
    Annulus { x: f64, y: f64, r_inner: f64, r_outer: f64 },
}

#[derive(Debug, Clone)]
pub struct Region {
    pub shape: RegionShape,
    pub frame: CoordFrame,
    pub style: RegionStyle,
    /// Exclusion region (prefixed with '-' in DS9)
    pub excluded: bool,
}

#[derive(Debug, Default, Clone)]
pub struct RegionFile {
    pub global_frame: CoordFrame,
    pub regions: Vec<Region>,
}

/// Parse DS9-format region file content from a string.
pub fn parse_region_str(content: &str) -> RegionFile {
    let mut rf = RegionFile::default();
    let mut current_frame = CoordFrame::Image;
    let mut global_style = RegionStyle::default();

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        // Comment line
        if line.starts_with('#') {
            // Skip comment-only lines (but parse any global defaults if needed)
            continue;
        }

        // Coordinate frame declarations
        if let Some(frame) = parse_frame_keyword(line) {
            current_frame = frame.clone();
            if matches!(rf.global_frame, CoordFrame::Image) {
                rf.global_frame = frame;
            }
            continue;
        }

        // Global keyword line
        if line.to_ascii_lowercase().starts_with("global ") {
            global_style = parse_style_props(&line["global ".len()..], &global_style);
            continue;
        }

        // Shape line: optionally prefixed with '-' for exclusion
        let (excluded, shape_line) = if let Some(s) = line.strip_prefix('-') {
            (true, s)
        } else if let Some(s) = line.strip_prefix('+') {
            (false, s)
        } else {
            (false, line)
        };

        // Split on '#' to separate shape from per-region properties
        let (shape_str, props_str) = match shape_line.find('#') {
            Some(pos) => (&shape_line[..pos], &shape_line[pos + 1..]),
            None => (shape_line, ""),
        };
        let shape_str = shape_str.trim();
        let props_str = props_str.trim();

        if shape_str.is_empty() {
            continue;
        }

        if let Some(shape) = parse_shape(shape_str, &current_frame) {
            let style = if props_str.is_empty() {
                global_style.clone()
            } else {
                parse_style_props(props_str, &global_style)
            };
            rf.regions.push(Region {
                shape,
                frame: current_frame.clone(),
                style,
                excluded,
            });
        }
    }

    rf
}

/// Load and parse a .reg file from disk.
pub fn load_region_file(path: &Path) -> anyhow::Result<RegionFile> {
    let content = std::fs::read_to_string(path)?;
    Ok(parse_region_str(&content))
}

// ── Parser helpers ─────────────────────────────────────────────────────────────

fn parse_frame_keyword(line: &str) -> Option<CoordFrame> {
    match line.to_ascii_lowercase().split_whitespace().next()? {
        "image" | "physical" => Some(CoordFrame::Image),
        "fk5" | "j2000" => Some(CoordFrame::Fk5),
        "icrs" => Some(CoordFrame::Icrs),
        "galactic" => Some(CoordFrame::Galactic),
        "b1950" | "fk4" => Some(CoordFrame::Fk5), // treat as FK5 for display
        _ => None,
    }
}

fn parse_style_props(props: &str, base: &RegionStyle) -> RegionStyle {
    let mut style = base.clone();
    for token in props.split_whitespace() {
        let token = token.trim_matches(',');
        if let Some(val) = token.strip_prefix("color=") {
            style.color = parse_color_name(val);
        } else if let Some(val) = token.strip_prefix("width=") {
            if let Ok(w) = val.parse::<f32>() {
                style.line_width = w;
            }
        } else if token == "dash=1" || token == "dashed=1" {
            style.dashed = true;
        }
    }
    style
}

fn parse_color_name(name: &str) -> [u8; 3] {
    match name.to_ascii_lowercase().as_str() {
        "white"   => [255, 255, 255],
        "black"   => [0,   0,   0  ],
        "red"     => [255, 0,   0  ],
        "green"   => [0,   255, 0  ],
        "blue"    => [0,   0,   255],
        "cyan"    => [0,   255, 255],
        "magenta" => [255, 0,   255],
        "yellow"  => [255, 255, 0  ],
        "orange"  => [255, 165, 0  ],
        _         => [0,   255, 0  ], // default green
    }
}

fn parse_shape(s: &str, frame: &CoordFrame) -> Option<RegionShape> {
    // Find the opening parenthesis
    let paren = s.find('(')?;
    let name = s[..paren].trim().to_ascii_lowercase();
    let inner = s[paren + 1..].trim_end_matches(')');
    let params = parse_coords(inner, frame);

    match name.as_str() {
        "circle" => {
            if params.len() >= 3 {
                Some(RegionShape::Circle { x: params[0], y: params[1], r: params[2] })
            } else {
                None
            }
        }
        "ellipse" => {
            if params.len() >= 4 {
                let angle = if params.len() >= 5 { params[4] } else { 0.0 };
                Some(RegionShape::Ellipse { x: params[0], y: params[1], rx: params[2], ry: params[3], angle_deg: angle })
            } else {
                None
            }
        }
        "box" | "rotbox" => {
            if params.len() >= 4 {
                let angle = if params.len() >= 5 { params[4] } else { 0.0 };
                Some(RegionShape::Box { x: params[0], y: params[1], w: params[2], h: params[3], angle_deg: angle })
            } else {
                None
            }
        }
        "polygon" => {
            if params.len() >= 4 && params.len().is_multiple_of(2) {
                let points = params.chunks(2).map(|c| (c[0], c[1])).collect();
                Some(RegionShape::Polygon(points))
            } else {
                None
            }
        }
        "line" => {
            if params.len() >= 4 {
                Some(RegionShape::Line { x1: params[0], y1: params[1], x2: params[2], y2: params[3] })
            } else {
                None
            }
        }
        "point" | "x" | "cross" | "diamond" => {
            if params.len() >= 2 {
                Some(RegionShape::Point { x: params[0], y: params[1] })
            } else {
                None
            }
        }
        "annulus" => {
            if params.len() >= 4 {
                Some(RegionShape::Annulus { x: params[0], y: params[1], r_inner: params[2], r_outer: params[3] })
            } else {
                None
            }
        }
        "text" => {
            if params.len() >= 2 {
                // Text content often in braces in the props string; use placeholder
                Some(RegionShape::Text { x: params[0], y: params[1], text: String::new() })
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Parse a comma-separated coordinate list, handling sexagesimal (hh:mm:ss / dd:mm:ss)
/// and arc-second/arc-minute suffixes.
fn parse_coords(s: &str, frame: &CoordFrame) -> Vec<f64> {
    let mut result = Vec::new();
    let is_wcs = matches!(frame, CoordFrame::Fk5 | CoordFrame::Icrs | CoordFrame::Galactic);
    let mut ra_index = 0usize; // for alternating RA/Dec parsing in WCS frames

    for (i, token) in s.split(',').enumerate() {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }

        let val = if is_wcs && i % 2 == 0 {
            // Even index = RA-like coordinate
            parse_sexagesimal_ra(token).unwrap_or_else(|| parse_angular_deg(token).unwrap_or(0.0))
        } else if is_wcs && i % 2 == 1 {
            // Odd index = Dec-like or angular size
            parse_sexagesimal_dec(token).unwrap_or_else(|| parse_angular_deg(token).unwrap_or(0.0))
        } else {
            // Image coordinates: plain numeric, possibly with arcsec suffix
            parse_angular_deg(token).unwrap_or_else(|| token.trim_end_matches(['"', '\'', 'd']).parse().unwrap_or(0.0))
        };
        let _ = ra_index; // suppress unused warning
        ra_index = i + 1;
        result.push(val);
    }
    result
}

/// Parse "hh:mm:ss.sss" or "hh mm ss.sss" → degrees (×15 for RA)
fn parse_sexagesimal_ra(s: &str) -> Option<f64> {
    let parts = split_sexagesimal(s)?;
    Some((parts[0] + parts[1] / 60.0 + parts[2] / 3600.0) * 15.0)
}

/// Parse "±dd:mm:ss.ss" → degrees
fn parse_sexagesimal_dec(s: &str) -> Option<f64> {
    let neg = s.starts_with('-');
    let s = s.trim_start_matches(['+', '-'].as_ref());
    let parts = split_sexagesimal(s)?;
    let abs = parts[0] + parts[1] / 60.0 + parts[2] / 3600.0;
    Some(if neg { -abs } else { abs })
}

fn split_sexagesimal(s: &str) -> Option<[f64; 3]> {
    // Handle both ':' and ' ' separators
    let sep = if s.contains(':') { ':' } else { ' ' };
    let mut it = s.splitn(3, sep);
    let h: f64 = it.next()?.parse().ok()?;
    let m: f64 = it.next()?.parse().ok()?;
    let sec_str = it.next().unwrap_or("0");
    // Remove DS9-specific suffixes like 's', 'd', etc.
    let sec_clean: String = sec_str.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    let sec: f64 = sec_clean.parse().ok()?;
    Some([h, m, sec])
}

/// Parse angular values: plain degrees, or with arcsec (") or arcmin (') suffix.
fn parse_angular_deg(s: &str) -> Option<f64> {
    if let Some(stripped) = s.strip_suffix('"') {
        stripped.trim().parse::<f64>().ok().map(|v| v / 3600.0)
    } else if let Some(stripped) = s.strip_suffix('\'') {
        stripped.trim().parse::<f64>().ok().map(|v| v / 60.0)
    } else if s.ends_with('d') || s.ends_with('°') {
        let end = s.len() - s.chars().last()?.len_utf8();
        s[..end].trim().parse::<f64>().ok()
    } else {
        s.parse::<f64>().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circle_image() {
        let rf = parse_region_str("# Region file format: DS9\nimage\ncircle(100,200,15)");
        assert_eq!(rf.regions.len(), 1);
        if let RegionShape::Circle { x, y, r } = rf.regions[0].shape {
            assert!((x - 100.0).abs() < 1e-6);
            assert!((y - 200.0).abs() < 1e-6);
            assert!((r - 15.0).abs() < 1e-6);
        } else {
            panic!("Expected Circle");
        }
    }

    #[test]
    fn test_exclusion_region() {
        let rf = parse_region_str("image\n-circle(50,50,10)");
        assert_eq!(rf.regions.len(), 1);
        assert!(rf.regions[0].excluded);
    }

    #[test]
    fn test_color_style() {
        let rf = parse_region_str("global color=red\nimage\ncircle(1,1,1)");
        assert_eq!(rf.regions[0].style.color, [255, 0, 0]);
    }

    #[test]
    fn test_polygon() {
        let rf = parse_region_str("image\npolygon(1,1,2,3,4,1)");
        assert_eq!(rf.regions.len(), 1);
        if let RegionShape::Polygon(pts) = &rf.regions[0].shape {
            assert_eq!(pts.len(), 3);
        } else {
            panic!("Expected Polygon");
        }
    }

    #[test]
    fn test_malformed_line_skipped() {
        let rf = parse_region_str("image\nmalformed_garbage\ncircle(1,1,1)");
        assert_eq!(rf.regions.len(), 1);
    }

    #[test]
    fn test_format_ra() {
        use crate::wcs::Wcs;
        assert_eq!(Wcs::format_ra(180.0), "12 00 00.00");
    }
}

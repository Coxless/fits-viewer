use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Projection {
    Tan,
    Sin,
    Car,
    Other(String),
}

#[derive(Debug, Clone)]
pub struct Wcs {
    /// 0-based reference pixel (FITS CRPIX is 1-based, stored here as 0-based)
    crpix: [f64; 2],
    /// Reference world coordinates in degrees (RA, Dec)
    crval: [f64; 2],
    /// CD matrix in degrees/pixel
    cd: [[f64; 2]; 2],
    /// Inverse CD matrix (for world→pixel)
    cd_inv: [[f64; 2]; 2],
    pub proj: Projection,
}

impl Wcs {
    /// Parse WCS from a FITS header HashMap. Returns None if required keywords are absent.
    pub fn from_header(header: &HashMap<String, String>) -> Option<Self> {
        let crpix1: f64 = header.get("CRPIX1")?.parse().ok()?;
        let crpix2: f64 = header.get("CRPIX2")?.parse().ok()?;
        let crval1: f64 = header.get("CRVAL1")?.parse().ok()?;
        let crval2: f64 = header.get("CRVAL2")?.parse().ok()?;

        let proj = detect_projection(header);

        // Build CD matrix from CD form, CDELT+PC form, or legacy CDELT+CROTA2
        let cd = build_cd_matrix(header)?;

        // Compute inverse CD matrix (2×2 inverse)
        let det = cd[0][0] * cd[1][1] - cd[0][1] * cd[1][0];
        if det.abs() < 1e-30 {
            return None;
        }
        let cd_inv = [
            [cd[1][1] / det, -cd[0][1] / det],
            [-cd[1][0] / det, cd[0][0] / det],
        ];

        Some(Wcs {
            crpix: [crpix1 - 1.0, crpix2 - 1.0], // convert to 0-based
            crval: [crval1, crval2],
            cd,
            cd_inv,
            proj,
        })
    }

    /// Convert 0-based image pixel (px, py) → (ra_deg, dec_deg).
    /// Returns None if the point is behind the projection plane.
    pub fn pixel_to_world(&self, px: f64, py: f64) -> Option<(f64, f64)> {
        let dx = px - self.crpix[0];
        let dy = py - self.crpix[1];

        // Intermediate world coordinates in degrees
        let xi  = self.cd[0][0] * dx + self.cd[0][1] * dy;
        let eta = self.cd[1][0] * dx + self.cd[1][1] * dy;

        match self.proj {
            Projection::Tan | Projection::Other(_) => tan_deproject(xi, eta, self.crval),
            Projection::Sin => sin_deproject(xi, eta, self.crval),
            Projection::Car => car_deproject(xi, eta, self.crval),
        }
    }

    /// Convert (ra_deg, dec_deg) → 0-based image pixel (px, py).
    /// Returns None if the point is behind the projection plane.
    pub fn world_to_pixel(&self, ra_deg: f64, dec_deg: f64) -> Option<(f64, f64)> {
        let (xi, eta) = match self.proj {
            Projection::Tan | Projection::Other(_) => tan_project(ra_deg, dec_deg, self.crval)?,
            Projection::Sin => sin_project(ra_deg, dec_deg, self.crval)?,
            Projection::Car => car_project(ra_deg, dec_deg, self.crval),
        };

        let dx = self.cd_inv[0][0] * xi + self.cd_inv[0][1] * eta;
        let dy = self.cd_inv[1][0] * xi + self.cd_inv[1][1] * eta;

        Some((self.crpix[0] + dx, self.crpix[1] + dy))
    }

    /// Format RA degrees as "HH MM SS.ss"
    pub fn format_ra(deg: f64) -> String {
        let ra = deg.rem_euclid(360.0);
        let total_sec = ra * 3600.0 / 15.0;
        let h = (total_sec / 3600.0).floor() as u32;
        let rem = total_sec - h as f64 * 3600.0;
        let m = (rem / 60.0).floor() as u32;
        let s = rem - m as f64 * 60.0;
        format!("{h:02} {m:02} {s:05.2}")
    }

    /// Format Dec degrees as "±DD MM SS.s"
    pub fn format_dec(deg: f64) -> String {
        let sign = if deg < 0.0 { '-' } else { '+' };
        let abs = deg.abs();
        let total_sec = abs * 3600.0;
        let d = (total_sec / 3600.0).floor() as u32;
        let rem = total_sec - d as f64 * 3600.0;
        let m = (rem / 60.0).floor() as u32;
        let s = rem - m as f64 * 60.0;
        format!("{sign}{d:02} {m:02} {s:04.1}")
    }
}

// ── Projection helpers ─────────────────────────────────────────────────────────

const D2R: f64 = std::f64::consts::PI / 180.0;
const R2D: f64 = 180.0 / std::f64::consts::PI;

fn tan_deproject(xi: f64, eta: f64, crval: [f64; 2]) -> Option<(f64, f64)> {
    let xi_r  = xi  * D2R;
    let eta_r = eta * D2R;
    let ra0  = crval[0] * D2R;
    let dec0 = crval[1] * D2R;

    let denom = dec0.cos() - eta_r * dec0.sin();
    if denom.abs() < 1e-15 {
        return None;
    }
    let ra  = ra0 + (xi_r).atan2(denom);
    let dec = (dec0.sin() + eta_r * dec0.cos())
        .atan2((xi_r * xi_r + denom * denom).sqrt());

    Some((ra * R2D, dec * R2D))
}

fn sin_deproject(xi: f64, eta: f64, crval: [f64; 2]) -> Option<(f64, f64)> {
    // SIN (orthographic) projection
    let xi_r  = xi  * D2R;
    let eta_r = eta * D2R;
    let ra0  = crval[0] * D2R;
    let dec0 = crval[1] * D2R;

    let rho2 = xi_r * xi_r + eta_r * eta_r;
    if rho2 > 1.0 {
        return None; // behind sphere
    }
    let cos_c = (1.0 - rho2).sqrt();
    let dec = (cos_c * dec0.sin() + eta_r * dec0.cos()).asin();
    let ra  = ra0 + xi_r.atan2(cos_c * dec0.cos() - eta_r * dec0.sin());
    Some((ra * R2D, dec * R2D))
}

fn car_deproject(xi: f64, eta: f64, crval: [f64; 2]) -> Option<(f64, f64)> {
    // CAR (Cartesian/plate-carree) — simple linear
    Some((crval[0] + xi, crval[1] + eta))
}

fn tan_project(ra_deg: f64, dec_deg: f64, crval: [f64; 2]) -> Option<(f64, f64)> {
    let ra  = ra_deg  * D2R;
    let dec = dec_deg * D2R;
    let ra0  = crval[0] * D2R;
    let dec0 = crval[1] * D2R;

    let denom = dec.sin() * dec0.sin() + dec.cos() * dec0.cos() * (ra - ra0).cos();
    if denom <= 0.0 {
        return None;
    }
    let xi  = dec.cos() * (ra - ra0).sin() / denom;
    let eta = (dec0.cos() * dec.sin() - dec0.sin() * dec.cos() * (ra - ra0).cos()) / denom;
    Some((xi * R2D, eta * R2D))
}

fn sin_project(ra_deg: f64, dec_deg: f64, crval: [f64; 2]) -> Option<(f64, f64)> {
    let ra  = ra_deg  * D2R;
    let dec = dec_deg * D2R;
    let ra0  = crval[0] * D2R;
    let dec0 = crval[1] * D2R;

    let cos_c = dec0.sin() * dec.sin() + dec0.cos() * dec.cos() * (ra - ra0).cos();
    let xi  = dec.cos() * (ra - ra0).sin();
    let eta = dec0.cos() * dec.sin() - dec0.sin() * dec.cos() * (ra - ra0).cos();
    if cos_c <= 0.0 {
        return None;
    }
    Some((xi * R2D / cos_c, eta * R2D / cos_c))
}

fn car_project(ra_deg: f64, dec_deg: f64, crval: [f64; 2]) -> (f64, f64) {
    (ra_deg - crval[0], dec_deg - crval[1])
}

// ── Header parsing helpers ─────────────────────────────────────────────────────

fn detect_projection(header: &HashMap<String, String>) -> Projection {
    let ctype1 = header.get("CTYPE1").map(|s| s.to_ascii_uppercase()).unwrap_or_default();
    // CTYPE format: "RA---TAN", "RA---SIN", etc.
    if ctype1.len() >= 8 {
        let proj_code = ctype1[5..8].trim_end_matches('-');
        return match proj_code {
            "TAN" => Projection::Tan,
            "SIN" => Projection::Sin,
            "CAR" => Projection::Car,
            other => Projection::Other(other.to_owned()),
        };
    }
    Projection::Tan // default assumption
}

fn build_cd_matrix(header: &HashMap<String, String>) -> Option<[[f64; 2]; 2]> {
    // Try CD matrix form first
    if let (Some(c11), Some(c12), Some(c21), Some(c22)) = (
        header.get("CD1_1"),
        header.get("CD1_2"),
        header.get("CD2_1"),
        header.get("CD2_2"),
    ) {
        let c11: f64 = c11.parse().ok()?;
        let c12: f64 = c12.parse().ok()?;
        let c21: f64 = c21.parse().ok()?;
        let c22: f64 = c22.parse().ok()?;
        return Some([[c11, c12], [c21, c22]]);
    }

    // Try CDELT + PC matrix
    let cdelt1: f64 = header.get("CDELT1")?.parse().ok()?;
    let cdelt2: f64 = header.get("CDELT2")?.parse().ok()?;

    if let (Some(p11), Some(p12), Some(p21), Some(p22)) = (
        header.get("PC1_1"),
        header.get("PC1_2"),
        header.get("PC2_1"),
        header.get("PC2_2"),
    ) {
        let p11: f64 = p11.parse().ok()?;
        let p12: f64 = p12.parse().ok()?;
        let p21: f64 = p21.parse().ok()?;
        let p22: f64 = p22.parse().ok()?;
        return Some([[cdelt1 * p11, cdelt1 * p12], [cdelt2 * p21, cdelt2 * p22]]);
    }

    // Legacy CROTA2 form
    if let Some(crota2_str) = header.get("CROTA2") {
        let crota2: f64 = crota2_str.parse().ok()?;
        let cos_r = crota2.to_radians().cos();
        let sin_r = crota2.to_radians().sin();
        return Some([
            [cdelt1 * cos_r, -cdelt2 * sin_r],
            [cdelt1 * sin_r,  cdelt2 * cos_r],
        ]);
    }

    // Simple CDELT only (no rotation)
    Some([[cdelt1, 0.0], [0.0, cdelt2]])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_header() -> HashMap<String, String> {
        let mut h = HashMap::new();
        h.insert("CRPIX1".into(), "256.5".into()); // 1-based
        h.insert("CRPIX2".into(), "256.5".into());
        h.insert("CRVAL1".into(), "180.0".into()); // RA = 180°
        h.insert("CRVAL2".into(), "0.0".into());   // Dec = 0°
        h.insert("CDELT1".into(), "-0.000277778".into()); // 1 arcsec/pixel
        h.insert("CDELT2".into(), "0.000277778".into());
        h.insert("CTYPE1".into(), "RA---TAN".into());
        h.insert("CTYPE2".into(), "DEC--TAN".into());
        h
    }

    #[test]
    fn test_parse_wcs() {
        let h = test_header();
        let wcs = Wcs::from_header(&h);
        assert!(wcs.is_some());
    }

    #[test]
    fn test_reference_pixel_maps_to_crval() {
        let h = test_header();
        let wcs = Wcs::from_header(&h).unwrap();
        // crpix1=256.5 → 0-based = 255.5
        let (ra, dec) = wcs.pixel_to_world(255.5, 255.5).unwrap();
        assert!((ra - 180.0).abs() < 1e-6, "RA mismatch: {ra}");
        assert!(dec.abs() < 1e-6, "Dec mismatch: {dec}");
    }

    #[test]
    fn test_round_trip() {
        let h = test_header();
        let wcs = Wcs::from_header(&h).unwrap();
        let (ra, dec) = wcs.pixel_to_world(300.0, 200.0).unwrap();
        let (px, py) = wcs.world_to_pixel(ra, dec).unwrap();
        assert!((px - 300.0).abs() < 1e-6);
        assert!((py - 200.0).abs() < 1e-6);
    }

    #[test]
    fn test_format_ra() {
        // 180° = 12h 00m 00.00s
        let s = Wcs::format_ra(180.0);
        assert_eq!(s, "12 00 00.00");
    }

    #[test]
    fn test_format_dec_positive() {
        // 45.5° = +45° 30' 00.0"
        let s = Wcs::format_dec(45.5);
        assert_eq!(s, "+45 30 00.0");
    }

    #[test]
    fn test_format_dec_negative() {
        let s = Wcs::format_dec(-10.25);
        assert_eq!(s, "-10 15 00.0");
    }

    #[test]
    fn test_cd_matrix_form() {
        let mut h = HashMap::new();
        h.insert("CRPIX1".into(), "1.0".into());
        h.insert("CRPIX2".into(), "1.0".into());
        h.insert("CRVAL1".into(), "0.0".into());
        h.insert("CRVAL2".into(), "0.0".into());
        h.insert("CD1_1".into(), "-0.001".into());
        h.insert("CD1_2".into(), "0.0".into());
        h.insert("CD2_1".into(), "0.0".into());
        h.insert("CD2_2".into(), "0.001".into());
        h.insert("CTYPE1".into(), "RA---TAN".into());
        h.insert("CTYPE2".into(), "DEC--TAN".into());
        let wcs = Wcs::from_header(&h).unwrap();
        let (ra, dec) = wcs.pixel_to_world(0.0, 0.0).unwrap();
        assert!((ra - 0.0).abs() < 1e-6);
        assert!(dec.abs() < 1e-6);
    }
}

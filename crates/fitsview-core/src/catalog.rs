use std::collections::HashMap;

/// A single astronomical source from a catalog query.
#[derive(Clone, Debug)]
pub struct CatalogSource {
    pub ra: f64,
    pub dec: f64,
    pub magnitude: Option<f32>,
    pub object_type: Option<String>,
    pub name: Option<String>,
    pub extra: HashMap<String, String>,
}

/// Query parameters for a cone search.
#[derive(Clone, Debug)]
pub struct ConeSearch {
    pub ra_deg: f64,
    pub dec_deg: f64,
    pub radius_arcmin: f64,
    pub max_results: usize,
}

#[cfg(feature = "network")]
pub mod remote {
    use super::*;
    use anyhow::Context;

    const TIMEOUT_SECS: u64 = 15;

    fn build_client() -> anyhow::Result<reqwest::blocking::Client> {
        reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
            .user_agent("fits-view/0.1")
            .build()
            .context("build HTTP client")
    }

    /// Query SIMBAD via TAP ADQL for objects within the search cone.
    pub fn query_simbad(search: &ConeSearch) -> anyhow::Result<Vec<CatalogSource>> {
        let adql = format!(
            "SELECT TOP {max} main_id, otype_txt, ra, dec, V \
             FROM basic \
             JOIN allfluxes ON oid = allfluxes.oidref \
             WHERE CONTAINS(POINT('ICRS', ra, dec), CIRCLE('ICRS', {ra}, {dec}, {r})) = 1",
            max = search.max_results,
            ra = search.ra_deg,
            dec = search.dec_deg,
            r = search.radius_arcmin / 60.0,
        );

        let client = build_client()?;
        let resp = client
            .get("https://simbad.u-strasbg.fr/simbad/tap/sync")
            .query(&[
                ("REQUEST", "doQuery"),
                ("LANG", "ADQL"),
                ("FORMAT", "json"),
                ("QUERY", adql.as_str()),
            ])
            .send()
            .context("SIMBAD TAP request")?;

        let json: serde_json::Value = resp.json().context("parse SIMBAD JSON")?;
        parse_tap_json(&json, "main_id", "otype_txt", "V")
    }

    /// Query VizieR via Simple Cone Search for a named catalog.
    pub fn query_vizier(
        catalog_id: &str,
        search: &ConeSearch,
    ) -> anyhow::Result<Vec<CatalogSource>> {
        let url = format!(
            "https://vizier.u-strasbg.fr/viz-bin/conesearch/{catalog}",
            catalog = catalog_id
        );

        let client = build_client()?;
        let resp = client
            .get(&url)
            .query(&[
                ("RA", search.ra_deg.to_string()),
                ("DEC", search.dec_deg.to_string()),
                ("SR", (search.radius_arcmin / 60.0).to_string()),
                ("MAXREC", search.max_results.to_string()),
                ("FORMAT", "json".to_owned()),
            ])
            .send()
            .context("VizieR cone search")?;

        let json: serde_json::Value = resp.json().context("parse VizieR JSON")?;
        // VizieR TAP-like JSON: try same parser
        parse_tap_json(&json, "_Name", "_Type", "Vmag")
    }

    /// Query Gaia DR3 via ESA TAP.
    pub fn query_gaia_dr3(search: &ConeSearch) -> anyhow::Result<Vec<CatalogSource>> {
        let adql = format!(
            "SELECT TOP {max} source_id, ra, dec, phot_g_mean_mag \
             FROM gaiadr3.gaia_source \
             WHERE CONTAINS(POINT('ICRS', ra, dec), CIRCLE('ICRS', {ra}, {dec}, {r})) = 1",
            max = search.max_results,
            ra = search.ra_deg,
            dec = search.dec_deg,
            r = search.radius_arcmin / 60.0,
        );

        let client = build_client()?;
        let resp = client
            .get("https://gea.esac.esa.int/tap-server/tap/sync")
            .query(&[
                ("REQUEST", "doQuery"),
                ("LANG", "ADQL"),
                ("FORMAT", "json"),
                ("QUERY", adql.as_str()),
            ])
            .send()
            .context("Gaia DR3 TAP request")?;

        let json: serde_json::Value = resp.json().context("parse Gaia JSON")?;
        parse_tap_json(&json, "source_id", "", "phot_g_mean_mag")
    }

    fn parse_tap_json(
        json: &serde_json::Value,
        name_col: &str,
        type_col: &str,
        mag_col: &str,
    ) -> anyhow::Result<Vec<CatalogSource>> {
        let metadata = json["metadata"].as_array();
        let data = json["data"].as_array().context("missing data array")?;

        // Build column index map from metadata
        let col_idx: HashMap<String, usize> = if let Some(meta) = metadata {
            meta.iter().enumerate().filter_map(|(i, m)| {
                m["name"].as_str().map(|n| (n.to_owned(), i))
            }).collect()
        } else {
            HashMap::new()
        };

        let ra_col = col_idx.get("ra").or_else(|| col_idx.get("RA")).copied().unwrap_or(0);
        let dec_col = col_idx.get("dec").or_else(|| col_idx.get("DEC")).copied().unwrap_or(1);
        let name_idx = col_idx.get(name_col).copied();
        let type_idx = col_idx.get(type_col).copied();
        let mag_idx = col_idx.get(mag_col).copied();

        let mut sources = Vec::with_capacity(data.len());
        for row in data {
            let arr = match row.as_array() {
                Some(a) => a,
                None => continue,
            };
            let ra = arr.get(ra_col).and_then(|v| v.as_f64()).unwrap_or(0.0);
            let dec = arr.get(dec_col).and_then(|v| v.as_f64()).unwrap_or(0.0);
            let name = name_idx.and_then(|i| arr.get(i)).and_then(|v| v.as_str()).map(str::to_owned);
            let object_type = type_idx.and_then(|i| arr.get(i)).and_then(|v| v.as_str()).map(str::to_owned);
            let magnitude = mag_idx.and_then(|i| arr.get(i)).and_then(|v| v.as_f64()).map(|m| m as f32);
            sources.push(CatalogSource { ra, dec, magnitude, object_type, name, extra: HashMap::new() });
        }

        Ok(sources)
    }
}

/// Parse a local VOTable XML file into a list of catalog sources.
pub fn parse_votable(xml: &str) -> anyhow::Result<Vec<CatalogSource>> {
    let mut sources = Vec::new();

    // Minimal VOTable parser: find TABLEDATA rows
    let mut ra_col = 0usize;
    let mut dec_col = 1usize;
    let mut col_count = 0usize;
    let mut col_names: Vec<String> = Vec::new();

    for line in xml.lines() {
        let line = line.trim();
        if line.starts_with("<FIELD") {
            // Extract name attribute
            let name = extract_attr(line, "name").unwrap_or_else(|| format!("col{col_count}"));
            let name_lower = name.to_lowercase();
            if name_lower.contains("ra") && !name_lower.contains("dec") {
                ra_col = col_count;
            }
            if name_lower.contains("dec") {
                dec_col = col_count;
            }
            col_names.push(name);
            col_count += 1;
        } else if line.starts_with("<TR>") || line.contains("<TR>") {
            // collect TD values in next lines — simple approach: scan for <TD> tags
            let tds: Vec<&str> = line.split("<TD>")
                .skip(1)
                .filter_map(|s| s.split("</TD>").next())
                .collect();
            if tds.len() >= 2 {
                let ra = tds.get(ra_col).and_then(|s| s.trim().parse::<f64>().ok()).unwrap_or(0.0);
                let dec = tds.get(dec_col).and_then(|s| s.trim().parse::<f64>().ok()).unwrap_or(0.0);
                let mut extra = HashMap::new();
                for (i, td) in tds.iter().enumerate() {
                    if i != ra_col && i != dec_col {
                        if let Some(name) = col_names.get(i) {
                            extra.insert(name.clone(), td.trim().to_owned());
                        }
                    }
                }
                sources.push(CatalogSource { ra, dec, magnitude: None, object_type: None, name: None, extra });
            }
        }
    }

    Ok(sources)
}

fn extract_attr(s: &str, attr: &str) -> Option<String> {
    let key = format!("{attr}=\"");
    let start = s.find(&key)? + key.len();
    let end = s[start..].find('"')?;
    Some(s[start..start + end].to_owned())
}

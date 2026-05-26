use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

use fitsrs::{hdu::data::bintable::ColumnId, Fits, HDU};
use rayon::prelude::*;

pub struct EventImage {
    pub data: Vec<f32>,
    pub width: usize,
    pub height: usize,
    pub bin_size: f64,
    pub x_col: String,
    pub y_col: String,
    pub total_events: usize,
    pub header: HashMap<String, String>,
}

pub struct EventFilter {
    pub energy_range: Option<(f32, f32)>,
    pub time_range: Option<(f64, f64)>,
}

/// Bin BINTABLE events into a 2D count map.
/// `hdu_index` is the 0-based absolute HDU index to look for a BINTABLE.
/// Searches all BINTABLEs starting from index 0; `hdu_index` counts only BINTABLE HDUs.
pub fn bin_events(path: &Path, hdu_index: usize, bin_size: f64) -> anyhow::Result<EventImage> {
    let reader = BufReader::new(File::open(path)?);
    let mut hdu_list = Fits::from_reader(reader);

    let mut bintable_count = 0usize;
    let bintable_hdu = loop {
        match hdu_list.next() {
            Some(Ok(HDU::XBinaryTable(h))) => {
                if bintable_count == hdu_index {
                    break h;
                }
                bintable_count += 1;
            }
            Some(Ok(_)) => {}
            Some(Err(e)) => return Err(e.into()),
            None => anyhow::bail!(
                "No BINTABLE HDU at index {hdu_index} (found {bintable_count} total)"
            ),
        }
    };

    // Extract header keywords
    let mut header = HashMap::new();
    for (key, val) in bintable_hdu.get_header().iter() {
        use fitsrs::card::Value;
        let s = match val {
            Value::Integer { value: v, .. } => v.to_string(),
            Value::Float { value: v, .. } => format!("{v:.10}"),
            Value::Logical { value: v, .. } => {
                if *v { "T" } else { "F" }.to_owned()
            }
            Value::String { value: v, .. } => v.clone(),
            Value::Undefined => String::new(),
            Value::Invalid(s) => s.clone(),
        };
        header.insert(key.to_owned(), s);
    }

    let xt = bintable_hdu.get_header().get_xtension();
    let num_rows = xt.get_num_rows();

    // Discover spatial columns from TTYPE keywords (case-insensitive)
    let num_fields: usize = header
        .get("TFIELDS")
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);

    let col_names: Vec<String> = (1..=num_fields)
        .map(|i| {
            header
                .get(&format!("TTYPE{i}"))
                .map(|s| s.trim().to_uppercase())
                .unwrap_or_default()
        })
        .collect();

    let candidates = [("X", "Y"), ("DETX", "DETY"), ("RA", "DEC"), ("RAWX", "RAWY")];
    let (x_idx, y_idx, x_col, y_col) = candidates
        .iter()
        .find_map(|(xn, yn)| {
            let xi = col_names.iter().position(|n| n == *xn)?;
            let yi = col_names.iter().position(|n| n == *yn)?;
            let orig_x = header
                .get(&format!("TTYPE{}", xi + 1))
                .map(|s| s.trim().to_owned())
                .unwrap_or_else(|| xn.to_string());
            let orig_y = header
                .get(&format!("TTYPE{}", yi + 1))
                .map(|s| s.trim().to_owned())
                .unwrap_or_else(|| yn.to_string());
            Some((xi, yi, orig_x, orig_y))
        })
        .ok_or_else(|| {
            let found = col_names
                .iter()
                .filter(|s| !s.is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            anyhow::anyhow!(
                "No spatial columns found in BINTABLE. \
                 Looking for (X,Y), (DETX,DETY), (RA,DEC), or (RAWX,RAWY). \
                 Found: [{found}]"
            )
        })?;

    // Read all rows collecting (x, y) coordinate pairs
    let mut xs: Vec<f64> = Vec::with_capacity(num_rows);
    let mut ys: Vec<f64> = Vec::with_capacity(num_rows);

    {
        let bin_data = hdu_list.get_data(&bintable_hdu);
        let mut table_data = bin_data.table_data();
        let cols = [ColumnId::Index(x_idx), ColumnId::Index(y_idx)];
        table_data.select_fields(&cols);

        for row in table_data.row_iter() {
            let mut xv = None;
            let mut yv = None;
            for dv in row.iter() {
                if let Some((col, val)) = datavalue_to_f64(dv) {
                    if col == x_idx {
                        xv = Some(val);
                    }
                    if col == y_idx {
                        yv = Some(val);
                    }
                }
            }
            if let (Some(x), Some(y)) = (xv, yv) {
                if x.is_finite() && y.is_finite() {
                    xs.push(x);
                    ys.push(y);
                }
            }
        }
    }

    let total_events = xs.len();
    anyhow::ensure!(total_events > 0, "No events with valid spatial coordinates found");

    // Parallel min/max
    let (x_min, x_max) = xs
        .par_iter()
        .map(|&v| (v, v))
        .reduce(
            || (f64::INFINITY, f64::NEG_INFINITY),
            |(a0, a1), (b0, b1)| (a0.min(b0), a1.max(b1)),
        );
    let (y_min, y_max) = ys
        .par_iter()
        .map(|&v| (v, v))
        .reduce(
            || (f64::INFINITY, f64::NEG_INFINITY),
            |(a0, a1), (b0, b1)| (a0.min(b0), a1.max(b1)),
        );

    let raw_w = ((x_max - x_min) / bin_size).ceil() as usize + 1;
    let raw_h = ((y_max - y_min) / bin_size).ceil() as usize + 1;
    let width = raw_w.clamp(1, 4096);
    let height = raw_h.clamp(1, 4096);

    // Parallel atomic binning
    let histogram: Vec<AtomicU32> = (0..width * height).map(|_| AtomicU32::new(0)).collect();
    xs.par_iter().zip(ys.par_iter()).for_each(|(x, y)| {
        let px = ((*x - x_min) / bin_size) as usize;
        let py = ((*y - y_min) / bin_size) as usize;
        if px < width && py < height {
            histogram[py * width + px].fetch_add(1, Ordering::Relaxed);
        }
    });

    let data: Vec<f32> = histogram
        .iter()
        .map(|a| a.load(Ordering::Relaxed) as f32)
        .collect();

    Ok(EventImage {
        data,
        width,
        height,
        bin_size,
        x_col,
        y_col,
        total_events,
        header,
    })
}

/// Extract a (column_index, f64_value) pair from a scalar DataValue (idx==0 only).
fn datavalue_to_f64(
    dv: &fitsrs::DataValue,
) -> Option<(usize, f64)> {
    use fitsrs::hdu::data::bintable::ColumnId;
    use fitsrs::DataValue;
    match dv {
        DataValue::UnsignedByte { value, column: ColumnId::Index(i), idx: 0 } => {
            Some((*i, *value as f64))
        }
        DataValue::Short { value, column: ColumnId::Index(i), idx: 0 } => {
            Some((*i, *value as f64))
        }
        DataValue::Integer { value, column: ColumnId::Index(i), idx: 0 } => {
            Some((*i, *value as f64))
        }
        DataValue::Long { value, column: ColumnId::Index(i), idx: 0 } => {
            Some((*i, *value as f64))
        }
        DataValue::Float { value, column: ColumnId::Index(i), idx: 0 } => {
            Some((*i, *value as f64))
        }
        DataValue::Double { value, column: ColumnId::Index(i), idx: 0 } => {
            Some((*i, *value))
        }
        _ => None,
    }
}

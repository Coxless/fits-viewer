use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Cursor, Read};
use std::path::Path;

use flate2::read::MultiGzDecoder;
use fitsrs::{hdu::data::image::Pixels, Fits, HDU};

pub struct FitsImage {
    pub data: Vec<f32>,
    pub width: usize,
    pub height: usize,
    pub header: HashMap<String, String>,
    pub bitpix: i32,
}

/// Extract image data + header from an image HDU via a concrete Fits reader.
/// Macro avoids complex FitsRead trait bound projections over associated types.
macro_rules! extract_image {
    ($hdu:expr, $hdu_list:expr) => {{
        let hdu = $hdu;
        let hdu_list = $hdu_list;
        let xt = hdu.get_header().get_xtension();
        let naxis = xt.get_naxis();
        anyhow::ensure!(
            naxis.len() >= 2,
            "FITS image must have at least 2 axes, found {}",
            naxis.len()
        );
        let width = naxis[0] as usize;
        let height = naxis[1] as usize;
        let bitpix = xt.get_bitpix() as i32;
        let mut header = HashMap::new();
        for (key, val) in hdu.get_header().iter() {
            header.insert(key.to_owned(), value_to_string(val));
        }
        let img_data = hdu_list.get_data(&hdu);
        let data: Vec<f32> = match img_data.pixels() {
            Pixels::U8(it) => it.map(|v| v as f32).collect(),
            Pixels::I16(it) => it.map(|v| v as f32).collect(),
            Pixels::I32(it) => it.map(|v| v as f32).collect(),
            Pixels::I64(it) => it.map(|v| v as f32).collect(),
            Pixels::F32(it) => it.collect(),
            Pixels::F64(it) => it.map(|v| v as f32).collect(),
        };
        anyhow::Ok(FitsImage { data, width, height, header, bitpix })
    }};
}

/// Walk HDUs to find the first image with pixel data; extract it.
macro_rules! load_first_image_hdu {
    ($hdu_list:expr) => {{
        let hdu_list = $hdu_list;
        let mut has_table = false;
        let hdu = loop {
            match hdu_list.next() {
                Some(Ok(HDU::Primary(h))) | Some(Ok(HDU::XImage(h))) => {
                    if h.get_header().get_xtension().get_num_pixels() > 0 {
                        break h;
                    }
                }
                Some(Ok(HDU::XBinaryTable(_))) | Some(Ok(HDU::XASCIITable(_))) => {
                    has_table = true;
                }
                Some(Err(_)) | None => {
                    if has_table {
                        anyhow::bail!("FITS file contains table data (BINTABLE/TABLE) but no image HDU.");
                    }
                    anyhow::bail!("No image data found in FITS file");
                }
            }
        };
        extract_image!(hdu, hdu_list)
    }};
}

#[derive(Debug, Clone, PartialEq)]
pub enum HduType {
    Image,
    BinTable,
    AsciiTable,
}

#[derive(Debug, Clone)]
pub struct HduInfo {
    pub index: usize,
    pub hdu_type: HduType,
    pub name: String,
    pub naxis: Vec<usize>,
}

/// List all HDUs in a FITS file without reading pixel data.
pub fn list_hdus(path: &Path) -> anyhow::Result<Vec<HduInfo>> {
    let reader = BufReader::new(File::open(path)?);
    let hdu_list = Fits::from_reader(reader);
    let mut result = Vec::new();

    for (idx, hdu_result) in hdu_list.enumerate() {
        let hdu = match hdu_result {
            Ok(h) => h,
            Err(_) => break, // fitsrs can error at EOF after the last HDU
        };
        match hdu {
            HDU::Primary(h) | HDU::XImage(h) => {
                let xt = h.get_header().get_xtension();
                let name = extract_header_str(h.get_header().iter(), "EXTNAME");
                let naxis: Vec<usize> = xt.get_naxis().iter().map(|&n| n as usize).collect();
                result.push(HduInfo { index: idx, hdu_type: HduType::Image, name, naxis });
            }
            HDU::XBinaryTable(h) => {
                let xt = h.get_header().get_xtension();
                let name = extract_header_str(h.get_header().iter(), "EXTNAME");
                let naxis = vec![xt.get_num_rows()];
                result.push(HduInfo { index: idx, hdu_type: HduType::BinTable, name, naxis });
            }
            HDU::XASCIITable(h) => {
                let name = extract_header_str(h.get_header().iter(), "EXTNAME");
                result.push(HduInfo {
                    index: idx,
                    hdu_type: HduType::AsciiTable,
                    name,
                    naxis: vec![],
                });
            }
        }
    }

    Ok(result)
}

fn extract_header_str<'a>(
    iter: impl Iterator<Item = (&'a str, &'a fitsrs::card::Value)>,
    key: &str,
) -> String {
    for (k, v) in iter {
        if k == key {
            if let fitsrs::card::Value::String { value, .. } = v {
                return value.trim().to_owned();
            }
        }
    }
    String::new()
}

/// Load the image HDU at absolute position `hdu_index` (counting all HDU types from 0).
pub fn load_fits_hdu(path: &Path, hdu_index: usize) -> anyhow::Result<FitsImage> {
    let reader = BufReader::new(File::open(path)?);
    let mut hdu_list = Fits::from_reader(reader);
    let mut cur = 0usize;

    loop {
        match hdu_list.next() {
            Some(Ok(HDU::Primary(h))) | Some(Ok(HDU::XImage(h))) => {
                if cur == hdu_index {
                    anyhow::ensure!(
                        h.get_header().get_xtension().get_num_pixels() > 0,
                        "HDU {hdu_index} is an image with no pixel data (NAXIS=0)"
                    );
                    return extract_image!(h, &mut hdu_list);
                }
            }
            Some(Ok(HDU::XBinaryTable(_))) | Some(Ok(HDU::XASCIITable(_))) => {
                if cur == hdu_index {
                    anyhow::bail!("HDU {hdu_index} is a table, not an image");
                }
            }
            Some(Err(e)) => return Err(e.into()),
            None => anyhow::bail!("HDU index {hdu_index} not found in file"),
        }
        cur += 1;
    }
}

fn value_to_string(val: &fitsrs::card::Value) -> String {
    use fitsrs::card::Value;
    match val {
        Value::Integer { value: v, .. } => v.to_string(),
        Value::Float { value: v, .. } => format!("{v:.10}"),
        Value::Logical { value: v, .. } => if *v { "T" } else { "F" }.to_owned(),
        Value::String { value: v, .. } => v.clone(),
        Value::Undefined => String::new(),
        Value::Invalid(s) => s.clone(),
    }
}

pub fn load_fits(path: &Path) -> anyhow::Result<FitsImage> {
    let path_str = path.to_string_lossy();

    // Gzip-compressed FITS: decompress into memory, then parse normally
    if path_str.ends_with(".gz") || path_str.ends_with(".GZ") {
        let file = File::open(path)?;
        let mut dec = MultiGzDecoder::new(BufReader::new(file));
        let mut bytes = Vec::new();
        dec.read_to_end(&mut bytes)?;
        let mut hdu_list = Fits::from_reader(BufReader::new(Cursor::new(bytes)));
        return load_first_image_hdu!(&mut hdu_list);
    }

    // Tile-compressed FITS (ZTILE / RICE / GZIP tiles in BINTABLE)
    if crate::compressed_fits::is_tile_compressed(path)? {
        return crate::compressed_fits::load_compressed(path);
    }

    let mut hdu_list = Fits::from_reader(BufReader::new(File::open(path)?));
    load_first_image_hdu!(&mut hdu_list)
}


#[cfg(test)]
mod tests {
    use super::*;

    fn sample_path() -> std::path::PathBuf {
        let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest.join("../../tests/fixtures/sample.fits")
    }

    #[test]
    fn test_load_sample_fits() {
        let img = load_fits(&sample_path()).expect("should load sample.fits");
        assert!(img.width > 0);
        assert!(img.height > 0);
        assert_eq!(img.data.len(), img.width * img.height);
        assert!(img.header.contains_key("SIMPLE"));
    }

    #[test]
    fn test_nonexistent_file() {
        assert!(load_fits(Path::new("/nonexistent/file.fits")).is_err());
    }
}

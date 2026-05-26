use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use fitsrs::{hdu::data::image::Pixels, Fits, HDU};

pub struct FitsImage {
    pub data: Vec<f32>,
    pub width: usize,
    pub height: usize,
    pub header: HashMap<String, String>,
    pub bitpix: i32,
}

pub fn load_fits(path: &Path) -> anyhow::Result<FitsImage> {
    let reader = BufReader::new(File::open(path)?);
    let mut hdu_list = Fits::from_reader(reader);

    // Find the first image HDU with actual pixel data (Primary HDU may have NAXIS=0)
    let hdu = loop {
        match hdu_list.next() {
            Some(Ok(HDU::Primary(h))) | Some(Ok(HDU::XImage(h))) => {
                if h.get_header().get_xtension().get_num_pixels() > 0 {
                    break h;
                }
            }
            Some(Ok(_)) => {}
            Some(Err(e)) => return Err(e.into()),
            None => anyhow::bail!("No image data found in FITS file"),
        }
    };

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

    // Header<Image> derefs to ValueMap; .iter() yields (&str, &Value)
    let mut header = HashMap::new();
    for (key, val) in hdu.get_header().iter() {
        use fitsrs::card::Value;
        let s = match val {
            Value::Integer { value: v, .. } => v.to_string(),
            Value::Float { value: v, .. } => format!("{v:.10}"),
            Value::Logical { value: v, .. } => if *v { "T" } else { "F" }.to_owned(),
            Value::String { value: v, .. } => v.clone(),
            Value::Undefined => String::new(),
            Value::Invalid(s) => s.clone(),
        };
        header.insert(key.to_owned(), s);
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

    Ok(FitsImage { data, width, height, header, bitpix })
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

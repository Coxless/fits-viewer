use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use fitsrs::hdu::header::Bitpix;
use fitsrs::{Fits, HDU};
use memmap2::Mmap;

pub struct MmapFitsImage {
    mmap: Mmap,
    pub data_offset: u64,
    pub width: usize,
    pub height: usize,
    pub bitpix: Bitpix,
    pub header: HashMap<String, String>,
}

impl MmapFitsImage {
    /// Open a FITS file and memory-map the image HDU at `hdu_index`.
    /// Uses fitsrs to locate the data block offset, then maps the whole file.
    pub fn open(path: &Path, hdu_index: usize) -> anyhow::Result<Self> {
        // Walk HDUs with fitsrs to get dimensions and data offset.
        let reader = BufReader::new(File::open(path)?);
        let mut hdu_list = Fits::from_reader(reader);
        let mut cur = 0usize;

        let (data_offset, width, height, bitpix, header) = loop {
            match hdu_list.next() {
                Some(Ok(HDU::Primary(h))) | Some(Ok(HDU::XImage(h))) => {
                    if cur == hdu_index {
                        let xt = h.get_header().get_xtension();
                        let naxis = xt.get_naxis();
                        anyhow::ensure!(
                            naxis.len() >= 2,
                            "HDU {hdu_index} has fewer than 2 axes"
                        );
                        let width = naxis[0] as usize;
                        let height = naxis[1] as usize;
                        let bitpix = xt.get_bitpix();
                        let data_offset = h.get_data_unit_byte_offset();
                        let mut hdr = HashMap::new();
                        for (k, v) in h.get_header().iter() {
                            hdr.insert(k.to_owned(), value_to_string(v));
                        }
                        break (data_offset, width, height, bitpix, hdr);
                    }
                }
                Some(Ok(HDU::XBinaryTable(_))) | Some(Ok(HDU::XASCIITable(_))) => {
                    if cur == hdu_index {
                        anyhow::bail!("HDU {hdu_index} is a table, not an image");
                    }
                }
                Some(Err(e)) => return Err(e.into()),
                None => anyhow::bail!("HDU index {hdu_index} not found"),
            }
            cur += 1;
        };
        drop(hdu_list);

        // Map the file independently.
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };

        Ok(Self { mmap, data_offset, width, height, bitpix, header })
    }

    /// Actual pixel dimensions of tile `(tx, ty)` after edge clipping.
    pub fn tile_dims(&self, tx: usize, ty: usize, tile_size: usize) -> (usize, usize) {
        let w = (self.width - tx * tile_size).min(tile_size);
        let h = (self.height - ty * tile_size).min(tile_size);
        (w, h)
    }

    /// Number of tile columns at the given tile size.
    pub fn tiles_x(&self, tile_size: usize) -> usize {
        self.width.div_ceil(tile_size)
    }

    /// Number of tile rows at the given tile size.
    pub fn tiles_y(&self, tile_size: usize) -> usize {
        self.height.div_ceil(tile_size)
    }

    /// Read tile `(tx, ty)` as `Vec<f32>` of length `tile_w * tile_h`.
    /// Pixel data is big-endian in FITS; we convert to native f32 here.
    pub fn read_tile(&self, tx: usize, ty: usize, tile_size: usize) -> anyhow::Result<Vec<f32>> {
        let col0 = tx * tile_size;
        let row0 = ty * tile_size;
        anyhow::ensure!(col0 < self.width && row0 < self.height, "tile out of bounds");

        let (tw, th) = self.tile_dims(tx, ty, tile_size);
        let bpp = self.bitpix.byte_size();
        let mut out = Vec::with_capacity(tw * th);

        for row in row0..row0 + th {
            let byte_start = self.data_offset as usize + (row * self.width + col0) * bpp;
            let byte_end = byte_start + tw * bpp;
            let slice = &self.mmap[byte_start..byte_end];
            decode_row(slice, self.bitpix, tw, &mut out);
        }

        Ok(out)
    }
}

fn decode_row(slice: &[u8], bitpix: Bitpix, n: usize, out: &mut Vec<f32>) {
    match bitpix {
        Bitpix::U8 => {
            for i in 0..n {
                out.push(slice[i] as f32);
            }
        }
        Bitpix::I16 => {
            for i in 0..n {
                let b = &slice[i * 2..i * 2 + 2];
                out.push(i16::from_be_bytes([b[0], b[1]]) as f32);
            }
        }
        Bitpix::I32 => {
            for i in 0..n {
                let b = &slice[i * 4..i * 4 + 4];
                out.push(i32::from_be_bytes([b[0], b[1], b[2], b[3]]) as f32);
            }
        }
        Bitpix::I64 => {
            for i in 0..n {
                let b = &slice[i * 8..i * 8 + 8];
                out.push(i64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
                    as f32);
            }
        }
        Bitpix::F32 => {
            for i in 0..n {
                let b = &slice[i * 4..i * 4 + 4];
                out.push(f32::from_be_bytes([b[0], b[1], b[2], b[3]]));
            }
        }
        Bitpix::F64 => {
            for i in 0..n {
                let b = &slice[i * 8..i * 8 + 8];
                out.push(
                    f64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]) as f32,
                );
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fits_reader::load_fits;

    const SAMPLE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/sample.fits"
    );

    #[test]
    fn test_mmap_open() {
        let mmap = MmapFitsImage::open(Path::new(SAMPLE), 0).expect("open");
        assert!(mmap.width > 0);
        assert!(mmap.height > 0);
    }

    #[test]
    fn test_mmap_tile_matches_full_load() {
        let mmap = MmapFitsImage::open(Path::new(SAMPLE), 0).expect("open mmap");
        let full = load_fits(Path::new(SAMPLE)).expect("load_fits");

        assert_eq!(mmap.width, full.width);
        assert_eq!(mmap.height, full.height);

        let tile_size = 512.min(mmap.width).min(mmap.height);
        let tile = mmap.read_tile(0, 0, tile_size).expect("read_tile");

        let tw = tile_size.min(full.width);
        let th = tile_size.min(full.height);

        for row in 0..th {
            for col in 0..tw {
                let got = tile[row * tw + col];
                let expected = full.data[row * full.width + col];
                assert!(
                    (got - expected).abs() < 1e-3,
                    "mismatch at ({col},{row}): got {got}, expected {expected}"
                );
            }
        }
    }
}

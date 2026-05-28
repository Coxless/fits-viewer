use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use flate2::read::GzDecoder;
use fitsrs::{Fits, HDU};
use rayon::prelude::*;

use crate::fits_reader::FitsImage;

/// Load a tile-compressed FITS file (ZIMAGE=T in a BINTABLE extension).
pub fn load_compressed(path: &Path) -> anyhow::Result<FitsImage> {
    let info = find_compressed_info(path)?;
    let tiles_x = info.znaxis1.div_ceil(info.ztile1);
    let image_data = decompress_all_tiles(path, &info, tiles_x)?;
    Ok(FitsImage { data: image_data, width: info.znaxis1, height: info.znaxis2, header: info.header, bitpix: info.zbitpix })
}

/// Return true if the file contains a tile-compressed image (ZIMAGE=T in any BINTABLE HDU).
pub fn is_tile_compressed(path: &Path) -> anyhow::Result<bool> {
    let reader = BufReader::new(File::open(path)?);
    let hdu_list = Fits::from_reader(reader);
    for hdu_res in hdu_list {
        let Ok(hdu) = hdu_res else { continue };
        if let HDU::XBinaryTable(h) = hdu {
            let found = h.get_header().iter().any(|(k, v)| {
                k == "ZIMAGE"
                    && matches!(v, fitsrs::card::Value::Logical { value: true, .. })
            });
            if found {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

// ---------------------------------------------------------------------------
// Internal structures

struct CompressedHduInfo {
    znaxis1: usize,
    znaxis2: usize,
    ztile1: usize,
    ztile2: usize,
    zcmptype: String,
    zbitpix: i32,
    nrows: usize,
    row_bytes: usize,
    data_offset: u64,
    comp_col_offset: usize,
    comp_col_bytes: usize,
    is_variable: bool,
    heap_start: u64,
    bytepix: usize,
    block_size: usize,
    header: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// HDU discovery and header parsing

fn find_compressed_info(path: &Path) -> anyhow::Result<CompressedHduInfo> {
    let reader = BufReader::new(File::open(path)?);
    let hdu_list = Fits::from_reader(reader);

    for hdu_res in hdu_list {
        let hdu = hdu_res?;
        if let HDU::XBinaryTable(h) = hdu {
            let is_compressed = h.get_header().iter().any(|(k, v)| {
                k == "ZIMAGE"
                    && matches!(v, fitsrs::card::Value::Logical { value: true, .. })
            });
            if !is_compressed {
                continue;
            }

            let kw = collect_keywords(h.get_header().iter());

            let znaxis1 = parse_kw_usize(&kw, "ZNAXIS1")?;
            let znaxis2 = parse_kw_usize(&kw, "ZNAXIS2")?;
            let ztile1 = kw.get("ZTILE1").and_then(|v| v.parse().ok()).unwrap_or(znaxis1);
            let ztile2 = kw.get("ZTILE2").and_then(|v| v.parse().ok()).unwrap_or(1usize);
            let zcmptype = kw.get("ZCMPTYPE").cloned().unwrap_or_default();
            let zbitpix: i32 = kw.get("ZBITPIX").and_then(|v| v.parse().ok()).unwrap_or(32);

            let naxis1 = parse_kw_usize(&kw, "NAXIS1")?;
            let naxis2 = parse_kw_usize(&kw, "NAXIS2")?;
            let tfields: usize = kw.get("TFIELDS").and_then(|v| v.parse().ok()).unwrap_or(0);

            // Compute column byte offsets
            let mut cursor = 0usize;
            let mut comp_col_offset = 0usize;
            let mut comp_col_bytes = 0usize;
            let mut is_variable = false;

            for ci in 1..=tfields {
                let ttype = kw.get(&format!("TTYPE{ci}")).map(|s| s.trim().to_owned()).unwrap_or_default();
                let tform = kw.get(&format!("TFORM{ci}")).cloned().unwrap_or_default();
                let (col_bytes, var) = parse_tform_bytes(tform.trim());
                if ttype == "COMPRESSED_DATA" {
                    comp_col_offset = cursor;
                    comp_col_bytes = col_bytes;
                    is_variable = var;
                }
                cursor += col_bytes;
            }

            let data_offset = h.get_data_unit_byte_offset();
            // THEAP gives the offset from start of data unit to the heap; default = naxis1*naxis2
            let theap: u64 = kw.get("THEAP").and_then(|v| v.parse().ok())
                .unwrap_or((naxis1 * naxis2) as u64);
            let heap_start = data_offset + theap;

            let bytepix = match zbitpix.abs() {
                8 => 1,
                16 => 2,
                32 | -32 => 4,
                64 | -64 => 8,
                _ => 4,
            };

            // Rice parameters: stored as ZNAME1/ZVAL1 ... ZNAMEn/ZVALn
            let block_size = rice_param(&kw, "BLOCKSIZE").unwrap_or(32);
            let bytepix_rice = rice_param(&kw, "BYTEPIX").unwrap_or(bytepix);

            return Ok(CompressedHduInfo {
                znaxis1, znaxis2, ztile1, ztile2,
                zcmptype: zcmptype.trim().to_uppercase(),
                zbitpix, nrows: naxis2, row_bytes: naxis1,
                data_offset, comp_col_offset, comp_col_bytes, is_variable, heap_start,
                bytepix: bytepix_rice, block_size, header: kw,
            });
        }
    }
    anyhow::bail!("No tile-compressed image (ZIMAGE=T) found in FITS file")
}

fn collect_keywords<'a>(
    iter: impl Iterator<Item = (&'a str, &'a fitsrs::card::Value)>,
) -> HashMap<String, String> {
    iter.map(|(k, v)| (k.to_owned(), value_to_str(v))).collect()
}

fn value_to_str(val: &fitsrs::card::Value) -> String {
    use fitsrs::card::Value;
    match val {
        Value::Integer { value: v, .. } => v.to_string(),
        Value::Float { value: v, .. } => format!("{v:.10}"),
        Value::Logical { value: v, .. } => if *v { "T" } else { "F" }.to_owned(),
        Value::String { value: v, .. } => v.trim().to_owned(),
        Value::Undefined => String::new(),
        Value::Invalid(s) => s.clone(),
    }
}

fn parse_kw_usize(kw: &HashMap<String, String>, key: &str) -> anyhow::Result<usize> {
    kw.get(key)
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| anyhow::anyhow!("Missing or invalid keyword: {key}"))
}

/// Returns (total bytes in row for this column, is_variable_length).
/// For variable-length (1PB/1PJ etc.) the row stores an 8-byte P descriptor.
fn parse_tform_bytes(tform: &str) -> (usize, bool) {
    // Variable-length: '1PB(n)', '1PJ(n)', '1PE(n)', etc.
    if tform.len() >= 2 && &tform[1..2] == "P" {
        return (8, true); // 2 × i32 pointer (count + heap_offset)
    }
    if tform.len() >= 2 && &tform[1..2] == "Q" {
        return (16, true); // 2 × i64 pointer
    }

    // Fixed: 'rT' where r is count, T is type letter
    let (count_str, type_char) = tform.split_at(tform.len().saturating_sub(1));
    let count: usize = if count_str.is_empty() { 1 } else { count_str.parse().unwrap_or(1) };
    let elem = match type_char {
        "B" | "L" | "A" => 1,
        "I" => 2,
        "J" | "E" => 4,
        "K" | "D" => 8,
        "M" => 16,
        "X" => count.div_ceil(8), // X = bits, packed into bytes
        _ => 1,
    };
    let total = if type_char == "X" { elem } else { count * elem };
    (total, false)
}

/// Find a named Rice parameter from ZNAMEn/ZVALn keyword pairs.
fn rice_param(kw: &HashMap<String, String>, name: &str) -> Option<usize> {
    for i in 1..=10 {
        if kw.get(&format!("ZNAME{i}")).map(|s| s.as_str()) == Some(name) {
            return kw.get(&format!("ZVAL{i}")).and_then(|v| v.parse().ok());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tile decompression

fn decompress_all_tiles(
    path: &Path,
    info: &CompressedHduInfo,
    tiles_x: usize,
) -> anyhow::Result<Vec<f32>> {
    let mut file = File::open(path)?;

    // Read table rows
    let table_bytes = {
        file.seek(SeekFrom::Start(info.data_offset))?;
        let mut buf = vec![0u8; info.row_bytes * info.nrows];
        file.read_exact(&mut buf)?;
        buf
    };

    // Read heap (for variable-length columns)
    let heap_bytes: Vec<u8> = if info.is_variable {
        file.seek(SeekFrom::Start(info.heap_start))?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        buf
    } else {
        Vec::new()
    };

    // Decompress each tile in parallel, collecting (row_idx, pixels)
    let results: Vec<anyhow::Result<(usize, Vec<f32>)>> = (0..info.nrows)
        .into_par_iter()
        .map(|row_idx| {
            let row = &table_bytes[row_idx * info.row_bytes..(row_idx + 1) * info.row_bytes];
            let compressed = extract_compressed_bytes(row, info, &heap_bytes)?;

            let ty = row_idx / tiles_x;
            let tx = row_idx % tiles_x;
            let tw = info.ztile1.min(info.znaxis1.saturating_sub(tx * info.ztile1));
            let th = info.ztile2.min(info.znaxis2.saturating_sub(ty * info.ztile2));
            let n_pixels = tw * th;

            let pixels = decompress_tile(compressed, n_pixels, info)?;
            Ok((row_idx, pixels))
        })
        .collect();

    // Assemble into output image
    let mut image = vec![0.0f32; info.znaxis1 * info.znaxis2];
    for r in results {
        let (row_idx, pixels) = r?;
        let ty = row_idx / tiles_x;
        let tx = row_idx % tiles_x;
        let tw = info.ztile1.min(info.znaxis1.saturating_sub(tx * info.ztile1));
        let th = info.ztile2.min(info.znaxis2.saturating_sub(ty * info.ztile2));

        for row in 0..th {
            let src_start = row * tw;
            let dst_start = (ty * info.ztile2 + row) * info.znaxis1 + tx * info.ztile1;
            let dst_end = dst_start + tw;
            if dst_end <= image.len() && src_start + tw <= pixels.len() {
                image[dst_start..dst_end].copy_from_slice(&pixels[src_start..src_start + tw]);
            }
        }
    }

    Ok(image)
}

fn extract_compressed_bytes<'a>(
    row: &'a [u8],
    info: &CompressedHduInfo,
    heap: &'a [u8],
) -> anyhow::Result<&'a [u8]> {
    let col_start = info.comp_col_offset;
    if info.is_variable {
        // P descriptor: count (i32 BE) + heap_offset (i32 BE)
        let p = &row[col_start..col_start + 8];
        let count = i32::from_be_bytes([p[0], p[1], p[2], p[3]]) as usize;
        let off = i32::from_be_bytes([p[4], p[5], p[6], p[7]]) as usize;
        anyhow::ensure!(off + count <= heap.len(), "tile heap offset out of range");
        Ok(&heap[off..off + count])
    } else {
        let end = col_start + info.comp_col_bytes;
        anyhow::ensure!(end <= row.len(), "fixed-length tile column out of range");
        Ok(&row[col_start..end])
    }
}

fn decompress_tile(data: &[u8], n_pixels: usize, info: &CompressedHduInfo) -> anyhow::Result<Vec<f32>> {
    match info.zcmptype.as_str() {
        "GZIP_1" => decompress_gzip1(data, n_pixels, info.zbitpix),
        "GZIP_2" => decompress_gzip2(data, n_pixels, info.zbitpix, info.bytepix),
        "RICE_1" => decompress_rice1(data, n_pixels, info.bytepix, info.block_size, info.zbitpix),
        "PLIO_1" => Err(anyhow::anyhow!("PLIO_1 tile compression not yet supported")),
        other => Err(anyhow::anyhow!("Unknown tile compression type: {other}")),
    }
}

// ---------------------------------------------------------------------------
// GZIP_1 decompression

fn decompress_gzip1(data: &[u8], n_pixels: usize, zbitpix: i32) -> anyhow::Result<Vec<f32>> {
    let mut dec = GzDecoder::new(data);
    let mut raw = Vec::new();
    dec.read_to_end(&mut raw)?;
    bytes_to_f32(&raw, n_pixels, zbitpix)
}

/// GZIP_2: byte-shuffled GZIP. Unshuffle, then decode.
fn decompress_gzip2(data: &[u8], n_pixels: usize, zbitpix: i32, bytepix: usize) -> anyhow::Result<Vec<f32>> {
    let mut dec = GzDecoder::new(data);
    let mut shuffled = Vec::new();
    dec.read_to_end(&mut shuffled)?;

    let total = n_pixels * bytepix;
    anyhow::ensure!(shuffled.len() >= total, "GZIP_2: decompressed data too short");

    // Byte unshuffle: grouped by byte plane
    let mut raw = vec![0u8; total];
    for i in 0..n_pixels {
        for b in 0..bytepix {
            raw[i * bytepix + b] = shuffled[b * n_pixels + i];
        }
    }
    bytes_to_f32(&raw, n_pixels, zbitpix)
}

fn bytes_to_f32(raw: &[u8], n_pixels: usize, zbitpix: i32) -> anyhow::Result<Vec<f32>> {
    let bpp = match zbitpix.abs() { 8 => 1, 16 => 2, 32 => 4, 64 => 8, _ => 4 };
    anyhow::ensure!(raw.len() >= n_pixels * bpp, "decompressed data shorter than expected");
    let out: Vec<f32> = match zbitpix {
        8  => raw.iter().take(n_pixels).map(|&b| b as f32).collect(),
        16 => raw.chunks_exact(2).take(n_pixels).map(|b| i16::from_be_bytes([b[0], b[1]]) as f32).collect(),
        32 => raw.chunks_exact(4).take(n_pixels).map(|b| i32::from_be_bytes([b[0], b[1], b[2], b[3]]) as f32).collect(),
        -32 => raw.chunks_exact(4).take(n_pixels).map(|b| f32::from_be_bytes([b[0], b[1], b[2], b[3]])).collect(),
        64 => raw.chunks_exact(8).take(n_pixels)
            .map(|b| i64::from_be_bytes(b.try_into().unwrap()) as f32).collect(),
        -64 => raw.chunks_exact(8).take(n_pixels)
            .map(|b| f64::from_be_bytes(b.try_into().unwrap()) as f32).collect(),
        _ => anyhow::bail!("Unsupported ZBITPIX: {zbitpix}"),
    };
    Ok(out)
}

// ---------------------------------------------------------------------------
// RICE_1 decompression

fn decompress_rice1(
    data: &[u8],
    n_pixels: usize,
    bytepix: usize,
    block_size: usize,
    zbitpix: i32,
) -> anyhow::Result<Vec<f32>> {
    let int_pixels = rice1_decode(data, n_pixels, bytepix, block_size);
    // Convert raw i32 storage values to f32 using original BITPIX sign/size
    let out: Vec<f32> = int_pixels.iter().map(|&v| match zbitpix {
        8  => (v as u8) as f32,
        16 => (v as i16) as f32,
        32 | -32 => v as f32,
        64 | -64 => v as f32,
        _ => v as f32,
    }).collect();
    Ok(out)
}

/// Decode FITS RICE_1 compressed data into integer pixel values.
/// Reference: Pence et al. 2010 (A&A 524, A42) + cfitsio ricecomp.c
fn rice1_decode(data: &[u8], n_pixels: usize, bytepix: usize, block_size: usize) -> Vec<i32> {
    if n_pixels == 0 {
        return Vec::new();
    }
    let mut reader = BitReader::new(data);
    let mut pixels = Vec::with_capacity(n_pixels);

    let bits = (bytepix * 8) as u8;
    // fsbits: width of the block Rice-parameter selector (5 bits for bytepix=4)
    let fsbits: u8 = match bytepix { 1 => 3, 2 => 4, _ => 5 };
    let fsmax: u32 = (1 << fsbits) - 1; // verbatim-block sentinel

    // First pixel stored verbatim (signed, big-endian bit order)
    let first = reader.read_signed(bits);
    pixels.push(first);
    let mut prev = first;

    while pixels.len() < n_pixels {
        // k selector for this block
        let k = reader.read_bits(fsbits);
        let count = block_size.min(n_pixels - pixels.len());

        if k == fsmax {
            // Verbatim block
            for _ in 0..count {
                let v = reader.read_signed(bits);
                pixels.push(v);
                prev = v;
            }
        } else {
            let k_u8 = k as u8;
            for _ in 0..count {
                // Quotient: count leading 0-bits until first 1-bit
                let mut q = 0u32;
                while !reader.read_bit() {
                    q += 1;
                    if q > 1_000_000 {
                        // Guard against corrupt data
                        pixels.push(prev);
                        break;
                    }
                }
                // Remainder: k bits
                let r = reader.read_bits(k_u8);
                let m = (q << k_u8) | r;
                // Inverse zigzag: even → m/2, odd → -(m+1)/2
                let diff = if m & 1 == 0 { (m >> 1) as i32 } else { -((m >> 1) as i32) - 1 };
                let v = prev.wrapping_add(diff);
                pixels.push(v);
                prev = v;
            }
        }
    }

    pixels
}

// ---------------------------------------------------------------------------
// Bit reader (MSB-first within each byte, matching FITS big-endian bit order)

struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_pos: i8, // 7 = MSB, 0 = LSB
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, byte_pos: 0, bit_pos: 7 }
    }

    fn read_bit(&mut self) -> bool {
        if self.byte_pos >= self.data.len() {
            return false;
        }
        let bit = (self.data[self.byte_pos] >> self.bit_pos) & 1 != 0;
        if self.bit_pos == 0 {
            self.byte_pos += 1;
            self.bit_pos = 7;
        } else {
            self.bit_pos -= 1;
        }
        bit
    }

    fn read_bits(&mut self, n: u8) -> u32 {
        let mut val = 0u32;
        for _ in 0..n {
            val = (val << 1) | self.read_bit() as u32;
        }
        val
    }

    fn read_signed(&mut self, n: u8) -> i32 {
        let raw = self.read_bits(n) as i32;
        // Sign-extend: if MSB of the n-bit value is 1, extend the sign
        if n > 0 && n < 32 && raw & (1 << (n - 1)) != 0 {
            raw - (1 << n)
        } else {
            raw
        }
    }
}

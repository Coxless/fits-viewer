use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use clap::{Parser, Subcommand};
use eframe::egui;
use rayon::prelude::*;

use fitsview_core::{
    arithmetic::{image_arithmetic, ArithOp},
    colormap::{render_to_rgba, Colormap},
    composite::{render_rgb_to_rgba, RgbChannel, RgbCompositeData},
    fits_reader::{list_hdus, load_fits, load_fits_hdu},
    scale::{build_histeq_lut, compute_scale, ScaleMode},
    stats::compute_stats,
    wcs::Wcs,
};

#[derive(Parser)]
#[command(name = "fits-view", version, about = "Fast FITS file viewer")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// FITS file(s) or directory to open (GUI mode).
    #[arg(num_args(0..))]
    paths: Vec<PathBuf>,

    /// Load a saved session file (.fvs) before opening files.
    #[arg(long)]
    session: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Render a FITS image to PNG, JPEG, or PDF
    Render {
        /// Input FITS file (or directory when --output-dir is given)
        input: PathBuf,
        /// Output image file (.png, .jpg, .pdf); omit when using --output-dir
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Output directory for batch rendering (renders all image HDUs)
        #[arg(long)]
        output_dir: Option<PathBuf>,
        /// Output size as WxH (e.g. 256x256)
        #[arg(long, default_value = "256x256")]
        size: String,
        /// Scaling algorithm: zscale|linear|log|sqrt|asinh|minmax|histeq
        #[arg(long, default_value = "zscale")]
        scale: String,
        /// Colormap: gray|viridis|plasma|inferno|hot|rainbow
        #[arg(long, default_value = "gray")]
        colormap: String,
        /// HDU index (0-based)
        #[arg(long)]
        hdu: Option<usize>,
        /// Manual vmin override (skips auto-scaling)
        #[arg(long)]
        vmin: Option<f32>,
        /// Manual vmax override (skips auto-scaling)
        #[arg(long)]
        vmax: Option<f32>,
        /// Number of parallel render jobs for batch mode
        #[arg(long, default_value = "4")]
        jobs: usize,
        /// Output format: png|jpeg|pdf|svg (auto-detected from extension if omitted)
        #[arg(long)]
        format: Option<String>,
        /// DPI for PDF output (default: 150)
        #[arg(long, default_value = "150")]
        dpi: u32,
        /// Add a colorbar to PDF output
        #[arg(long)]
        colorbar: bool,
        /// Title for PDF output
        #[arg(long)]
        title: Option<String>,
        /// RGB composite: provide exactly 3 FITS files (R G B channels)
        #[arg(long, num_args = 3, value_names = ["R_FITS", "G_FITS", "B_FITS"])]
        rgb: Option<Vec<PathBuf>>,
    },
    /// Pixel-wise arithmetic between two FITS images
    Arithmetic {
        /// First input FITS file (A)
        a: PathBuf,
        /// Second input FITS file (B)
        b: PathBuf,
        /// Output FITS-like file (.fits or .png)
        #[arg(short, long)]
        output: PathBuf,
        /// Operation: add|sub|mul|div|reldiff
        #[arg(long, default_value = "sub")]
        op: String,
        /// Scale factor applied to result
        #[arg(long, default_value = "1.0")]
        scale: f32,
    },
    /// Print FITS header information
    Info {
        /// Input FITS file
        input: PathBuf,
        /// Output format: table|json|csv
        #[arg(long, default_value = "table")]
        format: String,
        /// HDU index (0-based); if omitted, lists all HDUs
        #[arg(long)]
        hdu: Option<usize>,
    },
    /// Compute and print image statistics
    Stats {
        /// Input FITS file
        input: PathBuf,
        /// HDU index (0-based)
        #[arg(long)]
        hdu: Option<usize>,
        /// Output format: table|json|csv
        #[arg(long, default_value = "table")]
        format: String,
        /// Compute statistics only within this pixel region: x1:x2,y1:y2 (0-based)
        #[arg(long)]
        region: Option<String>,
    },
    /// Validate FITS file against conditions; exits 0 if all pass, 1 otherwise
    Check {
        /// Input FITS file
        input: PathBuf,
        /// Require image width >= N pixels
        #[arg(long)]
        min_width: Option<usize>,
        /// Require image height >= N pixels
        #[arg(long)]
        min_height: Option<usize>,
        /// Require valid WCS keywords
        #[arg(long)]
        has_wcs: bool,
        /// Require these FITS keywords to be present
        #[arg(long = "has-keyword")]
        has_keyword: Vec<String>,
        /// Assert a header keyword expression, e.g. "NAXIS==2" or "BITPIX==-32"
        #[arg(long = "assert")]
        assert: Vec<String>,
    },
}

fn main() -> anyhow::Result<()> {
    // fitsrs logs ERROR when it hits EOF while scanning for the next HDU header,
    // which is normal behaviour for any valid single-HDU file. Silence that module
    // so users don't see spurious red errors; the RUST_LOG env var can override.
    env_logger::Builder::from_default_env()
        .filter_module("fitsrs::hdu", log::LevelFilter::Off)
        .init();
    let cli = Cli::parse();

    match cli.command {
        Some(cmd) => run_headless(cmd),
        None => run_gui(cli.paths, cli.session),
    }
}

fn run_headless(cmd: Commands) -> anyhow::Result<()> {
    match cmd {
        Commands::Render { input, output, output_dir, size, scale, colormap, hdu, vmin, vmax, jobs, format, dpi, colorbar, title, rgb } => {
            cmd_render(&input, output.as_deref(), output_dir.as_deref(), &size, &scale, &colormap, hdu, vmin, vmax, jobs, format.as_deref(), dpi, colorbar, title.as_deref(), rgb.as_deref())
        }
        Commands::Arithmetic { a, b, output, op, scale } => cmd_arithmetic(&a, &b, &output, &op, scale),
        Commands::Info { input, format, hdu } => cmd_info(&input, &format, hdu),
        Commands::Stats { input, hdu, format, region } => cmd_stats(&input, hdu, &format, region.as_deref()),
        Commands::Check { input, min_width, min_height, has_wcs, has_keyword, assert } => {
            cmd_check(&input, min_width, min_height, has_wcs, &has_keyword, &assert)
        }
    }
}

/// Write raw RGBA pixel data as a PNG file using the `image` crate's save API.
fn write_rgba_png(rgba: &[u8], width: usize, height: usize, path: &Path) -> anyhow::Result<()> {
    // Encode PNG manually via the `png` encoder embedded in the image crate
    let file = std::fs::File::create(path)?;
    let w = &mut std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(rgba)?;
    println!("Saved {}×{} → {}", width, height, path.display());
    Ok(())
}

fn parse_scale(s: &str) -> anyhow::Result<ScaleMode> {
    match s.to_ascii_lowercase().as_str() {
        "zscale" => Ok(ScaleMode::ZScale),
        "linear" => Ok(ScaleMode::Linear),
        "log" => Ok(ScaleMode::Log),
        "sqrt" => Ok(ScaleMode::Sqrt),
        "asinh" => Ok(ScaleMode::Asinh),
        "minmax" => Ok(ScaleMode::MinMax),
        "histeq" => Ok(ScaleMode::HistEq),
        other => anyhow::bail!("Unknown scale mode: {other}"),
    }
}

fn parse_colormap(s: &str) -> anyhow::Result<Colormap> {
    match s.to_ascii_lowercase().as_str() {
        "gray" | "grey" => Ok(Colormap::Gray),
        "viridis" => Ok(Colormap::Viridis),
        "plasma" => Ok(Colormap::Plasma),
        "inferno" => Ok(Colormap::Inferno),
        "hot" => Ok(Colormap::Hot),
        "rainbow" => Ok(Colormap::Rainbow),
        other => anyhow::bail!("Unknown colormap: {other}"),
    }
}

fn parse_size(s: &str) -> anyhow::Result<(u32, u32)> {
    let parts: Vec<&str> = s.split('x').collect();
    anyhow::ensure!(parts.len() == 2, "Size must be WxH (e.g. 256x256), got: {s}");
    let w = u32::from_str(parts[0]).map_err(|_| anyhow::anyhow!("Invalid width: {}", parts[0]))?;
    let h = u32::from_str(parts[1]).map_err(|_| anyhow::anyhow!("Invalid height: {}", parts[1]))?;
    Ok((w, h))
}

#[allow(clippy::too_many_arguments)]
fn render_one(
    input: &Path,
    output: &Path,
    out_w: u32,
    out_h: u32,
    scale_mode: ScaleMode,
    cmap: Colormap,
    hdu: Option<usize>,
    vmin: Option<f32>,
    vmax: Option<f32>,
) -> anyhow::Result<()> {
    let img = if let Some(idx) = hdu { load_fits_hdu(input, idx)? } else { load_fits(input)? };

    let (use_vmin, use_vmax) = if let (Some(mn), Some(mx)) = (vmin, vmax) {
        (mn, mx)
    } else {
        let sr = compute_scale(&img.data, scale_mode);
        (sr.vmin, sr.vmax)
    };

    let histeq_lut = if scale_mode == ScaleMode::HistEq {
        Some(build_histeq_lut(&img.data, use_vmin, use_vmax))
    } else {
        None
    };

    let rgba = render_to_rgba(&img.data, use_vmin, use_vmax, cmap, scale_mode, 1.0, 0.5, histeq_lut.as_deref());
    let src = image::RgbaImage::from_raw(img.width as u32, img.height as u32, rgba)
        .ok_or_else(|| anyhow::anyhow!("Failed to create image buffer"))?;
    let resized = image::imageops::resize(&src, out_w, out_h, image::imageops::FilterType::Lanczos3);
    resized.save(output)?;
    println!("Saved {}x{} → {}", out_w, out_h, output.display());
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_render(
    input: &Path,
    output: Option<&Path>,
    output_dir: Option<&Path>,
    size: &str,
    scale: &str,
    colormap: &str,
    hdu: Option<usize>,
    vmin: Option<f32>,
    vmax: Option<f32>,
    jobs: usize,
    format: Option<&str>,
    dpi: u32,
    colorbar: bool,
    title: Option<&str>,
    rgb: Option<&[PathBuf]>,
) -> anyhow::Result<()> {
    // RGB composite mode
    if let Some(channels) = rgb {
        anyhow::ensure!(channels.len() == 3, "RGB requires exactly 3 input files");
        let out = output.ok_or_else(|| anyhow::anyhow!("--output is required for RGB mode"))?;
        return cmd_render_rgb(&channels[0], &channels[1], &channels[2], out, parse_scale(scale)?);
    }

    let scale_mode = parse_scale(scale)?;
    let cmap = parse_colormap(colormap)?;
    let (out_w, out_h) = parse_size(size)?;

    // Determine output format from argument or extension
    let fmt = format.map(str::to_ascii_lowercase);
    let output_format = fmt.as_deref();

    if let Some(out_dir) = output_dir {
        // Batch mode
        std::fs::create_dir_all(out_dir)?;
        let files: Vec<PathBuf> = if input.is_dir() {
            let mut v = Vec::new();
            for entry in std::fs::read_dir(input)? {
                let entry = entry?;
                let p = entry.path();
                if let Some(ext) = p.extension() {
                    if matches!(ext.to_str().unwrap_or(""), "fits" | "fit" | "fts") {
                        v.push(p);
                    }
                }
            }
            v
        } else {
            vec![input.to_path_buf()]
        };

        let pool = rayon::ThreadPoolBuilder::new().num_threads(jobs).build()?;
        let errors: Vec<String> = pool.install(|| {
            files.par_iter().filter_map(|f| {
                let stem = f.file_stem().unwrap_or_default().to_string_lossy();
                let out = out_dir.join(format!("{stem}.png"));
                render_one(f, &out, out_w, out_h, scale_mode, cmap, None, vmin, vmax).err()
                    .map(|e| format!("{}: {e}", f.display()))
            }).collect()
        });
        for e in &errors {
            eprintln!("ERROR: {e}");
        }
        if !errors.is_empty() {
            anyhow::bail!("{} file(s) failed to render", errors.len());
        }
        Ok(())
    } else {
        let out = output.ok_or_else(|| anyhow::anyhow!("--output is required when not using --output-dir"))?;

        let ext = out.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        let use_pdf = output_format == Some("pdf") || ext == "pdf";

        if use_pdf {
            render_pdf(input, out, out_w, out_h, scale_mode, cmap, hdu, vmin, vmax, dpi, colorbar, title)
        } else {
            render_one(input, out, out_w, out_h, scale_mode, cmap, hdu, vmin, vmax)
        }
    }
}

fn cmd_render_rgb(r_path: &Path, g_path: &Path, b_path: &Path, output: &Path, scale: ScaleMode) -> anyhow::Result<()> {
    let r = load_fits(r_path)?;
    let g = load_fits(g_path)?;
    let b = load_fits(b_path)?;

    let w = r.width.min(g.width).min(b.width);
    let h = r.height.min(g.height).min(b.height);

    let make_channel = |img: &fitsview_core::fits_reader::FitsImage| -> RgbChannel {
        let sr = compute_scale(&img.data, scale);
        RgbChannel { data: img.data.clone(), vmin: sr.vmin, vmax: sr.vmax, scale, contrast: 1.0, bias: 0.5 }
    };

    let composite = RgbCompositeData { r: make_channel(&r), g: make_channel(&g), b: make_channel(&b), width: w, height: h };
    let rgba = render_rgb_to_rgba(&composite);

    // Write as PNG directly without using image crate's higher-level types
    write_rgba_png(&rgba, w, h, output)?;
    println!("RGB composite saved → {}", output.display());
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn render_pdf(
    input: &Path,
    output: &Path,
    out_w: u32,
    out_h: u32,
    scale_mode: ScaleMode,
    cmap: Colormap,
    hdu: Option<usize>,
    vmin: Option<f32>,
    vmax: Option<f32>,
    dpi: u32,
    colorbar: bool,
    title: Option<&str>,
) -> anyhow::Result<()> {
    use printpdf::*;

    let img_data = if let Some(idx) = hdu { load_fits_hdu(input, idx)? } else { load_fits(input)? };

    let (use_vmin, use_vmax) = if let (Some(mn), Some(mx)) = (vmin, vmax) {
        (mn, mx)
    } else {
        let sr = compute_scale(&img_data.data, scale_mode);
        (sr.vmin, sr.vmax)
    };

    let histeq_lut = if scale_mode == ScaleMode::HistEq {
        Some(build_histeq_lut(&img_data.data, use_vmin, use_vmax))
    } else {
        None
    };

    let rgba = render_to_rgba(&img_data.data, use_vmin, use_vmax, cmap, scale_mode, 1.0, 0.5, histeq_lut.as_deref());

    // Convert RGBA → RGB by dropping alpha channel; use original image dimensions
    let px_w = img_data.width as u32;
    let px_h = img_data.height as u32;
    let rgb_img: Vec<u8> = rgba.chunks(4).flat_map(|px| [px[0], px[1], px[2]]).collect();

    // Create PDF
    let dpi_f = dpi as f32;
    let page_w_mm = (out_w as f32 / dpi_f) * 25.4;
    let page_h_mm = (out_h as f32 / dpi_f) * 25.4 + if title.is_some() { 12.0 } else { 0.0 } + if colorbar { 12.0 } else { 0.0 };

    let (doc, page1, layer1) = PdfDocument::new(
        title.unwrap_or("fits-view"),
        Mm(page_w_mm),
        Mm(page_h_mm),
        "Layer 1",
    );

    let current_layer = doc.get_page(page1).get_layer(layer1);

    // Image y offset if title present
    let img_y_offset = if colorbar { 12.0f32 } else { 0.0 };

    // Embed the rasterized image
    let pdf_img = Image::from(ImageXObject {
        width: Px(px_w as usize),
        height: Px(px_h as usize),
        color_space: ColorSpace::Rgb,
        bits_per_component: ColorBits::Bit8,
        interpolate: true,
        image_data: rgb_img,
        image_filter: None,
        smask: None,
        clipping_bbox: None,
    });

    pdf_img.add_to_layer(
        current_layer.clone(),
        ImageTransform {
            translate_x: Some(Mm(0.0)),
            translate_y: Some(Mm(img_y_offset)),
            scale_x: Some(page_w_mm / (px_w as f32 / dpi_f * 25.4)),
            scale_y: Some((page_h_mm - img_y_offset - if title.is_some() { 12.0 } else { 0.0 }) / (px_h as f32 / dpi_f * 25.4)),
            ..Default::default()
        },
    );

    // Title text
    if let Some(t) = title {
        let font = doc.add_builtin_font(BuiltinFont::Helvetica)?;
        current_layer.use_text(t, 12.0, Mm(2.0), Mm(page_h_mm - 10.0), &font);
    }

    // Simple colorbar: gradient text labels at bottom
    if colorbar {
        let font = doc.add_builtin_font(BuiltinFont::Helvetica)?;
        current_layer.use_text(format!("{use_vmin:.3}"), 7.0, Mm(1.0), Mm(2.0), &font);
        current_layer.use_text(format!("{use_vmax:.3}"), 7.0, Mm(page_w_mm - 18.0), Mm(2.0), &font);
        current_layer.use_text("Color range", 7.0, Mm(page_w_mm / 2.0 - 8.0), Mm(2.0), &font);
    }

    doc.save(&mut std::io::BufWriter::new(std::fs::File::create(output)?))?;
    println!("PDF saved ({dpi} DPI) → {}", output.display());
    Ok(())
}

fn cmd_arithmetic(a: &Path, b: &Path, output: &Path, op_str: &str, scale: f32) -> anyhow::Result<()> {
    let img_a = load_fits(a)?;
    let img_b = load_fits(b)?;

    anyhow::ensure!(
        img_a.width == img_b.width && img_a.height == img_b.height,
        "Image size mismatch: {:?}={}×{} vs {:?}={}×{}",
        a, img_a.width, img_a.height, b, img_b.width, img_b.height
    );

    let op = match op_str.to_ascii_lowercase().as_str() {
        "add" | "+" => ArithOp::Add,
        "sub" | "-" => ArithOp::Sub,
        "mul" | "*" => ArithOp::Mul,
        "div" | "/" => ArithOp::Div,
        "reldiff" => ArithOp::RelDiff,
        other => anyhow::bail!("Unknown operation: {other}. Use add|sub|mul|div|reldiff"),
    };

    let result = image_arithmetic(&img_a.data, &img_b.data, img_a.width, img_a.height, op, scale)?;

    // Save as PNG for now (could add FITS write support later)
    let sr = compute_scale(&result, ScaleMode::ZScale);
    let rgba = render_to_rgba(&result, sr.vmin, sr.vmax, Colormap::Gray, ScaleMode::ZScale, 1.0, 0.5, None);
    write_rgba_png(&rgba, img_a.width, img_a.height, output)?;
    println!("Arithmetic result ({op_str}) saved → {}", output.display());
    Ok(())
}

fn cmd_info(input: &Path, format: &str, hdu: Option<usize>) -> anyhow::Result<()> {
    if let Some(idx) = hdu {
        // Print header for specific HDU
        let img = load_fits_hdu(input, idx)?;
        let mut keys: Vec<(&String, &String)> = img.header.iter().collect();
        keys.sort_by_key(|(k, _)| k.as_str());

        match format.to_ascii_lowercase().as_str() {
            "json" => {
                let map: serde_json::Map<String, serde_json::Value> = keys
                    .into_iter()
                    .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&serde_json::Value::Object(map))?);
            }
            "csv" => {
                println!("keyword,value");
                for (k, v) in &keys {
                    println!("{},{}", k, v.replace(',', "\\,"));
                }
            }
            _ => {
                println!("{:<10}  VALUE", "KEYWORD");
                println!("{}", "-".repeat(60));
                for (k, v) in &keys {
                    println!("{:<10}  {}", k, v);
                }
            }
        }
    } else {
        // List all HDUs
        let hdus = list_hdus(input)?;

        match format.to_ascii_lowercase().as_str() {
            "json" => {
                let arr: Vec<serde_json::Value> = hdus
                    .iter()
                    .enumerate()
                    .map(|(i, h)| {
                        serde_json::json!({
                            "index": i,
                            "name": h.name,
                            "type": format!("{:?}", h.hdu_type),
                            "naxis": h.naxis,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&arr)?);
            }
            "csv" => {
                println!("index,name,type,naxis");
                for (i, h) in hdus.iter().enumerate() {
                    println!("{},{},{:?},{:?}", i, h.name, h.hdu_type, h.naxis);
                }
            }
            _ => {
                println!("{:<6}  {:<20}  {:<10}  NAXIS", "INDEX", "NAME", "TYPE");
                println!("{}", "-".repeat(60));
                for (i, h) in hdus.iter().enumerate() {
                    println!(
                        "{:<6}  {:<20}  {:<10}  {:?}",
                        i,
                        if h.name.is_empty() { "-" } else { &h.name },
                        format!("{:?}", h.hdu_type),
                        h.naxis,
                    );
                }
            }
        }
    }
    Ok(())
}

fn parse_region(s: &str) -> anyhow::Result<(usize, usize, usize, usize)> {
    // Format: "x1:x2,y1:y2"
    let parts: Vec<&str> = s.split(',').collect();
    anyhow::ensure!(parts.len() == 2, "Region must be x1:x2,y1:y2, got: {s}");
    let xp: Vec<&str> = parts[0].split(':').collect();
    let yp: Vec<&str> = parts[1].split(':').collect();
    anyhow::ensure!(xp.len() == 2 && yp.len() == 2, "Region must be x1:x2,y1:y2");
    let x1: usize = xp[0].trim().parse()?;
    let x2: usize = xp[1].trim().parse()?;
    let y1: usize = yp[0].trim().parse()?;
    let y2: usize = yp[1].trim().parse()?;
    Ok((x1, x2, y1, y2))
}

type CmpFn = fn(f64, f64) -> bool;

fn evaluate_assert(expr: &str, header: &HashMap<String, String>) -> anyhow::Result<bool> {
    // Try two-char operators first, then single-char
    let ops: &[(&str, CmpFn)] = &[
        ("==", |a, b| (a - b).abs() < 1e-9),
        ("!=", |a, b| (a - b).abs() >= 1e-9),
        ("<=", |a, b| a <= b),
        (">=", |a, b| a >= b),
        ("<",  |a, b| a < b),
        (">",  |a, b| a > b),
    ];
    for &(op, cmp) in ops {
        if let Some(pos) = expr.find(op) {
            let lhs = expr[..pos].trim().to_ascii_uppercase();
            let rhs = expr[pos + op.len()..].trim();
            let val_str = header.get(&lhs).ok_or_else(|| anyhow::anyhow!("keyword '{lhs}' not in header"))?;
            // Try numeric comparison
            if let (Ok(a), Ok(b)) = (val_str.trim().parse::<f64>(), rhs.parse::<f64>()) {
                return Ok(cmp(a, b));
            }
            // String comparison (only == and !=)
            let rhs_clean = rhs.trim_matches('\'');
            return Ok(match op {
                "==" => val_str.trim() == rhs_clean,
                "!=" => val_str.trim() != rhs_clean,
                _ => anyhow::bail!("Cannot compare non-numeric values with '{op}'"),
            });
        }
    }
    anyhow::bail!("Could not parse assertion: {expr}")
}

fn cmd_stats(input: &Path, hdu: Option<usize>, format: &str, region: Option<&str>) -> anyhow::Result<()> {
    let img = if let Some(idx) = hdu {
        load_fits_hdu(input, idx)?
    } else {
        load_fits(input)?
    };

    let data: &[f32] = if let Some(r) = region {
        let (x1, x2, y1, y2) = parse_region(r)?;
        let x2 = x2.min(img.width);
        let y2 = y2.min(img.height);
        // Collect region pixels (we'll use a temporary allocation)
        let mut region_data = Vec::with_capacity((x2 - x1) * (y2 - y1));
        for row in y1..y2 {
            for col in x1..x2 {
                if row < img.height && col < img.width {
                    region_data.push(img.data[row * img.width + col]);
                }
            }
        }
        // stats borrows data, so we handle this inline
        let stats = compute_stats(&region_data);
        return print_stats(&stats, format);
    } else {
        &img.data
    };

    let stats = compute_stats(data);
    print_stats(&stats, format)
}

fn print_stats(stats: &fitsview_core::stats::ImageStats, format: &str) -> anyhow::Result<()> {

    match format.to_ascii_lowercase().as_str() {
        "json" => {
            let v = serde_json::json!({
                "npix": stats.npix,
                "n_finite": stats.n_finite,
                "min": stats.min,
                "max": stats.max,
                "mean": stats.mean,
                "median": stats.median,
                "std_dev": stats.std_dev,
                "sum": stats.sum,
            });
            println!("{}", serde_json::to_string_pretty(&v)?);
        }
        "csv" => {
            println!("npix,n_finite,min,max,mean,median,std_dev,sum");
            println!(
                "{},{},{},{},{},{},{},{}",
                stats.npix,
                stats.n_finite,
                stats.min,
                stats.max,
                stats.mean,
                stats.median,
                stats.std_dev,
                stats.sum,
            );
        }
        _ => {
            println!("{:<12}  VALUE", "STATISTIC");
            println!("{}", "-".repeat(40));
            println!("{:<12}  {}", "npix", stats.npix);
            println!("{:<12}  {}", "n_finite", stats.n_finite);
            println!("{:<12}  {:.6}", "min", stats.min);
            println!("{:<12}  {:.6}", "max", stats.max);
            println!("{:<12}  {:.6}", "mean", stats.mean);
            println!("{:<12}  {:.6}", "median", stats.median);
            println!("{:<12}  {:.6}", "std_dev", stats.std_dev);
            println!("{:<12}  {:.6}", "sum", stats.sum);
        }
    }
    Ok(())
}

fn cmd_check(
    input: &Path,
    min_width: Option<usize>,
    min_height: Option<usize>,
    has_wcs: bool,
    has_keyword: &[String],
    assert: &[String],
) -> anyhow::Result<()> {
    let img = load_fits(input)?;
    let mut failures: Vec<String> = Vec::new();

    if let Some(mw) = min_width {
        if img.width < mw {
            failures.push(format!("width {} < min_width {}", img.width, mw));
        }
    }
    if let Some(mh) = min_height {
        if img.height < mh {
            failures.push(format!("height {} < min_height {}", img.height, mh));
        }
    }
    if has_wcs && Wcs::from_header(&img.header).is_none() {
        failures.push("no valid WCS keywords found".to_string());
    }
    for kw in has_keyword {
        let kw_upper = kw.to_ascii_uppercase();
        if !img.header.contains_key(&kw_upper) {
            failures.push(format!("keyword {kw_upper} not found"));
        }
    }
    for expr in assert {
        match evaluate_assert(expr, &img.header) {
            Ok(true) => {}
            Ok(false) => failures.push(format!("assertion failed: {expr}")),
            Err(e) => failures.push(format!("assertion error ({expr}): {e}")),
        }
    }

    if failures.is_empty() {
        println!("OK: {}", input.display());
        Ok(())
    } else {
        for f in &failures {
            eprintln!("FAIL: {f}");
        }
        std::process::exit(1);
    }
}

fn run_gui(mut paths: Vec<PathBuf>, session: Option<PathBuf>) -> anyhow::Result<()> {
    // Load session file and prepend its paths if provided
    if let Some(ref session_path) = session {
        use fitsview_core::session::Session;
        match Session::load(session_path) {
            Ok(s) => {
                let mut session_paths: Vec<PathBuf> = s.files.iter()
                    .map(|f| PathBuf::from(&f.path))
                    .filter(|p| p.exists())
                    .collect();
                session_paths.extend(paths);
                paths = session_paths;
            }
            Err(e) => log::warn!("Could not load session {}: {e}", session_path.display()),
        }
    }

    let mut files: Vec<PathBuf> = Vec::new();
    let mut dir: Option<PathBuf> = None;

    for p in paths {
        if p.is_dir() {
            dir = Some(p);
        } else if p == std::path::Path::new(".") {
            dir = Some(std::env::current_dir()?);
        } else {
            files.push(p);
        }
    }

    if dir.is_none() && !files.is_empty() {
        if let Some(parent) = files[0].parent() {
            if parent != std::path::Path::new("") {
                dir = Some(parent.to_path_buf());
            }
        }
    }

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("fits-view")
            .with_inner_size([1200.0, 800.0]),
        #[cfg(feature = "gpu")]
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "fits-view",
        native_options,
        Box::new(move |cc| {
            Ok(Box::new(fitsview_gui::app::FitsViewApp::new(cc, files.clone(), dir.clone())))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    Ok(())
}

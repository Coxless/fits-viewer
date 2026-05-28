use std::path::{Path, PathBuf};
use std::str::FromStr;

use clap::{Parser, Subcommand};
use eframe::egui;

use fitsview_core::{
    colormap::{render_to_rgba, Colormap},
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
}

#[derive(Subcommand)]
enum Commands {
    /// Render a FITS image to PNG or JPEG
    Render {
        /// Input FITS file
        input: PathBuf,
        /// Output image file (.png or .jpg)
        #[arg(short, long)]
        output: PathBuf,
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
    },
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    match cli.command {
        Some(cmd) => run_headless(cmd),
        None => run_gui(cli.paths),
    }
}

fn run_headless(cmd: Commands) -> anyhow::Result<()> {
    match cmd {
        Commands::Render { input, output, size, scale, colormap, hdu } => {
            cmd_render(&input, &output, &size, &scale, &colormap, hdu)
        }
        Commands::Info { input, format, hdu } => cmd_info(&input, &format, hdu),
        Commands::Stats { input, hdu, format } => cmd_stats(&input, hdu, &format),
        Commands::Check { input, min_width, min_height, has_wcs, has_keyword } => {
            cmd_check(&input, min_width, min_height, has_wcs, &has_keyword)
        }
    }
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

fn cmd_render(
    input: &Path,
    output: &Path,
    size: &str,
    scale: &str,
    colormap: &str,
    hdu: Option<usize>,
) -> anyhow::Result<()> {
    let scale_mode = parse_scale(scale)?;
    let cmap = parse_colormap(colormap)?;
    let (out_w, out_h) = parse_size(size)?;

    let img = if let Some(idx) = hdu {
        load_fits_hdu(input, idx)?
    } else {
        load_fits(input)?
    };

    let scale_result = compute_scale(&img.data, scale_mode);
    let histeq_lut = if scale_mode == ScaleMode::HistEq {
        Some(build_histeq_lut(&img.data, scale_result.vmin, scale_result.vmax))
    } else {
        None
    };

    let rgba = render_to_rgba(
        &img.data,
        scale_result.vmin,
        scale_result.vmax,
        cmap,
        scale_mode,
        1.0,
        0.5,
        histeq_lut.as_deref(),
    );

    // Build image buffer at native resolution, then resize
    let src = image::RgbaImage::from_raw(img.width as u32, img.height as u32, rgba)
        .ok_or_else(|| anyhow::anyhow!("Failed to create image buffer"))?;

    let resized = image::imageops::resize(
        &src,
        out_w,
        out_h,
        image::imageops::FilterType::Lanczos3,
    );

    resized.save(output)?;
    println!("Saved {}x{} → {}", out_w, out_h, output.display());
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

fn cmd_stats(input: &Path, hdu: Option<usize>, format: &str) -> anyhow::Result<()> {
    let img = if let Some(idx) = hdu {
        load_fits_hdu(input, idx)?
    } else {
        load_fits(input)?
    };

    let stats = compute_stats(&img.data);

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

fn run_gui(paths: Vec<PathBuf>) -> anyhow::Result<()> {
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

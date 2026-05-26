use std::path::PathBuf;

use clap::Parser;
use eframe::egui;

#[derive(Parser)]
#[command(name = "fits-view", version, about = "Fast FITS file viewer")]
struct Cli {
    /// FITS file(s) or directory to open.
    /// Pass a single directory (or `.`) to open the file explorer.
    /// Pass one or more FITS files to open them as tabs.
    #[arg(num_args(0..))]
    paths: Vec<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    // Separate directories from files
    let mut files: Vec<PathBuf> = Vec::new();
    let mut dir: Option<PathBuf> = None;

    for p in cli.paths {
        if p.is_dir() {
            dir = Some(p);
        } else if p == std::path::Path::new(".") {
            dir = Some(std::env::current_dir()?);
        } else {
            files.push(p);
        }
    }

    // If no dir but we have files, use the parent of the first file as the explorer root
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
        ..Default::default()
    };

    eframe::run_native(
        "fits-view",
        native_options,
        Box::new(move |cc| {
            Ok(Box::new(fitsview_gui::app::FitsViewApp::new(
                cc,
                files.clone(),
                dir.clone(),
            )))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    Ok(())
}

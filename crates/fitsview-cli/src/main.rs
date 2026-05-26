use clap::Parser;
use eframe::egui;

#[derive(Parser)]
#[command(name = "fits-view", version, about = "Fast FITS file viewer")]
struct Cli {
    /// FITS file or directory to open
    path: Option<std::path::PathBuf>,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("fits-view")
            .with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };

    eframe::run_native(
        "fits-view",
        native_options,
        Box::new(move |cc| Ok(Box::new(fitsview_gui::app::FitsViewApp::new(cc, cli.path.clone())))),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    Ok(())
}

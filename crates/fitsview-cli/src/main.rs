use clap::Parser;

#[derive(Parser)]
#[command(name = "fits-view", version, about = "Fast FITS file viewer")]
struct Cli {
    /// FITS file or directory to open
    path: Option<std::path::PathBuf>,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let _cli = Cli::parse();
    println!("fits-view: Step 0 scaffold — GUI not yet implemented");
    Ok(())
}

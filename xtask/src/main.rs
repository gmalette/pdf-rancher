use anyhow::Result;
use clap::{Parser, Subcommand};

mod build_libheif;
mod release;
mod update_pdfium;

#[derive(Parser)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Release {
        #[arg(long)]
        allow_dirty: bool,
    },
    UpdatePdfium {
        #[arg(long)]
        version: String,
    },
    BuildLibheif {
        /// Optional libheif tag to build (e.g., v1.17.6). If not provided, a sensible default is used.
        #[arg(long)]
        version: Option<String>,
        /// Build for both supported targets (cross-compile when possible)
        #[arg(long, default_value_t = false)]
        all_targets: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Release { allow_dirty } => release::run(allow_dirty)?,
        Commands::UpdatePdfium { version } => update_pdfium::run(&version)?,
        Commands::BuildLibheif {
            version,
            all_targets,
        } => build_libheif::run(version, all_targets)?,
    }
    Ok(())
}

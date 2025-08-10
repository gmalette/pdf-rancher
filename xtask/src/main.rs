use clap::{Parser, Subcommand};
use anyhow::Result;

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
    /// Create a release: check git, bump version, build, and create GitHub draft release
    Release {
        /// Allow uncommitted changes in the working directory
        #[arg(long)]
        allow_dirty: bool,
    },
    /// Update pdfium binaries
    UpdatePdfium,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Release { allow_dirty } => release::run(allow_dirty)?,
        Commands::UpdatePdfium => update_pdfium::run()?,
    }
    Ok(())
}

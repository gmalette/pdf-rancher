use anyhow::Result;
use clap::{Parser, Subcommand};

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
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Release { allow_dirty } => release::run(allow_dirty)?,
        Commands::UpdatePdfium { version } => update_pdfium::run(&version)?,
    }
    Ok(())
}

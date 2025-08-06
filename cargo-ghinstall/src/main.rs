mod cli;
mod config;
mod github;
mod installer;
mod error;
mod utils;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::cli::Args;
use crate::installer::Installer;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // Parse command line arguments
    let args = Args::parse();

    if args.verbose {
        tracing::info!("Running cargo-ghinstall with verbose output");
    }

    // Create installer and run
    let installer = Installer::new(args)?;
    installer.run().await?;

    Ok(())
}

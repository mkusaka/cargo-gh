mod cli;
mod config;
mod error;
mod github;
mod installer;
mod utils;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::cli::{Args, CargoCli};
use crate::installer::Installer;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // Parse command line arguments - handle both cargo subcommand and direct invocation
    let args = match CargoCli::try_parse() {
        Ok(CargoCli::Ghinstall(args)) => args,
        Err(_) => {
            // Fall back to parsing as direct invocation (for cargo-ghinstall binary)
            Args::parse()
        }
    };

    if args.verbose {
        tracing::info!("Running cargo-ghinstall with verbose output");
    }

    // Create installer and run
    let installer = Installer::new(args)?;
    installer.run().await?;

    Ok(())
}

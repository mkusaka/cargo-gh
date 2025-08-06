mod cli;
mod config;
mod builder;
mod github;
mod packager;
mod error;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::cli::Args;
use crate::builder::DistBuilder;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // Parse command line arguments
    let args = Args::parse();

    if args.verbose {
        tracing::info!("Running cargo-ghdist with verbose output");
    }

    // Create builder and run
    let builder = DistBuilder::new(args)?;
    builder.run().await?;

    Ok(())
}

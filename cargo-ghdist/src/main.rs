mod builder;
mod cli;
mod config;
mod error;
mod github;
mod init;
mod packager;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::builder::DistBuilder;
use crate::cli::{CargoCli, Command};
use crate::init::Initializer;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // Parse command line arguments - handle both cargo subcommand and direct invocation
    let cli = match CargoCli::try_parse() {
        Ok(CargoCli::Ghdist(cli)) => cli,
        Err(_) => {
            // Fall back to parsing as direct invocation (for cargo-ghdist binary)
            // In this case, parse GhdistCli directly
            use crate::cli::GhdistCli;
            GhdistCli::parse()
        }
    };

    if cli.verbose {
        tracing::info!("Running cargo-ghdist with verbose output");
    }

    // Handle subcommands
    match cli.command {
        Some(Command::Init { yes, ci, skip_ci }) => {
            // Run init command
            let initializer = Initializer::new(yes, ci, skip_ci);
            initializer.run().await?;
        }
        None => {
            // Default behavior: build and distribute
            let args = cli.into();
            let builder = DistBuilder::new(args)?;
            builder.run().await?;
        }
    }

    Ok(())
}

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[clap(
    name = "cargo-ghdist",
    version,
    about = "Build and distribute binaries to GitHub Releases",
    long_about = None,
    bin_name = "cargo"
)]
pub enum CargoCli {
    #[clap(name = "ghdist")]
    Ghdist(GhdistCli),
}

#[derive(Parser, Debug, Clone)]
#[clap(version, about, long_about = None)]
pub struct GhdistCli {
    #[clap(subcommand)]
    pub command: Option<Command>,

    /// Release tag (e.g., v1.2.3, abcdef0, main, or any git ref)
    /// If not specified, uses the tag on HEAD
    #[clap(short, long, global = true)]
    pub tag: Option<String>,

    /// Build targets (comma-separated Rust triple format)
    /// Example: x86_64-unknown-linux-gnu,aarch64-unknown-linux-gnu
    #[clap(short = 'T', long, value_delimiter = ',', global = true)]
    pub targets: Option<Vec<String>>,

    /// Archive format (tgz or zip)
    #[clap(short, long, default_value = "tgz", global = true)]
    pub format: ArchiveFormat,

    /// Create as draft release
    #[clap(long, global = true)]
    pub draft: bool,

    /// Skip cargo publish step
    #[clap(long, default_value_t = true, global = true)]
    pub skip_publish: bool,

    /// Don't generate checksum files (SHA256SUMS)
    #[clap(long, global = true)]
    pub no_checksum: bool,

    /// Configuration file path
    #[clap(long, default_value = ".config/ghdist.toml", global = true)]
    pub config: PathBuf,

    /// Enable verbose output
    #[clap(long, global = true)]
    pub verbose: bool,

    /// GitHub repository (owner/repo)
    /// If not specified, uses repository from Cargo.toml
    #[clap(long, global = true)]
    pub repository: Option<String>,

    /// GitHub token (can also be set via GITHUB_TOKEN env var)
    #[clap(long, env = "GITHUB_TOKEN", global = true)]
    pub github_token: Option<String>,

    /// Binary names to include (if not specified, includes all)
    #[clap(long, value_delimiter = ',', global = true)]
    pub bins: Option<Vec<String>>,

    /// Cargo build profile (release, debug, etc.)
    #[clap(long, default_value = "release", global = true)]
    pub profile: String,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    /// Initialize cargo-ghdist configuration for the project
    Init {
        /// Skip interactive prompts and use defaults
        #[clap(short = 'y', long)]
        yes: bool,

        /// CI provider (github, gitlab, etc.)
        #[clap(long, default_value = "github")]
        ci: String,

        /// Skip generating CI workflow
        #[clap(long)]
        skip_ci: bool,
    },
}

// For backward compatibility, create Args from GhdistCli
#[derive(Debug, Clone)]
pub struct Args {
    pub tag: Option<String>,
    pub targets: Option<Vec<String>>,
    pub format: ArchiveFormat,
    pub draft: bool,
    pub skip_publish: bool,
    pub no_checksum: bool,
    pub config: Option<PathBuf>,
    pub verbose: bool,
    pub repository: Option<String>,
    pub github_token: Option<String>,
    pub bins: Option<Vec<String>>,
    pub profile: String,
}

impl From<GhdistCli> for Args {
    fn from(cli: GhdistCli) -> Self {
        Args {
            tag: cli.tag,
            targets: cli.targets,
            format: cli.format,
            draft: cli.draft,
            skip_publish: cli.skip_publish,
            no_checksum: cli.no_checksum,
            config: Some(cli.config),
            verbose: cli.verbose,
            repository: cli.repository,
            github_token: cli.github_token,
            bins: cli.bins,
            profile: cli.profile,
        }
    }
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ArchiveFormat {
    Tgz,
    Zip,
}

impl std::fmt::Display for ArchiveFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArchiveFormat::Tgz => write!(f, "tgz"),
            ArchiveFormat::Zip => write!(f, "zip"),
        }
    }
}

impl Args {
    /// Get the list of targets, using defaults if not specified
    pub fn targets(&self) -> Vec<String> {
        self.targets.clone().unwrap_or_else(|| {
            vec![
                "x86_64-unknown-linux-gnu".to_string(),
                "aarch64-unknown-linux-gnu".to_string(),
            ]
        })
    }

    /// Parse repository from argument or Cargo.toml
    pub fn parse_repository(&self) -> anyhow::Result<(String, String)> {
        if let Some(repo) = &self.repository {
            let parts: Vec<&str> = repo.split('/').collect();
            if parts.len() != 2 {
                anyhow::bail!("Invalid repository format. Expected: owner/repo");
            }
            Ok((parts[0].to_string(), parts[1].to_string()))
        } else {
            // Try to read from Cargo.toml
            let cargo_toml = std::fs::read_to_string("Cargo.toml")?;
            let manifest: toml::Value = toml::from_str(&cargo_toml)?;

            let repo_url = manifest
                .get("package")
                .and_then(|p| p.get("repository"))
                .and_then(|r| r.as_str())
                .ok_or_else(|| anyhow::anyhow!("No repository field in Cargo.toml"))?;

            // Parse GitHub URL
            let repo_url = repo_url.trim_end_matches(".git");
            if let Some(repo) = repo_url.strip_prefix("https://github.com/") {
                let parts: Vec<&str> = repo.split('/').collect();
                if parts.len() == 2 {
                    return Ok((parts[0].to_string(), parts[1].to_string()));
                }
            }

            anyhow::bail!("Could not parse repository from Cargo.toml")
        }
    }
}
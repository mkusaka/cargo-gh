use clap::Parser;
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
    Ghdist(Args),
}

#[derive(Parser, Debug, Clone)]
#[clap(version, about, long_about = None)]
pub struct Args {
    /// Release tag (e.g., v1.2.3, abcdef0, main, or any git ref)
    /// If not specified, uses the tag on HEAD
    #[clap(short, long)]
    pub tag: Option<String>,

    /// Build targets (comma-separated Rust triple format)
    /// Example: x86_64-unknown-linux-gnu,aarch64-unknown-linux-gnu
    #[clap(short = 'T', long, value_delimiter = ',')]
    pub targets: Option<Vec<String>>,

    /// Archive format (tgz or zip)
    #[clap(short, long, default_value = "tgz")]
    pub format: ArchiveFormat,

    /// Create as draft release
    #[clap(long)]
    pub draft: bool,

    /// Skip cargo publish step
    #[clap(long, default_value_t = true)]
    pub skip_publish: bool,

    /// Don't generate checksum files (SHA256SUMS)
    #[clap(long)]
    pub no_checksum: bool,

    /// Configuration file path
    #[clap(long)]
    pub config: Option<PathBuf>,

    /// Enable verbose output
    #[clap(long)]
    pub verbose: bool,

    /// GitHub repository (owner/repo)
    /// If not specified, uses repository from Cargo.toml
    #[clap(long)]
    pub repository: Option<String>,

    /// GitHub token (can also be set via GITHUB_TOKEN env var)
    #[clap(long, env = "GITHUB_TOKEN")]
    pub github_token: Option<String>,

    /// Binary names to include (if not specified, includes all)
    #[clap(long, value_delimiter = ',')]
    pub bins: Option<Vec<String>>,

    /// Cargo build profile (release, debug, etc.)
    #[clap(long, default_value = "release")]
    pub profile: String,
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

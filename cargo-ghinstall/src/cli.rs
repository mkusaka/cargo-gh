use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[clap(
    name = "cargo-ghinstall",
    version,
    about = "Install binaries from GitHub Releases",
    long_about = None,
    bin_name = "cargo"
)]
pub enum CargoCli {
    #[clap(name = "ghinstall")]
    Ghinstall(Args),
}

#[derive(Parser, Debug, Clone)]
#[clap(version, about, long_about = None)]
pub struct Args {
    /// Repository to install from
    /// Format: owner/repo[@tag]
    /// Examples: rust-lang/rust-analyzer@v1.2.3, owner/repo@abcdef0, owner/repo@main
    #[clap(value_name = "OWNER/REPO[@TAG]")]
    pub repo: String,

    /// Release tag (e.g., v1.2.3, abcdef0, main, or any git ref)
    #[clap(short, long)]
    pub tag: Option<String>,

    /// Binary name or pattern to install
    #[clap(short, long)]
    pub bin: Option<String>,

    /// Install all binaries from the repository
    #[clap(long, conflicts_with = "bin")]
    pub bins: bool,

    /// Target platform triple (e.g., aarch64-apple-darwin)
    #[clap(short = 'T', long)]
    pub target: Option<String>,

    /// Installation directory
    #[clap(short = 'd', long, default_value = "~/.cargo/bin")]
    pub install_dir: String,

    /// Show release notes
    #[clap(long)]
    pub show_notes: bool,

    /// Verify GPG signature if .sig asset is available
    #[clap(long)]
    pub verify_signature: bool,

    /// Disable fallback to cargo install --git
    #[clap(long)]
    pub no_fallback: bool,

    /// Skip SHA256 checksum verification
    #[clap(long)]
    pub skip_checksum: bool,

    /// Configuration file path
    #[clap(long, default_value = ".config/ghinstall.toml")]
    pub config: PathBuf,

    /// Enable verbose output
    #[clap(long)]
    pub verbose: bool,
}

impl Args {
    /// Parse repository string to extract owner, repo, and optional tag
    pub fn parse_repo(&self) -> anyhow::Result<(String, String, Option<String>)> {
        let repo_str = &self.repo;

        // Check if tag is specified with @
        let (repo_part, tag_part) = if let Some(idx) = repo_str.rfind('@') {
            let repo = &repo_str[..idx];
            let tag = &repo_str[idx + 1..];
            (repo, Some(tag.to_string()))
        } else {
            (repo_str.as_str(), None)
        };

        // Split owner/repo
        let parts: Vec<&str> = repo_part.split('/').collect();
        if parts.len() != 2 {
            return Err(crate::error::GhInstallError::InvalidRepo {
                input: self.repo.clone(),
            }
            .into());
        }

        let owner = parts[0].to_string();
        let repo = parts[1].to_string();

        // Combine tag from @ notation or --tag flag
        let final_tag = tag_part.or_else(|| self.tag.clone());

        Ok((owner, repo, final_tag))
    }

    /// Get the installation directory as PathBuf, expanding ~
    pub fn install_dir(&self) -> PathBuf {
        let path = &self.install_dir;
        if path.starts_with("~") {
            if let Some(home) =
                directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf())
            {
                let rest = path.strip_prefix("~").unwrap_or(path);
                let rest = rest.strip_prefix('/').unwrap_or(rest);
                return home.join(rest);
            }
        }
        PathBuf::from(path)
    }

    /// Get the target triple, defaulting to current platform
    pub fn target(&self) -> String {
        self.target.clone().unwrap_or_else(|| {
            // Construct target triple for current platform
            let arch = std::env::consts::ARCH;
            let os = std::env::consts::OS;

            match (arch, os) {
                ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
                ("x86_64", "macos") => "x86_64-apple-darwin",
                ("x86_64", "windows") => "x86_64-pc-windows-msvc",
                ("aarch64", "linux") => "aarch64-unknown-linux-gnu",
                ("aarch64", "macos") => "aarch64-apple-darwin",
                ("aarch64", "windows") => "aarch64-pc-windows-msvc",
                _ => panic!("Unsupported platform: {arch}-{os}"),
            }
            .to_string()
        })
    }
}

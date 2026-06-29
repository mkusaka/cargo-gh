use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::cli::ArchiveFormat;

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct Config {
    #[serde(default)]
    pub default: DefaultConfig,

    #[serde(default)]
    pub repository: RepositoryConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DefaultConfig {
    #[serde(default = "default_profile")]
    pub profile: String,

    #[serde(default = "default_targets")]
    pub targets: Vec<String>,

    #[serde(default = "default_format")]
    pub format: String,

    #[serde(default)]
    pub draft: bool,

    #[serde(default = "default_skip_publish")]
    pub skip_publish: bool,

    #[serde(default = "default_generate_checksum")]
    pub generate_checksum: bool,

    #[serde(default)]
    pub bins: Option<Vec<String>>,
}

impl Default for DefaultConfig {
    fn default() -> Self {
        Self {
            profile: default_profile(),
            targets: default_targets(),
            format: default_format(),
            draft: false,
            skip_publish: default_skip_publish(),
            generate_checksum: default_generate_checksum(),
            bins: None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct RepositoryConfig {
    pub owner: Option<String>,
    pub repo: Option<String>,
}

fn default_targets() -> Vec<String> {
    vec![
        "x86_64-unknown-linux-gnu".to_string(),
        "aarch64-unknown-linux-gnu".to_string(),
    ]
}

fn default_profile() -> String {
    "release".to_string()
}

fn default_format() -> String {
    "tgz".to_string()
}

fn default_skip_publish() -> bool {
    true
}

fn default_generate_checksum() -> bool {
    true
}

impl Config {
    /// Load configuration from file
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Config::default());
        }

        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Get the default configuration file path
    #[allow(dead_code)]
    pub fn default_path() -> PathBuf {
        directories::BaseDirs::new()
            .map(|dirs| dirs.config_dir().join("ghdist.toml"))
            .unwrap_or_else(|| PathBuf::from("~/.config/ghdist.toml"))
    }

    /// Merge configuration with command line arguments
    pub fn merge_with_args(&self, args: &mut crate::cli::Args) -> Result<()> {
        // Apply default configuration
        if args.profile.is_none() {
            args.profile = Some(self.default.profile.clone());
        }

        if args.targets.is_none() && !self.default.targets.is_empty() {
            args.targets = Some(self.default.targets.clone());
        }

        if args.format.is_none() {
            args.format = Some(parse_archive_format(&self.default.format)?);
        }

        if !args.draft && self.default.draft {
            args.draft = true;
        }

        if !args.skip_publish {
            args.skip_publish = self.default.skip_publish;
        }

        if !args.no_checksum && !self.default.generate_checksum {
            args.no_checksum = true;
        }

        if args.bins.is_none() {
            args.bins = self.default.bins.clone();
        }

        // Apply repository configuration
        if args.repository.is_none() {
            if let (Some(owner), Some(repo)) = (&self.repository.owner, &self.repository.repo) {
                args.repository = Some(format!("{owner}/{repo}"));
            }
        }

        Ok(())
    }
}

fn parse_archive_format(value: &str) -> Result<ArchiveFormat> {
    match value {
        "tgz" => Ok(ArchiveFormat::Tgz),
        "zip" => Ok(ArchiveFormat::Zip),
        other => anyhow::bail!("Unsupported archive format in config: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_load_config() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("test.toml");

        let config_content = r#"
[default]
profile = "dist"
targets = ["x86_64-unknown-linux-gnu", "x86_64-apple-darwin"]
format = "zip"
draft = true
skip_publish = false
generate_checksum = false
bins = ["cargo-ghdist"]

[repository]
owner = "test-org"
repo = "test-crate"
"#;

        fs::write(&config_path, config_content).unwrap();

        let config = Config::load(&config_path).unwrap();

        assert_eq!(config.default.profile, "dist");
        assert_eq!(config.default.targets.len(), 2);
        assert_eq!(config.default.format, "zip");
        assert!(config.default.draft);
        assert!(!config.default.skip_publish);
        assert!(!config.default.generate_checksum);
        assert_eq!(config.default.bins, Some(vec!["cargo-ghdist".to_string()]));

        assert_eq!(config.repository.owner, Some("test-org".to_string()));
        assert_eq!(config.repository.repo, Some("test-crate".to_string()));
    }

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.default.profile, "release");
        assert_eq!(config.default.targets.len(), 2);
        assert_eq!(config.default.format, "tgz");
        assert!(!config.default.draft);
        assert!(config.default.skip_publish);
        assert!(config.default.generate_checksum);
        assert_eq!(config.default.bins, None);
    }

    #[test]
    fn test_merge_config_with_args() {
        let config = Config {
            default: DefaultConfig {
                profile: "dist".to_string(),
                targets: vec!["x86_64-apple-darwin".to_string()],
                format: "zip".to_string(),
                draft: true,
                skip_publish: false,
                generate_checksum: false,
                bins: Some(vec!["cargo-ghdist".to_string()]),
            },
            repository: RepositoryConfig {
                owner: Some("owner".to_string()),
                repo: Some("repo".to_string()),
            },
        };

        let mut args = crate::cli::Args {
            tag: None,
            hash: false,
            targets: None,
            format: None,
            draft: false,
            skip_publish: false,
            no_checksum: false,
            config: None,
            verbose: false,
            repository: None,
            github_token: None,
            bins: None,
            profile: None,
        };

        config.merge_with_args(&mut args).unwrap();

        assert_eq!(args.profile(), "dist");
        assert_eq!(args.targets(), vec!["x86_64-apple-darwin"]);
        assert_eq!(args.archive_format(), ArchiveFormat::Zip);
        assert!(args.draft);
        assert!(!args.skip_publish);
        assert!(args.no_checksum);
        assert_eq!(args.repository, Some("owner/repo".to_string()));
        assert_eq!(args.bins, Some(vec!["cargo-ghdist".to_string()]));
    }
}

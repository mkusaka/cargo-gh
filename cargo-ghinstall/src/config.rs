use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct Config {
    #[serde(default)]
    pub default: DefaultConfig,

    #[serde(default)]
    pub repo: HashMap<String, RepoConfig>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DefaultConfig {
    #[serde(default = "default_install_dir")]
    pub install_dir: String,

    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

impl Default for DefaultConfig {
    fn default() -> Self {
        Self {
            install_dir: default_install_dir(),
            timeout: default_timeout(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct RepoConfig {
    pub bin: Option<String>,
    pub targets: Option<Vec<String>>,
    #[serde(default)]
    pub verify_signature: bool,
}

fn default_install_dir() -> String {
    "~/.cargo/bin".to_string()
}

fn default_timeout() -> u64 {
    30
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
    pub fn default_path() -> PathBuf {
        directories::BaseDirs::new()
            .map(|dirs| dirs.config_dir().join("ghinstall.toml"))
            .unwrap_or_else(|| PathBuf::from("~/.config/ghinstall.toml"))
    }

    /// Get repository-specific configuration
    pub fn get_repo_config(&self, owner: &str, repo: &str) -> Option<&RepoConfig> {
        let key = format!("{owner}/{repo}");
        self.repo.get(&key)
    }

    /// Merge configuration with command line arguments
    pub fn merge_with_args(&self, args: &mut crate::cli::Args, owner: &str, repo: &str) {
        // Apply default configuration
        if args.install_dir == "~/.cargo/bin" && self.default.install_dir != "~/.cargo/bin" {
            args.install_dir = self.default.install_dir.clone();
        }

        // Apply repository-specific configuration
        if let Some(repo_config) = self.get_repo_config(owner, repo) {
            if args.bin.is_none() && repo_config.bin.is_some() {
                args.bin = repo_config.bin.clone();
            }

            if !args.verify_signature && repo_config.verify_signature {
                args.verify_signature = true;
            }
        }
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
install_dir = "/usr/local/bin"
timeout = 60

[repo."owner/repo"]
bin = "mybin"
targets = ["x86_64-unknown-linux-gnu"]
verify_signature = true
"#;

        fs::write(&config_path, config_content).unwrap();

        let config = Config::load(&config_path).unwrap();

        assert_eq!(config.default.install_dir, "/usr/local/bin");
        assert_eq!(config.default.timeout, 60);

        let repo_config = config.get_repo_config("owner", "repo").unwrap();
        assert_eq!(repo_config.bin, Some("mybin".to_string()));
        assert!(repo_config.verify_signature);
    }

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.default.install_dir, "~/.cargo/bin");
        assert_eq!(config.default.timeout, 30);
    }
}

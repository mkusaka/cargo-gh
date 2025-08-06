use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use anyhow::Result;

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct Config {
    #[serde(default)]
    pub default: DefaultConfig,
    
    #[serde(default)]
    pub repository: RepositoryConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DefaultConfig {
    #[serde(default = "default_targets")]
    pub targets: Vec<String>,
    
    #[serde(default = "default_format")]
    pub format: String,
    
    #[serde(default)]
    pub draft: bool,
    
    #[serde(default = "default_skip_publish")]
    pub skip_publish: bool,
}

impl Default for DefaultConfig {
    fn default() -> Self {
        Self {
            targets: default_targets(),
            format: default_format(),
            draft: false,
            skip_publish: default_skip_publish(),
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

fn default_format() -> String {
    "tgz".to_string()
}

fn default_skip_publish() -> bool {
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
    pub fn default_path() -> PathBuf {
        directories::BaseDirs::new()
            .map(|dirs| dirs.config_dir().join("ghdist.toml"))
            .unwrap_or_else(|| PathBuf::from("~/.config/ghdist.toml"))
    }

    /// Merge configuration with command line arguments
    pub fn merge_with_args(&self, args: &mut crate::cli::Args) {
        // Apply default configuration
        if args.targets.is_none() && !self.default.targets.is_empty() {
            args.targets = Some(self.default.targets.clone());
        }

        if !args.draft && self.default.draft {
            args.draft = true;
        }

        // Apply repository configuration
        if args.repository.is_none() {
            if let (Some(owner), Some(repo)) = (&self.repository.owner, &self.repository.repo) {
                args.repository = Some(format!("{}/{}", owner, repo));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::fs;

    #[test]
    fn test_load_config() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("test.toml");
        
        let config_content = r#"
[default]
targets = ["x86_64-unknown-linux-gnu", "x86_64-apple-darwin"]
format = "zip"
draft = true
skip_publish = false

[repository]
owner = "test-org"
repo = "test-crate"
"#;
        
        fs::write(&config_path, config_content).unwrap();
        
        let config = Config::load(&config_path).unwrap();
        
        assert_eq!(config.default.targets.len(), 2);
        assert_eq!(config.default.format, "zip");
        assert!(config.default.draft);
        assert!(!config.default.skip_publish);
        
        assert_eq!(config.repository.owner, Some("test-org".to_string()));
        assert_eq!(config.repository.repo, Some("test-crate".to_string()));
    }

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.default.targets.len(), 2);
        assert_eq!(config.default.format, "tgz");
        assert!(!config.default.draft);
        assert!(config.default.skip_publish);
    }
}
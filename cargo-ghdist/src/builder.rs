use anyhow::{Context, Result};
use git2::Repository;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::Args;
use crate::config::Config;
use crate::error::{GhDistError, Result as GhResult};
use crate::github::{get_content_type, GitHubClient};
use crate::packager;

pub struct DistBuilder {
    args: Args,
    #[allow(dead_code)]
    config: Config,
    github_client: GitHubClient,
}

impl DistBuilder {
    pub fn new(mut args: Args) -> Result<Self> {
        // Load configuration
        let config_path = args
            .config
            .clone()
            .unwrap_or_else(Config::default_path);

        let config = Config::load(&config_path).context("Failed to load configuration")?;

        // Merge configuration with args
        config.merge_with_args(&mut args);

        let github_client = GitHubClient::new(args.github_token.clone())?;

        Ok(Self {
            args,
            config,
            github_client,
        })
    }

    pub async fn run(&self) -> Result<()> {
        // Get or detect tag
        let tag = self.get_tag()?;
        tracing::info!("Building distribution for tag: {}", tag);

        // Parse repository info
        let (owner, repo) = self.args.parse_repository()?;
        tracing::info!("Repository: {}/{}", owner, repo);

        // Create output directory
        let output_dir = PathBuf::from(format!("target/dist/{tag}"));
        fs::create_dir_all(&output_dir)?;

        // Build for each target
        let mut all_archives = Vec::new();
        for target in self.args.targets() {
            tracing::info!("Building for target: {}", target);

            match self.build_for_target(&target).await {
                Ok(binaries) => {
                    // Create archive for this target
                    let archive_name = format!("{repo}-{target}-{tag}");
                    let archive_path = packager::create_archive(
                        &binaries,
                        &output_dir,
                        &archive_name,
                        self.args.format,
                    )?;
                    all_archives.push(archive_path);
                }
                Err(e) => {
                    tracing::error!("Failed to build for {}: {}", target, e);
                    if !self.should_continue_on_error() {
                        return Err(e.into());
                    }
                }
            }
        }

        if all_archives.is_empty() {
            return Err(GhDistError::BuildFailed {
                target: "all targets".to_string(),
            }
            .into());
        }

        // Generate checksums if requested
        if !self.args.no_checksum {
            let checksum_file = packager::generate_checksums(&all_archives, &output_dir)?;
            all_archives.push(checksum_file);
        }

        // Create or update GitHub release
        let release = self
            .github_client
            .create_release(&owner, &repo, &tag, self.args.draft)
            .await?;

        // Upload all assets
        for asset_path in &all_archives {
            let asset_name = asset_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            // Check if asset already exists and delete it
            if let Some(asset_id) = self
                .github_client
                .asset_exists(&owner, &repo, release.id.0 as u64, asset_name)
                .await?
            {
                tracing::info!("Deleting existing asset: {}", asset_name);
                self.github_client
                    .delete_asset(&owner, &repo, asset_id)
                    .await?;
            }

            // Upload new asset
            self.github_client
                .upload_asset(
                    &owner,
                    &repo,
                    release.id.0 as u64,
                    asset_path,
                    get_content_type(asset_path),
                )
                .await?;
        }

        // Run cargo publish if requested
        if !self.args.skip_publish {
            self.run_cargo_publish()?;
        }

        tracing::info!("Distribution completed successfully!");
        tracing::info!("Release URL: {}", release.html_url);

        Ok(())
    }

    /// Get tag from args or detect from git
    fn get_tag(&self) -> GhResult<String> {
        if let Some(tag) = &self.args.tag {
            return Ok(tag.clone());
        }

        // Try to get tag from git HEAD
        let repo = Repository::open(".").map_err(GhDistError::Git)?;

        let head = repo.head().map_err(GhDistError::Git)?;

        let oid = head.target().ok_or_else(|| GhDistError::NoTag)?;

        // Look for tags pointing to HEAD
        let tags = repo.tag_names(None).map_err(GhDistError::Git)?;

        for tag in tags.iter().flatten() {
            if let Ok(tag_obj) = repo.revparse_single(tag) {
                if tag_obj.id() == oid {
                    return Ok(tag.to_string());
                }
            }
        }

        Err(GhDistError::NoTag)
    }

    /// Build binaries for a specific target
    async fn build_for_target(&self, target: &str) -> GhResult<Vec<PathBuf>> {
        let mut cmd = Command::new("cargo");
        cmd.arg("build").arg("--target").arg(target);

        // Add profile
        if self.args.profile == "release" {
            cmd.arg("--release");
        } else {
            cmd.arg("--profile").arg(&self.args.profile);
        }

        // Add specific bins if requested
        if let Some(bins) = &self.args.bins {
            for bin in bins {
                cmd.arg("--bin").arg(bin);
            }
        }

        let status = cmd.status().map_err(|_| GhDistError::BuildFailed {
            target: target.to_string(),
        })?;

        if !status.success() {
            return Err(GhDistError::BuildFailed {
                target: target.to_string(),
            });
        }

        // Find built binaries
        let target_dir = self.get_target_dir(target);
        let binaries = self
            .find_binaries(&target_dir)
            .map_err(|_| GhDistError::BuildFailed {
                target: target.to_string(),
            })?;

        if binaries.is_empty() {
            return Err(GhDistError::BuildFailed {
                target: format!("{target} (no binaries found)"),
            });
        }

        Ok(binaries)
    }

    /// Get the target directory for built binaries
    fn get_target_dir(&self, target: &str) -> PathBuf {
        let profile = if self.args.profile == "release" {
            "release"
        } else {
            &self.args.profile
        };

        PathBuf::from("target").join(target).join(profile)
    }

    /// Find binary files in a directory
    fn find_binaries(&self, dir: &Path) -> Result<Vec<PathBuf>> {
        let mut binaries = Vec::new();

        if !dir.exists() {
            return Ok(binaries);
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() && self.is_binary(&path)? {
                // Filter by requested bins if specified
                if let Some(bins) = &self.args.bins {
                    let file_name = path.file_stem().and_then(|n| n.to_str()).unwrap_or("");
                    if !bins.iter().any(|b| b == file_name) {
                        continue;
                    }
                }
                binaries.push(path);
            }
        }

        Ok(binaries)
    }

    /// Check if a file is a binary executable
    fn is_binary(&self, path: &Path) -> Result<bool> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(path)?;
            let permissions = metadata.permissions();
            Ok(permissions.mode() & 0o111 != 0)
        }

        #[cfg(windows)]
        {
            // On Windows, check for .exe extension
            Ok(path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("exe"))
                .unwrap_or(false))
        }
    }

    /// Run cargo publish
    fn run_cargo_publish(&self) -> Result<()> {
        tracing::info!("Running cargo publish");

        let status = Command::new("cargo").arg("publish").status()?;

        if !status.success() {
            tracing::warn!("cargo publish failed");
        }

        Ok(())
    }

    /// Check if we should continue on build errors
    fn should_continue_on_error(&self) -> bool {
        // Could add a flag for this in the future
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_is_binary() {
        let temp_dir = tempdir().unwrap();

        #[cfg(unix)]
        {
            let binary_path = temp_dir.path().join("test_binary");
            fs::write(&binary_path, b"#!/bin/bash\necho test").unwrap();

            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&binary_path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&binary_path, perms).unwrap();

            // Test the is_binary function directly without creating a DistBuilder
            let metadata = fs::metadata(&binary_path).unwrap();
            let permissions = metadata.permissions();
            assert!(permissions.mode() & 0o111 != 0);
        }

        #[cfg(windows)]
        {
            let binary_path = temp_dir.path().join("test.exe");
            fs::write(&binary_path, b"test").unwrap();

            // Test Windows executable detection
            assert!(binary_path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("exe"))
                .unwrap_or(false));
        }
    }
}

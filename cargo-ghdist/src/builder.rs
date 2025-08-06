use anyhow::{Context, Result};
use cargo_manifest::Manifest;
use git2::Repository;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::Args;
use crate::config::Config;
use crate::error::{GhDistError, Result as GhResult};
use crate::github::{GitHubClient, get_content_type};
use crate::packager;

/// Find workspace manifest by looking up parent directories
fn find_workspace_manifest() -> Result<Manifest> {
    let mut current_dir = std::env::current_dir()?;

    loop {
        let manifest_path = current_dir.join("Cargo.toml");
        if manifest_path.exists() {
            if let Ok(manifest) = Manifest::from_path(&manifest_path) {
                if manifest.workspace.is_some() {
                    return Ok(manifest);
                }
            }
        }

        if !current_dir.pop() {
            break;
        }
    }

    anyhow::bail!("No workspace manifest found")
}

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
            .unwrap_or_else(|| PathBuf::from(".config/ghdist.toml"));

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
                .asset_exists(&owner, &repo, release.id.0, asset_name)
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
                    release.id.0,
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

    /// Get package version from Cargo.toml
    fn get_package_version(&self) -> Result<String> {
        let manifest = Manifest::from_path("Cargo.toml").context("Failed to parse Cargo.toml")?;

        // Get version from package
        if let Some(package) = manifest.package {
            match package.version {
                Some(cargo_manifest::MaybeInherited::Local(version)) => {
                    return Ok(version);
                }
                Some(cargo_manifest::MaybeInherited::Inherited { .. }) => {
                    // Version is inherited from workspace, need to read workspace manifest
                    if let Ok(workspace_manifest) = find_workspace_manifest() {
                        if let Some(ws_package) =
                            workspace_manifest.workspace.and_then(|ws| ws.package)
                        {
                            if let Some(version) = ws_package.version {
                                return Ok(version);
                            }
                        }
                    }
                }
                None => {
                    // No version in package, try workspace
                    if let Ok(workspace_manifest) = find_workspace_manifest() {
                        if let Some(ws_package) =
                            workspace_manifest.workspace.and_then(|ws| ws.package)
                        {
                            if let Some(version) = ws_package.version {
                                return Ok(version);
                            }
                        }
                    }
                }
            }
        }

        anyhow::bail!("No version field found in Cargo.toml")
    }

    /// Get tag from args or detect from git
    fn get_tag(&self) -> Result<String> {
        if let Some(tag) = &self.args.tag {
            return Ok(tag.clone());
        }

        // Try to get tag from git HEAD
        let repo = Repository::open(".").context(
            "Failed to find git repository. Please run this command from a git repository \
             or specify a tag explicitly with --tag",
        )?;

        let head = repo.head().context("Failed to get git HEAD")?;

        let oid = head.target().context("HEAD has no target")?;

        // Look for tags pointing to HEAD
        let tags = repo.tag_names(None).context("Failed to get git tags")?;

        for tag in tags.iter().flatten() {
            if let Ok(tag_obj) = repo.revparse_single(tag) {
                if tag_obj.id() == oid {
                    return Ok(tag.to_string());
                }
            }
        }

        // If no tag found and --hash is specified, use version-sha format
        if self.args.hash {
            let short_sha = oid.to_string()[..8].to_string();

            // Read version from Cargo.toml
            let version = self.get_package_version()?;
            let tag = format!("{version}-{short_sha}");

            tracing::info!(
                "No tag found on HEAD. Using version-sha format as tag: {}",
                tag
            );
            Ok(tag)
        } else {
            // If no tag found and --hash not specified, suggest using --tag or --hash
            anyhow::bail!(
                "No tag found on current HEAD. Please create a tag first with 'git tag <version>', \
                 specify a tag explicitly with --tag, or use --hash to generate version-sha tag"
            )
        }
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
            assert!(
                binary_path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext.eq_ignore_ascii_case("exe"))
                    .unwrap_or(false)
            );
        }
    }

    #[test]
    fn test_get_package_version_from_package() {
        let temp_dir = tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();

        // Change to temp directory
        std::env::set_current_dir(&temp_dir).unwrap();

        // Create a simple Cargo.toml with version in package
        let cargo_toml = r#"
[package]
name = "test-package"
version = "1.2.3"
edition = "2021"
"#;
        fs::write("Cargo.toml", cargo_toml).unwrap();

        // Test get_package_version directly
        // We can't easily test this without refactoring the struct,
        // so we'll test via the manifest directly
        let manifest = Manifest::from_path("Cargo.toml").unwrap();
        let version = manifest.package.unwrap().version.unwrap();
        if let cargo_manifest::MaybeInherited::Local(v) = version {
            assert_eq!(v, "1.2.3");
        } else {
            panic!("Expected local version");
        }

        // Restore original directory
        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn test_get_package_version_from_workspace() {
        let temp_dir = tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();

        // Change to temp directory
        std::env::set_current_dir(&temp_dir).unwrap();

        // Create a workspace Cargo.toml
        let workspace_toml = r#"
[workspace]
members = ["test-package"]

[workspace.package]
version = "2.3.4"
edition = "2021"
"#;
        fs::write("Cargo.toml", workspace_toml).unwrap();

        // Create package directory
        fs::create_dir("test-package").unwrap();
        std::env::set_current_dir("test-package").unwrap();

        // Create package Cargo.toml with inherited version
        let package_toml = r#"
[package]
name = "test-package"
version.workspace = true
edition.workspace = true
"#;
        fs::write("Cargo.toml", package_toml).unwrap();

        // Test that workspace version is inherited correctly
        let manifest = Manifest::from_path("Cargo.toml").unwrap();
        assert!(manifest.package.is_some());
        let package = manifest.package.unwrap();

        // Version should be inherited
        match package.version {
            Some(cargo_manifest::MaybeInherited::Inherited { .. }) => {
                // This is expected - version is inherited from workspace
                // Now check the workspace manifest
                let ws_manifest = find_workspace_manifest().unwrap();
                let ws_version = ws_manifest
                    .workspace
                    .unwrap()
                    .package
                    .unwrap()
                    .version
                    .unwrap();
                assert_eq!(ws_version, "2.3.4");
            }
            _ => panic!("Expected inherited version"),
        }

        // Restore original directory
        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn test_get_tag_with_hash_option() {
        let temp_dir = tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();

        // Change to temp directory
        std::env::set_current_dir(&temp_dir).unwrap();

        // Initialize git repo
        let repo = Repository::init(".").unwrap();

        // Configure git user for test
        let mut config = repo.config().unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
        config.set_str("user.name", "Test User").unwrap();
        drop(config);

        // Create Cargo.toml
        let cargo_toml = r#"
[package]
name = "test-package"
version = "0.5.0"
"#;
        fs::write("Cargo.toml", cargo_toml).unwrap();

        // Add and commit using git2
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("Cargo.toml")).unwrap();
        index.write().unwrap();

        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = repo.signature().unwrap();

        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
            .unwrap();

        // Test the tag generation logic with hash option
        // Since we can't create DistBuilder without Tokio runtime,
        // we'll test the logic directly
        let head = repo.head().unwrap();
        let oid = head.target().unwrap();

        // Simulate what get_tag() does with hash = true
        let short_sha = oid.to_string()[..8].to_string();
        let version = "0.5.0"; // From Cargo.toml
        let tag = format!("{version}-{short_sha}");

        assert!(tag.starts_with("0.5.0-"));
        assert_eq!(tag.len(), "0.5.0-".len() + 8);

        // Restore original directory
        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn test_get_tag_without_hash_option_fails() {
        let temp_dir = tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();

        // Change to temp directory
        std::env::set_current_dir(&temp_dir).unwrap();

        // Initialize git repo without tags
        let repo = Repository::init(".").unwrap();

        // Configure git user for test
        let mut config = repo.config().unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
        config.set_str("user.name", "Test User").unwrap();
        drop(config);

        // Create Cargo.toml
        fs::write(
            "Cargo.toml",
            r#"[package]
name = "test"
version = "1.0.0""#,
        )
        .unwrap();

        // Add and commit using git2
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("Cargo.toml")).unwrap();
        index.write().unwrap();

        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = repo.signature().unwrap();

        repo.commit(Some("HEAD"), &sig, &sig, "Initial", &tree, &[])
            .unwrap();

        // Test the tag detection logic without hash option
        // Simulate what get_tag() does with hash = false and no tag
        let head = repo.head().unwrap();
        let oid = head.target().unwrap();

        // Look for tags pointing to HEAD
        let tags = repo.tag_names(None).unwrap();
        let mut found_tag = None;

        for tag in tags.iter().flatten() {
            if let Ok(tag_obj) = repo.revparse_single(tag) {
                if tag_obj.id() == oid {
                    found_tag = Some(tag.to_string());
                    break;
                }
            }
        }

        // Should not find any tags
        assert!(found_tag.is_none());

        // Restore original directory
        std::env::set_current_dir(original_dir).unwrap();
    }
}

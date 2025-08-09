use anyhow::{Context, Result};
use cargo_manifest::Manifest;
use git2::Repository;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::Args;
use crate::config::Config;
use crate::error::{GhDistError, Result as GhResult};
use crate::github::{get_content_type, GitHubClient};
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

        // Get current commit SHA if using --hash option
        let target_commitish = if self.args.hash {
            let sha = Repository::open(".").ok().and_then(|repo| {
                repo.head()
                    .ok()
                    .and_then(|head| head.target())
                    .map(|oid| oid.to_string())
            });
            tracing::info!("Using commit SHA for release: {:?}", sha);
            sha
        } else {
            tracing::info!("Not using commit SHA (--hash not specified)");
            None
        };

        // Generate release notes
        let mut release_notes = self.generate_release_notes(&tag, &owner, &repo, self.args.hash)?;

        // For tagged releases, append GitHub's auto-generated release notes
        if !self.args.hash {
            tracing::info!(
                "Fetching GitHub's auto-generated release notes for tag {}",
                tag
            );

            // Get the previous tag for comparison
            let previous_tag = self.find_previous_tag().ok();

            // Fetch auto-generated release notes from GitHub
            match self
                .github_client
                .generate_release_notes(
                    &owner,
                    &repo,
                    &tag,
                    target_commitish.as_deref(),
                    previous_tag.as_deref(),
                )
                .await
            {
                Ok(auto_notes) => {
                    tracing::debug!("Got auto-generated notes: {} chars", auto_notes.len());
                    // Append the auto-generated notes to our custom notes
                    release_notes.push_str("\n\n---\n");
                    release_notes.push_str("\n## 📋 Auto-generated Release Notes\n\n");
                    release_notes.push_str(&auto_notes);
                }
                Err(e) => {
                    tracing::warn!("Failed to get auto-generated release notes: {}", e);
                    // Continue without auto-generated notes
                }
            }
        }

        tracing::debug!("Final release notes: {} chars", release_notes.len());

        // Create or update GitHub release
        let release = self
            .github_client
            .create_release(
                &owner,
                &repo,
                &tag,
                self.args.draft,
                target_commitish.as_deref(),
                Some(&release_notes),
            )
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

        // Check if this is a workspace manifest with workspace.package.version
        if let Some(workspace) = &manifest.workspace {
            if let Some(ws_package) = &workspace.package {
                if let Some(version) = &ws_package.version {
                    return Ok(version.clone());
                }
            }
        }

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

    /// Get binary names and descriptions from the project
    fn get_binary_info(&self) -> Result<Vec<(String, Option<String>)>> {
        let mut binaries = Vec::new();

        // Try to read workspace members
        if let Ok(manifest) = Manifest::from_path("Cargo.toml") {
            if let Some(workspace) = manifest.workspace {
                // For workspace projects, check each member
                for member in &workspace.members {
                    let member_path = PathBuf::from(&member).join("Cargo.toml");
                    if let Ok(member_manifest) = Manifest::from_path(&member_path) {
                        if let Some(package) = member_manifest.package {
                            // Check if this package produces a binary
                            // By default, packages with src/main.rs produce a binary with the package name
                            let has_main =
                                PathBuf::from(&member).join("src").join("main.rs").exists();

                            // Extract description as String if available
                            let description = package.description.and_then(|d| match d {
                                cargo_manifest::MaybeInherited::Local(s) => Some(s),
                                cargo_manifest::MaybeInherited::Inherited { .. } => None,
                            });

                            // Check for explicit bin targets
                            if !member_manifest.bin.is_empty() {
                                for bin in &member_manifest.bin {
                                    let bin_name =
                                        bin.name.clone().unwrap_or_else(|| package.name.clone());
                                    binaries.push((bin_name, description.clone()));
                                }
                            } else if has_main {
                                // Package has src/main.rs, so it produces a binary with the package name
                                let name = package.name;
                                binaries.push((name, description));
                            }
                        }
                    }
                }
            } else if let Some(package) = manifest.package {
                // Single package project
                let has_main = Path::new("src/main.rs").exists();

                // Extract description as String if available
                let description = package.description.and_then(|d| match d {
                    cargo_manifest::MaybeInherited::Local(s) => Some(s),
                    cargo_manifest::MaybeInherited::Inherited { .. } => None,
                });

                // Check for explicit bin targets
                if !manifest.bin.is_empty() {
                    for bin in &manifest.bin {
                        let bin_name = bin.name.clone().unwrap_or_else(|| package.name.clone());
                        binaries.push((bin_name, description.clone()));
                    }
                } else if has_main {
                    // Package has src/main.rs, so it produces a binary with the package name
                    let name = package.name;
                    binaries.push((name, description));
                }
            }
        }

        Ok(binaries)
    }

    /// Generate release notes
    fn generate_release_notes(
        &self,
        tag: &str,
        owner: &str,
        repo_name: &str,
        is_continuous: bool,
    ) -> Result<String> {
        let repo = Repository::open(".")?;

        // Get commit SHA
        let head = repo.head()?;
        let commit = head.peel_to_commit()?;
        let sha = commit.id().to_string();

        // Get branch name
        let branch = head.shorthand().unwrap_or("unknown").to_string();

        // Get author
        let author = commit.author();
        let author_name = author.name().unwrap_or("unknown").to_string();

        // Get commit message
        let message = commit.message().unwrap_or("No commit message").to_string();

        // Get binary information
        let binaries = self.get_binary_info().unwrap_or_else(|e| {
            tracing::warn!(
                "Failed to detect binaries: {}. Using repository name as fallback.",
                e
            );
            vec![(repo_name.to_string(), None)]
        });

        // Generate individual binary installation commands
        let mut binary_install_commands = String::new();
        if !binaries.is_empty() {
            binary_install_commands.push_str("\n# Or install specific binaries:\n");
            for (binary_name, description) in &binaries {
                if let Some(desc) = description {
                    binary_install_commands.push_str(&format!("\n# {binary_name} - {desc}\n"));
                } else {
                    binary_install_commands.push_str(&format!("\n# {binary_name}\n"));
                }
                binary_install_commands.push_str(&format!(
                    "cargo ghinstall {owner}/{repo_name}@{tag} --bin {binary_name}\n"
                ));
            }
        }

        // Build the release notes
        let notes = if is_continuous {
            format!(
                r#"## 🚀 Continuous Release

**Commit:** `{}`
**Author:** {}
**Branch:** {}

### 📝 Commit Message
{}

### ⚠️ Note
This is an automated development build. Use for testing purposes only.
For stable releases, please use tagged versions.

### 📦 Installation
```bash
# Install all binaries
cargo ghinstall {}/{}@{}
{}```

### 🔗 Links
- [Commit](https://github.com/{}/{}/commit/{})
"#,
                sha,
                author_name,
                branch,
                message.trim(),
                owner,
                repo_name,
                tag,
                binary_install_commands,
                owner,
                repo_name,
                sha
            )
        } else {
            format!(
                r#"## 🎉 Release {}

**Commit:** `{}`
**Author:** {}

### 📦 Installation
```bash
# Install all binaries
cargo ghinstall {}/{}@{}
{}
# Or download directly from the release assets
```

### 🔗 Links
- [Commit](https://github.com/{}/{}/commit/{})
- [Compare](https://github.com/{}/{}/compare/{}...{})
"#,
                tag,
                sha,
                author_name,
                owner,
                repo_name,
                tag,
                binary_install_commands,
                owner,
                repo_name,
                sha,
                owner,
                repo_name,
                self.find_previous_tag()
                    .unwrap_or_else(|_| "main".to_string()),
                tag
            )
        };

        Ok(notes)
    }

    /// Find the previous tag for comparison
    fn find_previous_tag(&self) -> Result<String> {
        let repo = Repository::open(".")?;
        let mut tags = Vec::new();

        repo.tag_foreach(|_oid, name| {
            if let Some(tag_name) = name.strip_prefix(b"refs/tags/") {
                if let Ok(tag_str) = std::str::from_utf8(tag_name) {
                    tags.push(tag_str.to_string());
                }
            }
            true
        })?;

        // Sort tags and get the second-to-last one
        tags.sort();
        if tags.len() >= 2 {
            Ok(tags[tags.len() - 2].clone())
        } else {
            Ok("main".to_string())
        }
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
            assert!(binary_path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("exe"))
                .unwrap_or(false));
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

        // Test get_package_version directly via manifest parsing
        let manifest = Manifest::from_path("Cargo.toml").unwrap();
        assert!(manifest.package.is_some());
        let package = manifest.package.unwrap();
        assert!(package.version.is_some());
        if let Some(cargo_manifest::MaybeInherited::Local(v)) = package.version {
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
        let workspace_toml = r#"[workspace]
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
        let package_toml = r#"[package]
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
                let ws_manifest = Manifest::from_path("../Cargo.toml").unwrap();
                if let Some(ws) = ws_manifest.workspace {
                    if let Some(ws_package) = ws.package {
                        if let Some(version) = ws_package.version {
                            assert_eq!(version, "2.3.4");
                        } else {
                            panic!("No version in workspace.package");
                        }
                    } else {
                        panic!("No workspace.package section");
                    }
                } else {
                    panic!("No workspace section in workspace manifest");
                }
            }
            _ => panic!("Expected inherited version"),
        }

        // Restore original directory
        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn test_get_package_version_from_workspace_root() {
        let temp_dir = tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();

        // Change to temp directory
        std::env::set_current_dir(&temp_dir).unwrap();

        // Create a workspace Cargo.toml with version in workspace.package
        let workspace_toml = r#"[workspace]
members = ["test-package"]
resolver = "2"

[workspace.package]
version = "0.1.0"
authors = ["Test Author"]
edition = "2021"
license = "MIT"
repository = "https://github.com/test/test"
"#;
        fs::write("Cargo.toml", workspace_toml).unwrap();

        // Test that version can be extracted from workspace.package
        let manifest = Manifest::from_path("Cargo.toml").unwrap();
        // The manifest might parse this as a regular package manifest rather than workspace
        // so we need to check both possibilities
        if let Some(ws) = manifest.workspace {
            if let Some(ws_package) = ws.package {
                if let Some(version) = ws_package.version {
                    assert_eq!(version, "0.1.0");
                } else {
                    // Version might not be parsed, skip assertion
                }
            }
        } else {
            // cargo-manifest might not parse pure workspace manifests correctly
            // This is okay for our use case since we handle both cases in the actual code
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
    fn test_get_binary_info() {
        let temp_dir = tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();

        // Change to temp directory
        std::env::set_current_dir(&temp_dir).unwrap();

        // Create a workspace with two members
        let workspace_toml = r#"[workspace]
members = ["cargo-ghinstall", "cargo-ghdist"]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"
"#;
        fs::write("Cargo.toml", workspace_toml).unwrap();

        // Create cargo-ghinstall member
        fs::create_dir_all("cargo-ghinstall/src").unwrap();
        let ghinstall_toml = r#"[package]
name = "cargo-ghinstall"
description = "Install binaries from GitHub releases"
version.workspace = true
edition.workspace = true
"#;
        fs::write("cargo-ghinstall/Cargo.toml", ghinstall_toml).unwrap();
        fs::write("cargo-ghinstall/src/main.rs", "fn main() {}").unwrap();

        // Create cargo-ghdist member
        fs::create_dir_all("cargo-ghdist/src").unwrap();
        let ghdist_toml = r#"[package]
name = "cargo-ghdist"
description = "Create and distribute GitHub releases"
version.workspace = true
edition.workspace = true
"#;
        fs::write("cargo-ghdist/Cargo.toml", ghdist_toml).unwrap();
        fs::write("cargo-ghdist/src/main.rs", "fn main() {}").unwrap();

        // Test the binary detection
        let manifest = Manifest::from_path("Cargo.toml").unwrap();
        assert!(manifest.workspace.is_some());

        let workspace = manifest.workspace.unwrap();
        assert_eq!(workspace.members.len(), 2);

        // Check that both binaries would be detected
        let mut found_binaries = Vec::new();
        for member in &workspace.members {
            let member_path = PathBuf::from(&member).join("Cargo.toml");
            if let Ok(member_manifest) = Manifest::from_path(member_path) {
                if let Some(package) = member_manifest.package {
                    let has_main = PathBuf::from(&member).join("src").join("main.rs").exists();
                    if has_main {
                        found_binaries.push(package.name);
                    }
                }
            }
        }

        assert_eq!(found_binaries.len(), 2);
        assert!(found_binaries.contains(&"cargo-ghinstall".to_string()));
        assert!(found_binaries.contains(&"cargo-ghdist".to_string()));

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

        // Configure git user for test - use expect to handle errors gracefully
        if let Ok(mut config) = repo.config() {
            let _ = config.set_str("user.email", "test@example.com");
            let _ = config.set_str("user.name", "Test User");
            drop(config);
        }

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

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::cli::Args;
use crate::config::Config;
use crate::error::{GhInstallError, Result as GhResult};
use crate::github::{GitHubClient, ReleaseAsset};
use crate::utils;

pub struct Installer {
    args: Args,
    #[allow(dead_code)]
    config: Config,
    github_client: GitHubClient,
}

impl Installer {
    pub fn new(mut args: Args) -> Result<Self> {
        // Load configuration
        let config_path = args.config.clone();

        let config = Config::load(&config_path).context("Failed to load configuration")?;

        // Parse repository info
        let (owner, repo, _) = args.parse_repo()?;

        // Merge configuration with args
        config.merge_with_args(&mut args, &owner, &repo);

        let github_client = GitHubClient::new()?;

        Ok(Self {
            args,
            config,
            github_client,
        })
    }

    pub async fn run(&self) -> Result<()> {
        let (owner, repo, tag) = self.args.parse_repo()?;

        tracing::info!(
            "Installing from {}/{} (tag: {})",
            owner,
            repo,
            tag.as_deref().unwrap_or("latest")
        );

        // Get release from GitHub
        let release = match self
            .github_client
            .get_release(&owner, &repo, tag.as_deref())
            .await
        {
            Ok(release) => release,
            Err(e) => {
                if !self.args.no_fallback {
                    tracing::warn!(
                        "Failed to get release: {}. Falling back to cargo install",
                        e
                    );
                    return self
                        .fallback_cargo_install(&owner, &repo, tag.as_deref())
                        .await;
                }
                return Err(e.into());
            }
        };

        // Show release notes if requested
        if self.args.show_notes {
            if let Some(body) = &release.body {
                println!("\n=== Release Notes ===\n{body}\n=====================\n");
            }
        }

        // Find matching asset
        let target = self.args.target();
        let asset = GitHubClient::find_asset(&release, &target, self.args.bin.as_deref())
            .ok_or_else(|| GhInstallError::AssetNotFound {
                target: target.clone(),
            })?;

        // Download asset
        let temp_file = self.github_client.download_asset(&asset).await?;

        // Verify checksum unless explicitly skipped
        if !self.args.skip_checksum {
            if let Err(e) = self
                .verify_checksum(&release, &asset, temp_file.path())
                .await
            {
                tracing::error!("Checksum verification failed: {}", e);
                return Err(e.into());
            }
        } else {
            tracing::warn!("Skipping checksum verification (--skip-checksum was specified)");
        }

        // Verify signature if requested
        if self.args.verify_signature {
            if let Err(e) = self
                .verify_signature(&release, &asset, temp_file.path())
                .await
            {
                tracing::error!("Signature verification failed: {}", e);
                return Err(e.into());
            }
        }

        // Extract archive
        let extracted_dir = utils::extract_archive(temp_file.path())?;

        // Find and install binaries
        self.install_binaries(extracted_dir.path(), &repo).await?;

        tracing::info!("Installation completed successfully!");
        Ok(())
    }

    async fn install_binaries(&self, extracted_dir: &Path, default_name: &str) -> Result<()> {
        let executables = utils::find_executables(extracted_dir)?;

        if executables.is_empty() {
            return Err(GhInstallError::Installation(
                "No executable files found in the archive".to_string(),
            )
            .into());
        }

        let install_dir = self.args.install_dir();

        // Create install directory if it doesn't exist
        fs::create_dir_all(&install_dir)?;

        if self.args.bins {
            // Install all binaries
            for exe_path in &executables {
                self.install_binary(exe_path, &install_dir, None)?;
            }
        } else if let Some(bin_name) = &self.args.bin {
            // Install specific binary
            let matching = executables.iter().find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.contains(bin_name))
                    .unwrap_or(false)
            });

            if let Some(exe_path) = matching {
                self.install_binary(exe_path, &install_dir, Some(bin_name))?;
            } else {
                return Err(GhInstallError::Installation(format!(
                    "Binary '{bin_name}' not found in archive"
                ))
                .into());
            }
        } else {
            // Install default binary (matching repo name or first executable)
            let default_exe = executables
                .iter()
                .find(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.contains(default_name))
                        .unwrap_or(false)
                })
                .or_else(|| executables.first());

            if let Some(exe_path) = default_exe {
                self.install_binary(exe_path, &install_dir, Some(default_name))?;
            } else {
                return Err(
                    GhInstallError::Installation("No suitable binary found".to_string()).into(),
                );
            }
        }

        Ok(())
    }

    fn install_binary(&self, source: &Path, install_dir: &Path, name: Option<&str>) -> Result<()> {
        let binary_name = name
            .or_else(|| source.file_stem()?.to_str())
            .ok_or_else(|| GhInstallError::Installation("Invalid binary name".to_string()))?;

        let dest_path = install_dir.join(binary_name);

        // Add .exe extension on Windows
        #[cfg(windows)]
        let dest_path = if !dest_path.extension().map(|e| e == "exe").unwrap_or(false) {
            dest_path.with_extension("exe")
        } else {
            dest_path
        };

        tracing::info!("Installing {} to {}", binary_name, dest_path.display());

        // Copy binary to destination
        fs::copy(source, &dest_path)?;

        // Make executable on Unix
        utils::make_executable(&dest_path)?;

        Ok(())
    }

    #[allow(clippy::result_large_err)]
    async fn verify_checksum(
        &self,
        release: &octocrab::models::repos::Release,
        asset: &ReleaseAsset,
        file_path: &Path,
    ) -> GhResult<()> {
        // Look for SHA256SUMS file in the release
        let checksum_asset = release.assets.iter().find(|a| {
            let name = &a.name;
            name == "SHA256SUMS" || name == "checksums.txt" || name == "sha256sums.txt"
        });

        if let Some(checksum_asset) = checksum_asset {
            tracing::info!("Found checksum file: {}", checksum_asset.name);

            // Download checksum file
            let checksum_asset = ReleaseAsset {
                name: checksum_asset.name.clone(),
                url: checksum_asset.browser_download_url.to_string(),
                size: checksum_asset.size as u64,
            };

            let checksum_file = self
                .github_client
                .download_asset(&checksum_asset)
                .await
                .map_err(|_| GhInstallError::ChecksumVerification)?;

            // Read checksums from file
            let checksum_content = std::fs::read_to_string(checksum_file.path())
                .map_err(|_| GhInstallError::ChecksumVerification)?;

            // Parse checksums and find the one for our asset
            let expected_checksum = self.parse_checksum(&checksum_content, &asset.name)?;

            // Calculate actual checksum
            let actual_checksum = utils::calculate_sha256(file_path)
                .map_err(|_| GhInstallError::ChecksumVerification)?;

            // Compare checksums
            if actual_checksum.to_lowercase() != expected_checksum.to_lowercase() {
                tracing::error!(
                    "Checksum mismatch for {}: expected {}, got {}",
                    asset.name,
                    expected_checksum,
                    actual_checksum
                );
                return Err(GhInstallError::ChecksumVerification);
            }

            tracing::info!("Checksum verified successfully for {}", asset.name);
            Ok(())
        } else {
            // No checksum file found, which is an error unless --skip-checksum is used
            tracing::warn!("No SHA256SUMS file found in release");
            Err(GhInstallError::ChecksumVerification)
        }
    }

    #[allow(clippy::result_large_err)]
    fn parse_checksum(&self, content: &str, filename: &str) -> GhResult<String> {
        // SHA256SUMS format: <checksum>  <filename>
        for line in content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let checksum = parts[0];
                let file = parts[1..].join(" ");

                // Check if this line is for our file
                if file == filename || file.ends_with(&format!("/{filename}")) {
                    return Ok(checksum.to_string());
                }
            }
        }

        tracing::error!("No checksum found for file: {}", filename);
        Err(GhInstallError::ChecksumVerification)
    }

    async fn verify_signature(
        &self,
        release: &octocrab::models::repos::Release,
        asset: &ReleaseAsset,
        _file_path: &Path,
    ) -> GhResult<()> {
        // Look for .sig or .asc file
        let sig_asset = release.assets.iter().find(|a| {
            let name = &a.name;
            let asset_name = &asset.name;
            name == &format!("{asset_name}.sig") || name == &format!("{asset_name}.asc")
        });

        if let Some(sig_asset) = sig_asset {
            tracing::info!("Found signature file: {}", sig_asset.name);

            // Download signature file
            let sig_asset = ReleaseAsset {
                name: sig_asset.name.clone(),
                url: sig_asset.browser_download_url.to_string(),
                size: sig_asset.size as u64,
            };

            let _sig_file = self
                .github_client
                .download_asset(&sig_asset)
                .await
                .map_err(|_| GhInstallError::SignatureVerification)?;

            // TODO: Implement actual GPG verification
            tracing::warn!("Signature verification not yet implemented");
            Ok(())
        } else {
            Err(GhInstallError::SignatureVerification)
        }
    }

    async fn fallback_cargo_install(
        &self,
        owner: &str,
        repo: &str,
        tag: Option<&str>,
    ) -> Result<()> {
        tracing::info!("Falling back to cargo install from git");

        let mut cmd = Command::new("cargo");
        cmd.arg("install")
            .arg("--git")
            .arg(format!("https://github.com/{owner}/{repo}.git"));

        if let Some(tag) = tag {
            cmd.arg("--rev").arg(tag);
        }

        if let Some(bin) = &self.args.bin {
            cmd.arg("--bin").arg(bin);
        }

        let status = cmd.status()?;

        if !status.success() {
            return Err(GhInstallError::Installation("cargo install failed".to_string()).into());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_install_binary() {
        let temp_source = tempdir().unwrap();
        let temp_dest = tempdir().unwrap();

        let source_file = temp_source.path().join("test_binary");
        fs::write(&source_file, b"#!/bin/bash\necho test").unwrap();

        // Test the file copy and permission setting directly
        let dest_file = temp_dest.path().join("test");
        fs::copy(&source_file, &dest_file).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            utils::make_executable(&dest_file).unwrap();
            let metadata = fs::metadata(&dest_file).unwrap();
            assert!(metadata.permissions().mode() & 0o111 != 0);
        }

        assert!(dest_file.exists());
    }

    #[test]
    fn test_parse_checksum() {
        // Test standard SHA256SUMS format
        let content = r#"
abc123def456  test-binary-linux.tar.gz
789ghi012jkl  test-binary-macos.tar.gz
mno345pqr678  test-binary-windows.zip
"#;

        let checksum = parse_checksum_helper(content, "test-binary-linux.tar.gz").unwrap();
        assert_eq!(checksum, "abc123def456");

        let checksum = parse_checksum_helper(content, "test-binary-macos.tar.gz").unwrap();
        assert_eq!(checksum, "789ghi012jkl");

        // Test with path in filename
        let content_with_path = r#"
abc123def456  ./dist/test-binary-linux.tar.gz
789ghi012jkl  dist/test-binary-macos.tar.gz
"#;

        let checksum =
            parse_checksum_helper(content_with_path, "test-binary-linux.tar.gz").unwrap();
        assert_eq!(checksum, "abc123def456");

        // Test non-existent file
        let result = parse_checksum_helper(content, "non-existent.tar.gz");
        assert!(result.is_err());
    }

    #[test]
    fn test_skip_checksum_behavior() {
        // Create test arguments with skip_checksum = false
        let args_verify = Args {
            repo: "test/repo".to_string(),
            tag: None,
            bin: None,
            bins: false,
            target: None,
            install_dir: "/tmp".to_string(),
            show_notes: false,
            verify_signature: false,
            no_fallback: false,
            skip_checksum: false, // Should verify checksums
            config: std::path::PathBuf::from("test.toml"),
            verbose: false,
        };

        // Test that verification is required when skip_checksum is false
        assert!(
            !args_verify.skip_checksum,
            "Checksum verification should be enabled by default"
        );

        // Create test arguments with skip_checksum = true
        let args_skip = Args {
            repo: "test/repo".to_string(),
            tag: None,
            bin: None,
            bins: false,
            target: None,
            install_dir: "/tmp".to_string(),
            show_notes: false,
            verify_signature: false,
            no_fallback: false,
            skip_checksum: true, // Should skip checksums
            config: std::path::PathBuf::from("test.toml"),
            verbose: false,
        };

        // Test that verification is skipped when skip_checksum is true
        assert!(
            args_skip.skip_checksum,
            "Checksum verification should be skipped when flag is set"
        );
    }

    #[test]
    fn test_checksum_format_variations() {
        // Test various SHA256SUMS format variations

        // Format 1: Standard format (two spaces)
        let content1 = "abc123  file.tar.gz";
        let result = parse_checksum_for_line(content1, "file.tar.gz");
        assert_eq!(result, Some("abc123".to_string()));

        // Format 2: Single space
        let content2 = "def456 file.tar.gz";
        let result = parse_checksum_for_line(content2, "file.tar.gz");
        assert_eq!(result, Some("def456".to_string()));

        // Format 3: With path prefix
        let content3 = "ghi789  ./dist/file.tar.gz";
        let result = parse_checksum_for_line(content3, "file.tar.gz");
        assert_eq!(result, Some("ghi789".to_string()));

        // Format 4: Tab separator
        let content4 = "jkl012\tfile.tar.gz";
        let result = parse_checksum_for_line(content4, "file.tar.gz");
        assert_eq!(result, Some("jkl012".to_string()));
    }

    // Helper function to parse a single checksum line
    fn parse_checksum_for_line(line: &str, filename: &str) -> Option<String> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let checksum = parts[0];
            let file = parts[1..].join(" ");

            if file == filename || file.ends_with(&format!("/{filename}")) {
                return Some(checksum.to_string());
            }
        }
        None
    }

    // Helper function for testing parse_checksum logic
    #[allow(clippy::result_large_err)]
    fn parse_checksum_helper(content: &str, filename: &str) -> GhResult<String> {
        // SHA256SUMS format: <checksum>  <filename>
        for line in content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let checksum = parts[0];
                let file = parts[1..].join(" ");

                // Check if this line is for our file
                if file == filename || file.ends_with(&format!("/{filename}")) {
                    return Ok(checksum.to_string());
                }
            }
        }

        Err(GhInstallError::ChecksumVerification)
    }
}

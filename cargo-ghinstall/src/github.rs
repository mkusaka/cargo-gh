use crate::error::{GhInstallError, Result as GhResult};
use crate::retry::{with_retry, RetryConfig};
use anyhow::Result;
use octocrab::{models::repos::Release, Octocrab};
use reqwest::Client;

pub struct GitHubClient {
    octocrab: Octocrab,
    http_client: Client,
    retry_config: RetryConfig,
}

impl GitHubClient {
    pub fn new() -> Result<Self> {
        let octocrab = if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            Octocrab::builder().personal_token(token).build()?
        } else {
            Octocrab::builder().build()?
        };

        let http_client = Client::builder()
            .user_agent("cargo-ghinstall")
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self {
            octocrab,
            http_client,
            retry_config: RetryConfig::default(),
        })
    }

    /// Create a new client with custom retry configuration
    pub fn with_retry_config(retry_config: RetryConfig) -> Result<Self> {
        let mut client = Self::new()?;
        client.retry_config = retry_config;
        Ok(client)
    }

    /// Fetch release by tag or get latest release
    pub async fn get_release(
        &self,
        owner: &str,
        repo: &str,
        tag: Option<&str>,
    ) -> GhResult<Release> {
        let owner_clone = owner.to_string();
        let repo_clone = repo.to_string();
        let tag_clone = tag.map(|t| t.to_string());
        let octocrab = self.octocrab.clone();

        let operation_name = if tag.is_some() {
            format!("Fetching release '{}' for {}/{}", tag.unwrap(), owner, repo)
        } else {
            format!("Fetching latest release for {}/{}", owner, repo)
        };

        with_retry(&operation_name, &self.retry_config, || {
            let octocrab = octocrab.clone();
            let owner = owner_clone.clone();
            let repo = repo_clone.clone();
            let tag = tag_clone.clone();

            async move {
                if let Some(tag) = tag {
                    // Fetch specific release by tag
                    octocrab
                        .repos(&owner, &repo)
                        .releases()
                        .get_by_tag(&tag)
                        .await
                        .map_err(|e| anyhow::anyhow!("Failed to fetch release: {}", e))
                } else {
                    // Fetch latest release
                    octocrab
                        .repos(&owner, &repo)
                        .releases()
                        .get_latest()
                        .await
                        .map_err(|e| anyhow::anyhow!("Failed to fetch latest release: {}", e))
                }
            }
        })
        .await
        .map_err(|e| {
            tracing::error!("{}: {}", operation_name, e);
            GhInstallError::ReleaseNotFound {
                tag: tag.map(|t| t.to_string()).unwrap_or_else(|| "latest".to_string()),
                owner: owner.to_string(),
                repo: repo.to_string(),
            }
        })
    }

    /// Find matching asset for the target platform
    pub fn find_asset(
        release: &Release,
        target: &str,
        bin_name: Option<&str>,
    ) -> Option<ReleaseAsset> {
        let bin_name = bin_name.unwrap_or("");

        for asset in &release.assets {
            let name = &asset.name;

            // Check if asset matches target platform
            if !name.contains(target) {
                continue;
            }

            // Check if it's a compressed archive
            if !is_archive(name) {
                continue;
            }

            // If bin_name is specified, check if it matches
            if !bin_name.is_empty() && !name.contains(bin_name) {
                continue;
            }

            return Some(ReleaseAsset {
                name: name.clone(),
                url: asset.browser_download_url.to_string(),
                size: asset.size as u64,
            });
        }

        None
    }

    /// Download asset to a temporary file
    pub async fn download_asset(&self, asset: &ReleaseAsset) -> Result<tempfile::NamedTempFile> {
        tracing::info!("Downloading asset: {}", asset.name);

        let operation_name = format!("Downloading {}", asset.name);
        let url_clone = asset.url.clone();
        let name_clone = asset.name.clone();
        let http_client = self.http_client.clone();

        // Determine file extension for temp file
        let extension = if asset.name.ends_with(".tar.gz") {
            ".tar.gz"
        } else if asset.name.ends_with(".tgz") {
            ".tgz"
        } else if asset.name.ends_with(".zip") {
            ".zip"
        } else if asset.name.ends_with(".tar.xz") {
            ".tar.xz"
        } else if asset.name.ends_with(".tar.bz2") {
            ".tar.bz2"
        } else {
            ""
        };

        with_retry(&operation_name, &self.retry_config, || {
            let http_client = http_client.clone();
            let url = url_clone.clone();
            let _name = name_clone.clone();
            let ext = extension;

            async move {
                let response = http_client
                    .get(&url)
                    .send()
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to send download request: {}", e))?;

                if !response.status().is_success() {
                    let status = response.status();
                    let error_text = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Unable to read error response".to_string());
                    
                    // Return as non-retryable error for client errors (4xx)
                    if status.is_client_error() {
                        return Err(anyhow::anyhow!(
                            "Download failed with status {}: {}",
                            status,
                            error_text
                        ));
                    }
                    
                    // Return as retryable error for server errors (5xx)
                    return Err(anyhow::anyhow!(
                        "Download failed with status {}: {} (retrying...)",
                        status,
                        error_text
                    ));
                }

                // Create temp file and stream content
                let mut temp_file = tempfile::Builder::new()
                    .suffix(ext)
                    .tempfile()
                    .map_err(|e| anyhow::anyhow!("Failed to create temp file: {}", e))?;
                
                let mut stream = response.bytes_stream();

                use futures_util::StreamExt;
                use std::io::Write;

                while let Some(chunk) = stream.next().await {
                    let chunk = chunk.map_err(|e| anyhow::anyhow!("Failed to read chunk: {}", e))?;
                    temp_file
                        .write_all(&chunk)
                        .map_err(|e| anyhow::anyhow!("Failed to write to temp file: {}", e))?;
                }

                Ok(temp_file)
            }
        })
        .await
        .map_err(|e| {
            crate::error::GhInstallError::DownloadFailed {
                asset: asset.name.clone(),
                url: asset.url.clone(),
                status: 0, // Status unknown after retries
                message: e.to_string(),
            }
            .into()
        })
    }
}

#[derive(Debug, Clone)]
pub struct ReleaseAsset {
    pub name: String,
    pub url: String,
    #[allow(dead_code)]
    pub size: u64,
}

/// Check if a filename is a supported archive format
fn is_archive(name: &str) -> bool {
    name.ends_with(".tar.gz")
        || name.ends_with(".tgz")
        || name.ends_with(".zip")
        || name.ends_with(".tar.xz")
        || name.ends_with(".tar.bz2")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_archive() {
        assert!(is_archive("binary.tar.gz"));
        assert!(is_archive("binary.tgz"));
        assert!(is_archive("binary.zip"));
        assert!(is_archive("binary.tar.xz"));
        assert!(is_archive("binary.tar.bz2"));
        assert!(!is_archive("binary.exe"));
        assert!(!is_archive("binary"));
        assert!(!is_archive("README.md"));
    }
}

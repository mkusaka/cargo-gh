use crate::error::{GhDistError, Result as GhResult};
use anyhow::Result;
use octocrab::{models::repos::Release, Octocrab};
use reqwest::Client;
use std::path::Path;

pub struct GitHubClient {
    octocrab: Octocrab,
    http_client: Client,
}

impl GitHubClient {
    pub fn new(token: Option<String>) -> Result<Self> {
        let octocrab = if let Some(token) = token {
            Octocrab::builder().personal_token(token).build()?
        } else if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            Octocrab::builder().personal_token(token).build()?
        } else {
            Octocrab::builder().build()?
        };

        let http_client = Client::builder()
            .user_agent("cargo-ghdist")
            .timeout(std::time::Duration::from_secs(300))
            .build()?;

        Ok(Self {
            octocrab,
            http_client,
        })
    }

    /// Create a new release or update existing one
    pub async fn create_release(
        &self,
        owner: &str,
        repo: &str,
        tag: &str,
        draft: bool,
        target_commitish: Option<&str>,
    ) -> GhResult<Release> {
        // Check if release already exists
        match self
            .octocrab
            .repos(owner, repo)
            .releases()
            .get_by_tag(tag)
            .await
        {
            Ok(release) => {
                tracing::info!("Release {} already exists, will update it", tag);
                Ok(release)
            }
            Err(_) => {
                // Create new release
                tracing::info!("Creating new release: {}", tag);

                let mut release_builder = self
                    .octocrab
                    .repos(owner, repo)
                    .releases()
                    .create(tag)
                    .draft(draft)
                    .name(tag);
                
                // Set target commitish if provided
                if let Some(target) = target_commitish {
                    release_builder = release_builder.target_commitish(target);
                }

                match release_builder.send().await {
                    Ok(release) => Ok(release),
                    Err(e) => Err(GhDistError::ReleaseCreation(e.to_string())),
                }
            }
        }
    }

    /// Upload an asset to a release
    pub async fn upload_asset(
        &self,
        owner: &str,
        repo: &str,
        release_id: u64,
        asset_path: &Path,
        content_type: &str,
    ) -> GhResult<()> {
        let asset_name = asset_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| GhDistError::AssetUpload("Invalid asset path".to_string()))?;

        tracing::info!("Uploading asset: {}", asset_name);

        // Read file content
        let file_content = tokio::fs::read(asset_path).await?;

        // Upload using GitHub API
        let url = format!(
            "https://uploads.github.com/repos/{owner}/{repo}/releases/{release_id}/assets?name={asset_name}"
        );

        let response = self
            .http_client
            .post(&url)
            .header("Content-Type", content_type)
            .header(
                "Authorization",
                format!(
                    "Bearer {}",
                    self.get_token()
                        .map_err(|_| GhDistError::AssetUpload("No GitHub token".to_string()))?
                ),
            )
            .body(file_content)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(GhDistError::AssetUpload(format!(
                "Failed to upload asset: {status} - {error_text}"
            )));
        }

        tracing::info!("Successfully uploaded: {}", asset_name);
        Ok(())
    }

    /// Delete an existing asset from a release
    pub async fn delete_asset(&self, owner: &str, repo: &str, asset_id: u64) -> Result<()> {
        let url = format!("https://api.github.com/repos/{owner}/{repo}/releases/assets/{asset_id}");

        let response = self
            .http_client
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.get_token()?))
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to delete asset: {}", response.status());
        }

        Ok(())
    }

    /// Get the GitHub token from the client
    fn get_token(&self) -> Result<String> {
        std::env::var("GITHUB_TOKEN").map_err(|_| {
            anyhow::anyhow!("GitHub token not found. Set GITHUB_TOKEN environment variable")
        })
    }

    /// Check if an asset already exists in a release
    pub async fn asset_exists(
        &self,
        owner: &str,
        repo: &str,
        release_id: u64,
        asset_name: &str,
    ) -> Result<Option<u64>> {
        // Get all releases and find the one with matching ID
        let releases = self
            .octocrab
            .repos(owner, repo)
            .releases()
            .list()
            .send()
            .await?;

        for release in releases {
            if release.id.0 == release_id {
                for asset in &release.assets {
                    if asset.name == asset_name {
                        return Ok(Some(asset.id.0));
                    }
                }
                break;
            }
        }

        Ok(None)
    }
}

/// Determine content type for an asset
pub fn get_content_type(path: &Path) -> &'static str {
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    match extension {
        "gz" | "tgz" => "application/gzip",
        "zip" => "application/zip",
        "xz" => "application/x-xz",
        "bz2" => "application/x-bzip2",
        "txt" => "text/plain",
        _ => "application/octet-stream",
    }
}

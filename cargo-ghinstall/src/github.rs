use crate::error::{GhInstallError, Result as GhResult};
use anyhow::Result;
use octocrab::{models::repos::Release, Octocrab};
use reqwest::Client;

pub struct GitHubClient {
    octocrab: Octocrab,
    http_client: Client,
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
        })
    }

    /// Fetch release by tag or get latest release
    pub async fn get_release(
        &self,
        owner: &str,
        repo: &str,
        tag: Option<&str>,
    ) -> GhResult<Release> {
        if let Some(tag) = tag {
            // Fetch specific release by tag
            match self
                .octocrab
                .repos(owner, repo)
                .releases()
                .get_by_tag(tag)
                .await
            {
                Ok(release) => Ok(release),
                Err(_) => Err(GhInstallError::ReleaseNotFound {
                    tag: tag.to_string(),
                }),
            }
        } else {
            // Fetch latest release
            match self
                .octocrab
                .repos(owner, repo)
                .releases()
                .get_latest()
                .await
            {
                Ok(release) => Ok(release),
                Err(_) => Err(GhInstallError::ReleaseNotFound {
                    tag: "latest".to_string(),
                }),
            }
        }
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

        let response = self.http_client.get(&asset.url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to download asset: {}", response.status());
        }

        let mut temp_file = tempfile::NamedTempFile::new()?;
        let mut stream = response.bytes_stream();

        use futures_util::StreamExt;
        use std::io::Write;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            temp_file.write_all(&chunk)?;
        }

        Ok(temp_file)
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

use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum GhDistError {
    #[error("GitHub API error: {0}")]
    GitHubApi(Box<octocrab::Error>),

    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Git error: {0}")]
    Git(#[from] git2::Error),

    #[error("Build failed for target: {target}")]
    BuildFailed { target: String },

    #[error("No tag found on HEAD")]
    NoTag,

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Package creation failed: {0}")]
    Package(String),

    #[error("Release creation failed: {0}")]
    ReleaseCreation(String),

    #[error("Asset upload failed: {0}")]
    AssetUpload(String),

    #[error("Invalid repository format: {0}")]
    InvalidRepo(String),
}

pub type Result<T> = std::result::Result<T, GhDistError>;

impl From<octocrab::Error> for GhDistError {
    fn from(err: octocrab::Error) -> Self {
        GhDistError::GitHubApi(Box::new(err))
    }
}

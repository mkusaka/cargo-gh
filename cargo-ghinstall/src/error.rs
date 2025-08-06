use thiserror::Error;

#[derive(Error, Debug)]
pub enum GhInstallError {
    #[error("GitHub API error: {0}")]
    GitHubApi(#[from] octocrab::Error),

    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Release not found: {tag}")]
    ReleaseNotFound { tag: String },

    #[error("No compatible asset found for target: {target}")]
    AssetNotFound { target: String },

    #[error("Failed to parse version: {0}")]
    VersionParse(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Installation failed: {0}")]
    Installation(String),

    #[error("Signature verification failed")]
    SignatureVerification,

    #[error("Invalid repository format: {0}")]
    InvalidRepo(String),

    #[error("Archive extraction failed: {0}")]
    ArchiveExtraction(String),
}

pub type Result<T> = std::result::Result<T, GhInstallError>;
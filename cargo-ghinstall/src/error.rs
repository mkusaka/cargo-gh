use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum GhInstallError {
    #[error("GitHub API error: {0}")]
    GitHubApi(#[from] octocrab::Error),

    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Release not found: {tag} in {owner}/{repo}")]
    ReleaseNotFound {
        tag: String,
        owner: String,
        repo: String,
    },

    #[error("No compatible asset found for target '{target}' in release {release_tag}. Available assets: {available}")]
    AssetNotFound {
        target: String,
        release_tag: String,
        available: String,
    },

    #[error("Failed to parse version '{input}': {reason}. Expected format: MAJOR.MINOR.PATCH (e.g., 1.2.3)")]
    VersionParse { input: String, reason: String },

    #[error("Configuration error at {path}: {message}")]
    Config { path: String, message: String },

    #[error("Installation failed: {message}. Path: {path}")]
    Installation { message: String, path: String },

    #[error("Signature verification failed for {file}. Signature file: {sig_file}")]
    SignatureVerification { file: String, sig_file: String },

    #[error("Checksum verification failed for {file}: expected {expected}, got {actual}")]
    ChecksumVerification {
        file: String,
        expected: String,
        actual: String,
    },

    #[error("Invalid repository format '{input}'. Expected format: owner/repo[@tag] (e.g., rust-lang/rust@v1.0.0)")]
    InvalidRepo { input: String },

    #[error("Archive extraction failed for {file}: {reason}. Supported formats: .tar.gz, .tgz, .zip, .tar.xz, .tar.bz2")]
    ArchiveExtraction { file: String, reason: String },

    #[error("No checksum file found in release. Expected one of: SHA256SUMS, checksums.txt, sha256sums.txt")]
    ChecksumFileNotFound,

    #[error("Failed to download {asset} from {url}: HTTP {status} - {message}")]
    DownloadFailed {
        asset: String,
        url: String,
        status: u16,
        message: String,
    },

    #[error("GitHub API rate limit exceeded. Limit: {limit}, Remaining: {remaining}, Reset at: {reset_at}")]
    RateLimitExceeded {
        limit: u32,
        remaining: u32,
        reset_at: String,
    },

    #[error("Binary '{name}' not found in archive. Available binaries: {available}")]
    BinaryNotFound { name: String, available: String },

    #[error("No executable files found in archive {archive}. Archive may be corrupted or contain source code only.")]
    NoExecutablesFound { archive: String },
}

pub type Result<T> = std::result::Result<T, GhInstallError>;

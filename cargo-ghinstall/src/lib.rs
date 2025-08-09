//! # cargo-ghinstall
//!
//! A cargo subcommand for installing binaries directly from GitHub releases.
//!
//! ## Overview
//!
//! `cargo-ghinstall` provides a convenient way to download and install pre-built
//! binaries from GitHub releases, avoiding the need to compile from source.
//! It automatically detects your platform, downloads the appropriate binary,
//! and installs it to your cargo bin directory.
//!
//! ## Features
//!
//! - Automatic platform detection (Linux, macOS, Windows)
//! - Architecture detection (x86_64, aarch64)
//! - Support for specific version tags and latest releases
//! - Multiple binary selection with `--bin` flag
//! - Configuration file support for custom settings
//! - Fallback to `cargo install` when binaries are unavailable
//!
//! ## Usage
//!
//! ```bash
//! # Install latest release
//! cargo ghinstall owner/repo
//!
//! # Install specific version
//! cargo ghinstall owner/repo@v1.0.0
//!
//! # Install specific binary from multi-binary release
//! cargo ghinstall owner/repo --bin specific-binary
//! ```
//!
//! ## Configuration
//!
//! Configuration can be specified in `~/.config/ghinstall.toml` or
//! `.config/ghinstall.toml` in your project directory.

/// Command-line interface definitions and argument parsing
pub mod cli;

/// Configuration file handling and repository-specific settings
pub mod config;

/// Error types and error handling utilities
pub mod error;

/// GitHub API client for interacting with releases and assets
pub mod github;

/// Core installation logic for downloading and installing binaries
pub mod installer;

/// Utility functions for platform detection, archive extraction, and file operations
pub mod utils;

/// Network retry logic with exponential backoff
pub mod retry;

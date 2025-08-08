//! # cargo-ghdist
//!
//! A cargo subcommand for creating and distributing releases to GitHub.
//!
//! ## Overview
//!
//! `cargo-ghdist` automates the process of building, packaging, and releasing
//! Rust binaries to GitHub. It handles multi-platform builds, generates checksums,
//! creates formatted release notes, and uploads all assets to GitHub releases.
//!
//! ## Features
//!
//! - Multi-platform builds (Linux, macOS, Windows)
//! - Automatic archive creation (tar.gz, zip)
//! - SHA256 checksum generation
//! - Beautiful formatted release notes
//! - Continuous releases with `--hash` option
//! - Integration with GitHub's auto-generated release notes
//! - Configuration file support
//! - CI/CD workflow generation
//!
//! ## Usage
//!
//! ```bash
//! # Create a tagged release
//! cargo ghdist --tag v1.0.0
//!
//! # Create a continuous release with commit hash
//! cargo ghdist --hash
//!
//! # Build for specific targets
//! cargo ghdist --targets x86_64-unknown-linux-gnu,aarch64-apple-darwin
//!
//! # Skip cargo publish step
//! cargo ghdist --skip-publish
//! ```
//!
//! ## Configuration
//!
//! Configuration can be specified in `.config/ghdist.toml` in your project
//! directory or `~/.config/ghdist.toml` for user-wide settings.
//!
//! ## Release Notes Format
//!
//! The tool generates formatted release notes that include:
//! - Commit information (SHA, author, branch)
//! - Installation instructions with examples
//! - Links to relevant commits and comparisons
//! - Auto-generated GitHub release notes for tagged releases

/// Core distribution builder that orchestrates the entire release process
pub mod builder;

/// Command-line interface definitions and argument parsing
pub mod cli;

/// Configuration file handling and default settings management
pub mod config;

/// Error types and error handling utilities
pub mod error;

/// GitHub API client for creating releases and uploading assets
pub mod github;

/// Archive creation and checksum generation utilities
pub mod packager;

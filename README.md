# cargo-gh

GitHub Releases integration for Cargo - Install and distribute binaries via GitHub Releases with support for any tag format including SemVer tags, commit hashes, and branch names.

## Overview

This repository provides two Cargo subcommands:

- **`cargo-ghinstall`**: Install prebuilt binaries from GitHub Releases
- **`cargo-ghdist`**: Build and distribute binaries to GitHub Releases

Both commands support any tag format including:
- Semantic version tags (e.g., `v1.2.3`, `1.0.0`)
- Commit hashes (e.g., `abcdef0`, `vabcdef0`)
- Branch names (e.g., `main`, `develop`)
- Any other git reference

This flexibility makes them suitable for various release workflows.

## Quick Start

```bash
# Install the tools
cargo install --git https://github.com/mkusaka/cargo-gh

# Install a binary from ANY tag/commit/branch
cargo ghinstall owner/repo@abcdef0          # From commit hash
cargo ghinstall owner/repo@main             # From branch
cargo ghinstall owner/repo@nightly-2024     # From custom tag

# Release binaries with ANY tag format
cargo ghdist --tag $(git rev-parse --short HEAD)  # Current commit
cargo ghdist --tag main                           # Branch name
cargo ghdist --tag nightly-$(date +%Y%m%d)       # Date-based tag
```

## Installation

```bash
# Install both commands
cargo install --git https://github.com/mkusaka/cargo-gh

# Or install individually
cargo install --git https://github.com/mkusaka/cargo-gh cargo-ghinstall
cargo install --git https://github.com/mkusaka/cargo-gh cargo-ghdist
```

## cargo-ghinstall

Install prebuilt binaries from GitHub Releases.

### Usage

```bash
# Install latest release
cargo ghinstall owner/repo

# Install specific version (SemVer - with or without 'v' prefix)
cargo ghinstall owner/repo@v1.2.3
cargo ghinstall owner/repo@1.2.3

# Install from commit hash (any format)
cargo ghinstall owner/repo@abcdef0
cargo ghinstall owner/repo@vabcdef0
cargo ghinstall owner/repo@abc123

# Install from branch name
cargo ghinstall owner/repo@main
cargo ghinstall owner/repo@develop
cargo ghinstall owner/repo@feature/new-ui

# Install from any git reference
cargo ghinstall owner/repo@nightly-2024-01-15
cargo ghinstall owner/repo@release-candidate

# Install with options
cargo ghinstall owner/repo \
  --tag v1.2.3 \
  --bin specific-binary \
  --target x86_64-apple-darwin \
  --install-dir ~/bin
```

### Options

| Option | Description | Default |
|--------|-------------|---------|
| `-t, --tag <TAG>` | Release tag (e.g., `v1.2.3`, `abcdef0`, `main`) | `latest` |
| `-b, --bin <NAME>` | Binary name or pattern to install | Repository name |
| `--bins` | Install all binaries from the repository | — |
| `-T, --target <TRIPLE>` | Platform target (e.g., `aarch64-apple-darwin`) | Host platform |
| `-d, --install-dir <DIR>` | Installation directory | `~/.cargo/bin` |
| `--show-notes` | Display release notes | Off |
| `--verify-signature` | Verify GPG signature if `.sig` asset exists | Off |
| `--no-fallback` | Disable fallback to `cargo install --git` | Off |
| `--config <FILE>` | Configuration file path | `~/.config/ghinstall.toml` |
| `--verbose` | Enable verbose output | Off |

### Configuration

Create `~/.config/ghinstall.toml`:

```toml
[default]
install-dir = "~/.cargo/bin"
timeout = 30  # HTTP timeout in seconds

[repo."owner/repo"]
bin = "specific-binary"
targets = ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"]
verify-signature = true
```

### Behavior

1. Attempts to download prebuilt binary from GitHub Releases
2. Searches for assets matching the target platform
3. Downloads and extracts the archive (supports `.tar.gz`, `.zip`, `.tar.xz`, `.tar.bz2`)
4. Installs binaries to the specified directory with executable permissions
5. Falls back to `cargo install --git` if no matching asset is found (unless `--no-fallback`)

## cargo-ghdist

Build and distribute binaries to GitHub Releases.

### Usage

```bash
# Build and release for default targets
cargo ghdist

# Release with specific version tag (with or without 'v' prefix)
cargo ghdist --tag v1.2.3
cargo ghdist --tag 1.2.3

# Release from commit hash (any format)
cargo ghdist --tag abcdef0
cargo ghdist --tag vabcdef0
cargo ghdist --tag abc123

# Release from branch name
cargo ghdist --tag main
cargo ghdist --tag develop
cargo ghdist --tag feature/new-ui

# Release with any custom tag
cargo ghdist --tag nightly-2024-01-15
cargo ghdist --tag release-candidate
cargo ghdist --tag beta-5

# Build for specific targets
cargo ghdist \
  --targets x86_64-unknown-linux-gnu,aarch64-unknown-linux-gnu \
  --format tgz \
  --draft
```

### Options

| Option | Description | Default |
|--------|-------------|---------|
| `-t, --tag <TAG>` | Release tag (e.g., `v1.2.3`, `abcdef0`, `main`) | Tag on HEAD |
| `-T, --targets <LIST>` | Build targets (comma-separated) | `x86_64-unknown-linux-gnu,`<br>`aarch64-unknown-linux-gnu` |
| `-f, --format <FMT>` | Archive format (`tgz` or `zip`) | `tgz` |
| `--draft` | Create as draft release | Off |
| `--skip-publish` | Skip `cargo publish` step | On |
| `--no-checksum` | Don't generate SHA256SUMS file | Off |
| `--repository <REPO>` | GitHub repository (owner/repo) | From `Cargo.toml` |
| `--github-token <TOKEN>` | GitHub token | `$GITHUB_TOKEN` |
| `--bins <LIST>` | Specific binaries to include | All binaries |
| `--profile <PROFILE>` | Build profile | `release` |
| `--config <FILE>` | Configuration file path | `~/.config/ghdist.toml` |
| `--verbose` | Enable verbose output | Off |

### Configuration

Create `~/.config/ghdist.toml`:

```toml
[default]
targets = ["x86_64-unknown-linux-gnu", "x86_64-apple-darwin"]
format = "tgz"
draft = false
skip-publish = true

[repository]
owner = "your-org"
repo = "your-crate"
```

### Behavior

1. Detects or uses specified tag
2. Builds binaries for each target platform
3. Creates archives in the specified format
4. Generates SHA256SUMS if not disabled
5. Creates or updates GitHub Release
6. Uploads all assets to the release
7. Optionally runs `cargo publish`

### GitHub Token

Set the `GITHUB_TOKEN` environment variable or use the `--github-token` option:

```bash
export GITHUB_TOKEN=ghp_xxxxxxxxxxxxxxxxxxxx
cargo ghdist
```

## Features

- **Universal Tag Support**: Works with ANY git reference format:
  - Semantic versions: `v1.2.3`, `1.0.0`, `2.0.0-beta.1`
  - Commit hashes: `abcdef0`, `vabcdef0`, `abc123` (any length)
  - Branch names: `main`, `develop`, `feature/new-ui`
  - Custom tags: `nightly-2024-01-15`, `release-candidate`, `beta-5`
  - Any other git reference your workflow requires
- **Multi-Platform**: Build and install for multiple target platforms
- **Archive Formats**: Supports `.tar.gz`, `.zip`, `.tar.xz`, `.tar.bz2`
- **Configuration Files**: Persistent settings via TOML configuration
- **Fallback Support**: Automatic fallback to source installation when binaries unavailable
- **Checksum Generation**: Automatic SHA256SUMS for release verification
- **Draft Releases**: Support for creating draft releases before publishing

## CI/CD Integration

### Release Workflows

This repository includes three types of GitHub Actions workflows for releases:

#### 1. Tagged Releases (`release.yml`)
Triggered by pushing version tags. Creates stable releases.

```yaml
# Triggers on:
# - v1.0.0 (SemVer with v prefix)
# - 1.0.0 (Direct version)
# - abc123 (Commit hash tags)
```

#### 2. Continuous Releases (`release-continuous.yml`)
Automatically creates pre-releases on every push to main/master.

```yaml
# Features:
# - Creates releases with format: dev-YYYYMMDD-HHMMSS-SHA
# - Marks as pre-release
# - Automatically cleans up old dev releases (keeps last 5)
# - Perfect for nightly/continuous deployment
```

#### 3. Manual Releases (`release-manual.yml`)
Trigger releases manually with custom parameters via GitHub UI.

```yaml
# Options:
# - Custom tag name
# - Draft/Pre-release flags
# - Platform selection
# - Triggered via Actions tab → Manual Release → Run workflow
```

### GitHub Actions Example

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'          # Semantic version tags
      - '[0-9]*'      # Plain version numbers
      - 'release-*'   # Release branches
      - 'nightly-*'   # Nightly builds
    branches:
      - main          # Continuous releases

jobs:
  release:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        
      - name: Install cargo-ghdist
        run: cargo install --git https://github.com/mkusaka/cargo-gh cargo-ghdist
        
      - name: Build and Release
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          # For continuous releases on push
          if [[ "${{ github.ref }}" == "refs/heads/"* ]]; then
            TAG="dev-$(date -u +%Y%m%d-%H%M%S)-$(git rev-parse --short HEAD)"
            DRAFT_FLAG="--draft"
          else
            TAG="${GITHUB_REF#refs/tags/}"
            DRAFT_FLAG=""
          fi
          
          cargo ghdist \
            --tag "$TAG" \
            --targets x86_64-unknown-linux-gnu,x86_64-apple-darwin,x86_64-pc-windows-msvc \
            --format tgz \
            $DRAFT_FLAG
```

## Comparison with Similar Tools

### vs cargo-binstall / cargo-dist
- **cargo-gh** supports ANY git reference format (commit hashes, branch names, custom tags)
- **cargo-binstall/cargo-dist** primarily focus on semantic version tags
- **cargo-gh** allows more flexible release workflows without being restricted to SemVer

### Key Advantages
- ✅ Release from any commit without creating a version tag
- ✅ Use branch names for continuous deployment (e.g., `main`, `develop`)
- ✅ Support custom tag formats for your workflow (e.g., `nightly-YYYY-MM-DD`)
- ✅ Compatible with existing GitHub Release workflows
- ✅ Fallback to source installation when binaries are unavailable

## Development

```bash
# Clone repository
git clone https://github.com/mkusaka/cargo-gh
cd cargo-gh

# Build
cargo build --workspace

# Run tests
cargo test --workspace

# Install locally
cargo install --path cargo-ghinstall
cargo install --path cargo-ghdist
```

## License

MIT

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
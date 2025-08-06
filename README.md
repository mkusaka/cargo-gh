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

# Install specific version (SemVer)
cargo ghinstall owner/repo@v1.2.3

# Install from commit hash (with or without 'v' prefix)
cargo ghinstall owner/repo@abcdef0
cargo ghinstall owner/repo@vabcdef0

# Install from branch name
cargo ghinstall owner/repo@main

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

# Release with specific tag
cargo ghdist --tag v1.2.3

# Release from commit hash (with or without 'v' prefix)
cargo ghdist --tag abcdef0
cargo ghdist --tag vabcdef0

# Release from branch name
cargo ghdist --tag main

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

- **Flexible Tag Support**: Works with any tag format - SemVer (`v1.2.3`), plain hashes (`abcdef0`), branch names (`main`), etc.
- **Multi-Platform**: Build and install for multiple target platforms
- **Archive Formats**: Supports `.tar.gz`, `.zip`, `.tar.xz`, `.tar.bz2`
- **Configuration Files**: Persistent settings via TOML configuration
- **Fallback Support**: Automatic fallback to source installation when binaries unavailable
- **Checksum Generation**: Automatic SHA256SUMS for release verification
- **Draft Releases**: Support for creating draft releases before publishing

## CI/CD Integration

### GitHub Actions Example

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

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
          cargo ghdist \
            --targets x86_64-unknown-linux-gnu,x86_64-apple-darwin,x86_64-pc-windows-msvc \
            --format tgz
```

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
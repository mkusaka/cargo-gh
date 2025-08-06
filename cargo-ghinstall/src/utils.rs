use crate::error::GhInstallError;
use anyhow::Result;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// Extract archive to a temporary directory
pub fn extract_archive(archive_path: &Path) -> Result<tempfile::TempDir> {
    let temp_dir = tempfile::tempdir()?;
    let archive_name = archive_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| GhInstallError::ArchiveExtraction("Invalid archive path".to_string()))?;

    if archive_name.ends_with(".tar.gz") || archive_name.ends_with(".tgz") {
        extract_tar_gz(archive_path, temp_dir.path())?;
    } else if archive_name.ends_with(".tar.xz") {
        extract_tar_xz(archive_path, temp_dir.path())?;
    } else if archive_name.ends_with(".tar.bz2") {
        extract_tar_bz2(archive_path, temp_dir.path())?;
    } else if archive_name.ends_with(".zip") {
        extract_zip(archive_path, temp_dir.path())?;
    } else {
        return Err(GhInstallError::ArchiveExtraction(format!(
            "Unsupported archive format: {archive_name}"
        ))
        .into());
    }

    Ok(temp_dir)
}

/// Extract tar.gz archive
fn extract_tar_gz(archive_path: &Path, dest_dir: &Path) -> Result<()> {
    let file = fs::File::open(archive_path)?;
    let gz_decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz_decoder);
    archive.unpack(dest_dir)?;
    Ok(())
}

/// Extract tar.xz archive
fn extract_tar_xz(archive_path: &Path, dest_dir: &Path) -> Result<()> {
    let file = fs::File::open(archive_path)?;
    let xz_decoder = xz2::read::XzDecoder::new(file);
    let mut archive = tar::Archive::new(xz_decoder);
    archive.unpack(dest_dir)?;
    Ok(())
}

/// Extract tar.bz2 archive
fn extract_tar_bz2(archive_path: &Path, dest_dir: &Path) -> Result<()> {
    let file = fs::File::open(archive_path)?;
    let bz2_decoder = bzip2::read::BzDecoder::new(file);
    let mut archive = tar::Archive::new(bz2_decoder);
    archive.unpack(dest_dir)?;
    Ok(())
}

/// Extract zip archive
fn extract_zip(archive_path: &Path, dest_dir: &Path) -> Result<()> {
    let file = fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = dest_dir.join(file.mangled_name());

        if file.is_dir() {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut outfile = fs::File::create(&outpath)?;
            io::copy(&mut file, &mut outfile)?;
        }

        // Set executable permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = file.unix_mode() {
                fs::set_permissions(&outpath, fs::Permissions::from_mode(mode))?;
            }
        }
    }

    Ok(())
}

/// Find executable files in a directory
pub fn find_executables(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut executables = Vec::new();

    for entry in walkdir::WalkDir::new(dir) {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && is_executable(path)? {
            executables.push(path.to_path_buf());
        }
    }

    Ok(executables)
}

/// Check if a file is executable
#[cfg(unix)]
fn is_executable(path: &Path) -> Result<bool> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = fs::metadata(path)?;
    let permissions = metadata.permissions();
    Ok(permissions.mode() & 0o111 != 0)
}

#[cfg(windows)]
fn is_executable(path: &Path) -> Result<bool> {
    // On Windows, check for common executable extensions
    Ok(path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| matches!(ext.to_lowercase().as_str(), "exe" | "bat" | "cmd" | "ps1"))
        .unwrap_or(false))
}

/// Make a file executable (Unix only)
#[cfg(unix)]
pub fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(permissions.mode() | 0o111);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(windows)]
pub fn make_executable(_path: &Path) -> Result<()> {
    // No-op on Windows
    Ok(())
}

/// Calculate SHA256 hash of a file
#[allow(dead_code)]
pub fn calculate_sha256(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};

    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 8192];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_is_executable_detection() {
        let dir = tempdir().unwrap();

        #[cfg(unix)]
        {
            let exec_file = dir.path().join("executable");
            fs::write(&exec_file, "#!/bin/bash\necho test").unwrap();
            make_executable(&exec_file).unwrap();
            assert!(is_executable(&exec_file).unwrap());

            let non_exec_file = dir.path().join("non_executable");
            fs::write(&non_exec_file, "test").unwrap();
            assert!(!is_executable(&non_exec_file).unwrap());
        }

        #[cfg(windows)]
        {
            let exec_file = dir.path().join("executable.exe");
            fs::write(&exec_file, "test").unwrap();
            assert!(is_executable(&exec_file).unwrap());

            let non_exec_file = dir.path().join("non_executable.txt");
            fs::write(&non_exec_file, "test").unwrap();
            assert!(!is_executable(&non_exec_file).unwrap());
        }
    }

    #[test]
    fn test_calculate_sha256() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, b"Hello, World!").unwrap();

        let hash = calculate_sha256(&file_path).unwrap();
        assert_eq!(
            hash,
            "dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f"
        );
    }
}

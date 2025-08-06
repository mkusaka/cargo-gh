use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use anyhow::Result;
use crate::cli::ArchiveFormat;
use crate::error::GhDistError;

/// Package binaries into an archive
pub fn create_archive(
    binaries: &[PathBuf],
    output_dir: &Path,
    archive_name: &str,
    format: ArchiveFormat,
) -> Result<PathBuf> {
    let archive_path = match format {
        ArchiveFormat::Tgz => {
            let path = output_dir.join(format!("{}.tar.gz", archive_name));
            create_tar_gz(&path, binaries)?;
            path
        }
        ArchiveFormat::Zip => {
            let path = output_dir.join(format!("{}.zip", archive_name));
            create_zip(&path, binaries)?;
            path
        }
    };

    tracing::info!("Created archive: {}", archive_path.display());
    Ok(archive_path)
}

/// Create a tar.gz archive
fn create_tar_gz(archive_path: &Path, files: &[PathBuf]) -> Result<()> {
    let tar_file = File::create(archive_path)?;
    let gz_encoder = flate2::write::GzEncoder::new(tar_file, flate2::Compression::default());
    let mut tar_builder = tar::Builder::new(gz_encoder);

    for file_path in files {
        let file_name = file_path.file_name()
            .ok_or_else(|| GhDistError::Package("Invalid file path".to_string()))?;
        
        let mut file = File::open(file_path)?;
        tar_builder.append_file(file_name, &mut file)?;
    }

    tar_builder.finish()?;
    Ok(())
}

/// Create a zip archive
fn create_zip(archive_path: &Path, files: &[PathBuf]) -> Result<()> {
    let file = File::create(archive_path)?;
    let mut zip = zip::ZipWriter::new(file);

    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755);

    for file_path in files {
        let file_name = file_path.file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| GhDistError::Package("Invalid file path".to_string()))?;

        zip.start_file(file_name, options)?;
        
        let file_content = fs::read(file_path)?;
        zip.write_all(&file_content)?;
    }

    zip.finish()?;
    Ok(())
}

/// Generate SHA256 checksums for files
pub fn generate_checksums(files: &[PathBuf], output_dir: &Path) -> Result<PathBuf> {
    use sha2::{Sha256, Digest};
    
    let checksum_path = output_dir.join("SHA256SUMS");
    let mut checksum_file = File::create(&checksum_path)?;

    for file_path in files {
        let file_name = file_path.file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| GhDistError::Package("Invalid file path".to_string()))?;

        let mut file = File::open(file_path)?;
        let mut hasher = Sha256::new();
        io::copy(&mut file, &mut hasher)?;
        let hash = hasher.finalize();
        let hash_hex = hex::encode(hash);

        writeln!(checksum_file, "{}  {}", hash_hex, file_name)?;
    }

    tracing::info!("Generated checksums: {}", checksum_path.display());
    Ok(checksum_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_create_tar_gz_archive() {
        let temp_dir = tempdir().unwrap();
        let output_dir = tempdir().unwrap();
        
        // Create test files
        let file1 = temp_dir.path().join("binary1");
        let file2 = temp_dir.path().join("binary2");
        fs::write(&file1, b"content1").unwrap();
        fs::write(&file2, b"content2").unwrap();
        
        let files = vec![file1, file2];
        let archive_path = create_archive(
            &files,
            output_dir.path(),
            "test",
            ArchiveFormat::Tgz,
        ).unwrap();
        
        assert!(archive_path.exists());
        assert!(archive_path.to_str().unwrap().ends_with(".tar.gz"));
    }

    #[test]
    fn test_create_zip_archive() {
        let temp_dir = tempdir().unwrap();
        let output_dir = tempdir().unwrap();
        
        // Create test files
        let file1 = temp_dir.path().join("binary1");
        let file2 = temp_dir.path().join("binary2");
        fs::write(&file1, b"content1").unwrap();
        fs::write(&file2, b"content2").unwrap();
        
        let files = vec![file1, file2];
        let archive_path = create_archive(
            &files,
            output_dir.path(),
            "test",
            ArchiveFormat::Zip,
        ).unwrap();
        
        assert!(archive_path.exists());
        assert!(archive_path.to_str().unwrap().ends_with(".zip"));
    }

    #[test]
    fn test_generate_checksums() {
        let temp_dir = tempdir().unwrap();
        
        // Create test files
        let file1 = temp_dir.path().join("binary1");
        let file2 = temp_dir.path().join("binary2");
        fs::write(&file1, b"content1").unwrap();
        fs::write(&file2, b"content2").unwrap();
        
        let files = vec![file1, file2];
        let checksum_path = generate_checksums(&files, temp_dir.path()).unwrap();
        
        assert!(checksum_path.exists());
        
        let content = fs::read_to_string(&checksum_path).unwrap();
        assert!(content.contains("binary1"));
        assert!(content.contains("binary2"));
    }
}
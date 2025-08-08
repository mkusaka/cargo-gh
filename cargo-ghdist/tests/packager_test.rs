use cargo_ghdist::cli::ArchiveFormat;
use cargo_ghdist::packager::{create_archive, generate_checksums};
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn test_create_tar_gz_archive() {
    let temp_dir = tempdir().unwrap();
    let output_dir = tempdir().unwrap();

    // Create test binaries
    let binary1 = temp_dir.path().join("test-binary1");
    let binary2 = temp_dir.path().join("test-binary2");
    fs::write(&binary1, b"binary content 1").unwrap();
    fs::write(&binary2, b"binary content 2").unwrap();

    let binaries = vec![binary1, binary2];

    let archive_path = create_archive(
        &binaries,
        output_dir.path(),
        "test-archive",
        ArchiveFormat::Tgz,
    )
    .unwrap();

    assert!(archive_path.exists());
    assert!(archive_path.to_string_lossy().ends_with(".tar.gz"));

    // Verify the archive is not empty
    let metadata = fs::metadata(&archive_path).unwrap();
    assert!(metadata.len() > 0);
}

#[test]
fn test_create_zip_archive() {
    let temp_dir = tempdir().unwrap();
    let output_dir = tempdir().unwrap();

    // Create test binaries
    let binary1 = temp_dir.path().join("test-binary1");
    let binary2 = temp_dir.path().join("test-binary2");
    fs::write(&binary1, b"binary content 1").unwrap();
    fs::write(&binary2, b"binary content 2").unwrap();

    let binaries = vec![binary1, binary2];

    let archive_path = create_archive(
        &binaries,
        output_dir.path(),
        "test-archive",
        ArchiveFormat::Zip,
    )
    .unwrap();

    assert!(archive_path.exists());
    assert!(archive_path.to_string_lossy().ends_with(".zip"));

    // Verify the archive is not empty
    let metadata = fs::metadata(&archive_path).unwrap();
    assert!(metadata.len() > 0);
}

#[test]
fn test_generate_checksums() {
    let temp_dir = tempdir().unwrap();

    // Create test files
    let file1 = temp_dir.path().join("file1.tar.gz");
    let file2 = temp_dir.path().join("file2.tar.gz");
    fs::write(&file1, b"test content 1").unwrap();
    fs::write(&file2, b"test content 2").unwrap();

    let files = vec![file1, file2];

    let checksum_path = generate_checksums(&files, temp_dir.path()).unwrap();

    assert!(checksum_path.exists());
    assert_eq!(checksum_path.file_name().unwrap(), "SHA256SUMS");

    let content = fs::read_to_string(&checksum_path).unwrap();
    assert!(content.contains("file1.tar.gz"));
    assert!(content.contains("file2.tar.gz"));

    // Each line should have a hash and filename
    for line in content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].len(), 64); // SHA256 hash is 64 hex characters
    }
}

#[test]
fn test_empty_archive_list() {
    let output_dir = tempdir().unwrap();
    let binaries: Vec<PathBuf> = vec![];

    let result = create_archive(
        &binaries,
        output_dir.path(),
        "empty-archive",
        ArchiveFormat::Tgz,
    );

    // Should succeed even with empty list
    assert!(result.is_ok());
}

#[test]
fn test_checksum_format() {
    let temp_dir = tempdir().unwrap();

    // Create a test file with known content
    let file1 = temp_dir.path().join("test.txt");
    fs::write(&file1, b"Hello, World!").unwrap();

    let files = vec![file1];
    let checksum_path = generate_checksums(&files, temp_dir.path()).unwrap();

    let content = fs::read_to_string(&checksum_path).unwrap();

    // The SHA256 hash of "Hello, World!" should be consistent
    assert!(content.contains("dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f"));
    assert!(content.contains("test.txt"));
}

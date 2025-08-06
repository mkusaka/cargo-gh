use cargo_ghdist::cli::{ArchiveFormat, Args};

#[test]
fn test_targets_default() {
    let args = Args {
        tag: None,
        targets: None,
        format: ArchiveFormat::Tgz,
        draft: false,
        skip_publish: true,
        no_checksum: false,
        config: None,
        verbose: false,
        repository: Some("owner/repo".to_string()),
        github_token: None,
        bins: None,
        profile: "release".to_string(),
    };

    let targets = args.targets();
    assert_eq!(targets.len(), 2);
    assert!(targets.contains(&"x86_64-unknown-linux-gnu".to_string()));
    assert!(targets.contains(&"aarch64-unknown-linux-gnu".to_string()));
}

#[test]
fn test_targets_override() {
    let args = Args {
        tag: None,
        targets: Some(vec![
            "x86_64-apple-darwin".to_string(),
            "aarch64-apple-darwin".to_string(),
        ]),
        format: ArchiveFormat::Tgz,
        draft: false,
        skip_publish: true,
        no_checksum: false,
        config: None,
        verbose: false,
        repository: Some("owner/repo".to_string()),
        github_token: None,
        bins: None,
        profile: "release".to_string(),
    };

    let targets = args.targets();
    assert_eq!(targets.len(), 2);
    assert!(targets.contains(&"x86_64-apple-darwin".to_string()));
    assert!(targets.contains(&"aarch64-apple-darwin".to_string()));
}

#[test]
fn test_parse_repository_from_arg() {
    let args = Args {
        tag: None,
        targets: None,
        format: ArchiveFormat::Tgz,
        draft: false,
        skip_publish: true,
        no_checksum: false,
        config: None,
        verbose: false,
        repository: Some("owner/repo".to_string()),
        github_token: None,
        bins: None,
        profile: "release".to_string(),
    };

    let (owner, repo) = args.parse_repository().unwrap();
    assert_eq!(owner, "owner");
    assert_eq!(repo, "repo");
}

#[test]
fn test_parse_repository_invalid_format() {
    let args = Args {
        tag: None,
        targets: None,
        format: ArchiveFormat::Tgz,
        draft: false,
        skip_publish: true,
        no_checksum: false,
        config: None,
        verbose: false,
        repository: Some("invalid-format".to_string()),
        github_token: None,
        bins: None,
        profile: "release".to_string(),
    };

    assert!(args.parse_repository().is_err());
}

#[test]
fn test_archive_format_display() {
    assert_eq!(format!("{}", ArchiveFormat::Tgz), "tgz");
    assert_eq!(format!("{}", ArchiveFormat::Zip), "zip");
}

#[test]
fn test_profile_default() {
    let args = Args {
        tag: None,
        targets: None,
        format: ArchiveFormat::Tgz,
        draft: false,
        skip_publish: true,
        no_checksum: false,
        config: None,
        verbose: false,
        repository: Some("owner/repo".to_string()),
        github_token: None,
        bins: None,
        profile: "release".to_string(),
    };

    assert_eq!(args.profile, "release");
}

#[test]
fn test_draft_mode() {
    let args = Args {
        tag: None,
        targets: None,
        format: ArchiveFormat::Tgz,
        draft: true,
        skip_publish: true,
        no_checksum: false,
        config: None,
        verbose: false,
        repository: Some("owner/repo".to_string()),
        github_token: None,
        bins: None,
        profile: "release".to_string(),
    };

    assert!(args.draft);
}

#[test]
fn test_checksum_generation() {
    let args = Args {
        tag: None,
        targets: None,
        format: ArchiveFormat::Tgz,
        draft: false,
        skip_publish: true,
        no_checksum: false,
        config: None,
        verbose: false,
        repository: Some("owner/repo".to_string()),
        github_token: None,
        bins: None,
        profile: "release".to_string(),
    };

    assert!(!args.no_checksum);
}

#[test]
fn test_bins_filter() {
    let args = Args {
        tag: None,
        targets: None,
        format: ArchiveFormat::Tgz,
        draft: false,
        skip_publish: true,
        no_checksum: false,
        config: None,
        verbose: false,
        repository: Some("owner/repo".to_string()),
        github_token: None,
        bins: Some(vec!["bin1".to_string(), "bin2".to_string()]),
        profile: "release".to_string(),
    };

    assert_eq!(args.bins.unwrap().len(), 2);
}

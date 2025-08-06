use cargo_ghinstall::cli::Args;

#[test]
fn test_parse_repo_with_tag() {
    let args = Args {
        repo: "owner/repo@v1.2.3".to_string(),
        tag: None,
        bin: None,
        bins: false,
        target: None,
        install_dir: "~/.cargo/bin".to_string(),
        show_notes: false,
        verify_signature: false,
        no_fallback: false,
        config: None,
        verbose: false,
    };

    let (owner, repo, tag) = args.parse_repo().unwrap();
    assert_eq!(owner, "owner");
    assert_eq!(repo, "repo");
    assert_eq!(tag, Some("v1.2.3".to_string()));
}

#[test]
fn test_parse_repo_without_tag() {
    let args = Args {
        repo: "owner/repo".to_string(),
        tag: Some("v2.0.0".to_string()),
        bin: None,
        bins: false,
        target: None,
        install_dir: "~/.cargo/bin".to_string(),
        show_notes: false,
        verify_signature: false,
        no_fallback: false,
        config: None,
        verbose: false,
    };

    let (owner, repo, tag) = args.parse_repo().unwrap();
    assert_eq!(owner, "owner");
    assert_eq!(repo, "repo");
    assert_eq!(tag, Some("v2.0.0".to_string()));
}

#[test]
fn test_parse_repo_with_hash_tag() {
    let args = Args {
        repo: "owner/repo@vabcdef0".to_string(),
        tag: None,
        bin: None,
        bins: false,
        target: None,
        install_dir: "~/.cargo/bin".to_string(),
        show_notes: false,
        verify_signature: false,
        no_fallback: false,
        config: None,
        verbose: false,
    };

    let (owner, repo, tag) = args.parse_repo().unwrap();
    assert_eq!(owner, "owner");
    assert_eq!(repo, "repo");
    assert_eq!(tag, Some("vabcdef0".to_string()));
}

#[test]
fn test_parse_repo_with_plain_hash() {
    let args = Args {
        repo: "owner/repo@abcdef0".to_string(),
        tag: None,
        bin: None,
        bins: false,
        target: None,
        install_dir: "~/.cargo/bin".to_string(),
        show_notes: false,
        verify_signature: false,
        no_fallback: false,
        config: None,
        verbose: false,
    };

    let (owner, repo, tag) = args.parse_repo().unwrap();
    assert_eq!(owner, "owner");
    assert_eq!(repo, "repo");
    assert_eq!(tag, Some("abcdef0".to_string()));
}

#[test]
fn test_parse_repo_with_branch_name() {
    let args = Args {
        repo: "owner/repo@main".to_string(),
        tag: None,
        bin: None,
        bins: false,
        target: None,
        install_dir: "~/.cargo/bin".to_string(),
        show_notes: false,
        verify_signature: false,
        no_fallback: false,
        config: None,
        verbose: false,
    };

    let (owner, repo, tag) = args.parse_repo().unwrap();
    assert_eq!(owner, "owner");
    assert_eq!(repo, "repo");
    assert_eq!(tag, Some("main".to_string()));
}

#[test]
fn test_parse_repo_invalid_format() {
    let args = Args {
        repo: "invalid-format".to_string(),
        tag: None,
        bin: None,
        bins: false,
        target: None,
        install_dir: "~/.cargo/bin".to_string(),
        show_notes: false,
        verify_signature: false,
        no_fallback: false,
        config: None,
        verbose: false,
    };

    assert!(args.parse_repo().is_err());
}

#[test]
fn test_target_detection() {
    let args = Args {
        repo: "owner/repo".to_string(),
        tag: None,
        bin: None,
        bins: false,
        target: None,
        install_dir: "~/.cargo/bin".to_string(),
        show_notes: false,
        verify_signature: false,
        no_fallback: false,
        config: None,
        verbose: false,
    };

    let target = args.target();
    // Should return a valid target triple
    assert!(!target.is_empty());
    assert!(target.contains('-'));
}

#[test]
fn test_target_override() {
    let args = Args {
        repo: "owner/repo".to_string(),
        tag: None,
        bin: None,
        bins: false,
        target: Some("x86_64-pc-windows-msvc".to_string()),
        install_dir: "~/.cargo/bin".to_string(),
        show_notes: false,
        verify_signature: false,
        no_fallback: false,
        config: None,
        verbose: false,
    };

    let target = args.target();
    assert_eq!(target, "x86_64-pc-windows-msvc");
}

#[test]
fn test_install_dir_expansion() {
    let args = Args {
        repo: "owner/repo".to_string(),
        tag: None,
        bin: None,
        bins: false,
        target: None,
        install_dir: "~/custom/bin".to_string(),
        show_notes: false,
        verify_signature: false,
        no_fallback: false,
        config: None,
        verbose: false,
    };

    let install_dir = args.install_dir();
    // Should expand ~ to home directory
    assert!(!install_dir.to_string_lossy().starts_with("~"));
}

#[test]
fn test_install_dir_absolute_path() {
    let args = Args {
        repo: "owner/repo".to_string(),
        tag: None,
        bin: None,
        bins: false,
        target: None,
        install_dir: "/usr/local/bin".to_string(),
        show_notes: false,
        verify_signature: false,
        no_fallback: false,
        config: None,
        verbose: false,
    };

    let install_dir = args.install_dir();
    assert_eq!(install_dir.to_string_lossy(), "/usr/local/bin");
}

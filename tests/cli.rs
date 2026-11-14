use assert_cmd::Command;

#[test]
fn help_lists_all_options() {
    let out = Command::cargo_bin("git-merge-safe")
        .unwrap()
        .arg("--help")
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    for flag in ["--source", "--dry-run", "--keep-temp", "--verbose"] {
        assert!(text.contains(flag), "missing {flag} in:\n{text}");
    }
}

#[test]
fn version_prints_cargo_version() {
    let out = Command::cargo_bin("git-merge-safe")
        .unwrap()
        .arg("--version")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains(env!("CARGO_PKG_VERSION")));
}

mod common;
use common::TestRepo;
use std::path::PathBuf;

fn run(repo: &TestRepo) -> std::process::Output {
    repo.bin().output().unwrap()
}

fn stderr(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

fn git_path(repo: &TestRepo, rel: &str) -> PathBuf {
    let p = repo.git_ok(&["rev-parse", "--git-path", rel]);
    let pb = PathBuf::from(&p);
    if pb.is_absolute() {
        pb
    } else {
        repo.path().join(pb)
    }
}

fn conflict_fixture() -> TestRepo {
    let repo = TestRepo::new();
    repo.write(".gitattributes", "docs/** merge=ours\n");
    repo.write("plain.txt", "base\n");
    repo.commit_all("base");
    repo.checkout_new("dev");
    repo.write("plain.txt", "dev\n");
    repo.commit_all("dev");
    repo.checkout("main");
    repo.write("plain.txt", "main\n");
    repo.commit_all("main");
    repo
}

#[test]
fn non_excluded_conflict_rolls_back() {
    let repo = conflict_fixture();
    let head = repo.rev("HEAD");
    let index = repo.git_ok(&["ls-files", "-s"]);
    let src = repo.rev("dev");

    let out = run(&repo);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("CONFLICT"), "{}", stderr(&out));
    assert_eq!(repo.rev("HEAD"), head);
    assert_eq!(repo.git_ok(&["ls-files", "-s"]), index);
    assert_eq!(repo.rev("dev"), src);
    assert!(repo.git_ok(&["status", "--porcelain"]).is_empty());
    assert!(repo
        .git_ok(&["branch", "--list", "merge-safe/*"])
        .is_empty());
}

#[test]
fn gitattributes_conflict_rolls_back() {
    let repo = TestRepo::new();
    repo.write(".gitattributes", "docs/** merge=ours\n");
    repo.write("plain.txt", "p\n");
    repo.commit_all("base");
    repo.checkout_new("dev");
    repo.write(
        ".gitattributes",
        "docs/** merge=ours\nAGENTS.md merge=ours\n",
    );
    repo.commit_all("dev");
    repo.checkout("main");
    repo.write(
        ".gitattributes",
        "docs/** merge=ours\nCLAUDE.md merge=ours\n",
    );
    repo.commit_all("main");

    let head = repo.rev("HEAD");
    let out = run(&repo);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(repo.rev("HEAD"), head);
    assert!(repo
        .git_ok(&["branch", "--list", "merge-safe/*"])
        .is_empty());
}

#[test]
fn gitlink_path_is_refused() {
    let sub = tempfile::TempDir::new().unwrap();
    let sub_git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .current_dir(sub.path())
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "sub git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    sub_git(&["init", "-q", "-b", "main"]);
    sub_git(&["config", "user.name", "T"]);
    sub_git(&["config", "user.email", "t@example.com"]);
    std::fs::write(sub.path().join("s.txt"), "s\n").unwrap();
    sub_git(&["add", "-A"]);
    sub_git(&["commit", "-q", "-m", "sub"]);

    let repo = TestRepo::new();
    repo.write(".gitattributes", "plugin merge=ours\n");
    repo.write("plain.txt", "p\n");
    repo.commit_all("base");
    repo.checkout_new("dev");
    repo.write("dev.txt", "d\n");
    repo.commit_all("dev");
    repo.checkout("main");
    repo.git_ok(&[
        "-c",
        "protocol.file.allow=always",
        "submodule",
        "add",
        sub.path().to_str().unwrap(),
        "plugin",
    ]);
    repo.commit_all("add submodule");

    let main_head = repo.rev("HEAD");
    let out = run(&repo);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("子模块"), "{}", stderr(&out));
    assert_eq!(repo.rev("HEAD"), main_head);
    assert!(repo
        .git_ok(&["branch", "--list", "merge-safe/*"])
        .is_empty());
}

#[cfg(unix)]
fn write_hook(repo: &TestRepo, name: &str, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    let p = git_path(repo, &format!("hooks/{name}"));
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, body).unwrap();
    let mut perms = std::fs::metadata(&p).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&p, perms).unwrap();
}

#[cfg(unix)]
#[test]
fn hook_readding_excluded_path_rolls_back() {
    let repo = TestRepo::new();
    repo.write(".gitattributes", "docs/** merge=ours\n");
    repo.write("plain.txt", "p\n");
    repo.commit_all("base");
    repo.checkout_new("dev");
    repo.write("docs/a.md", "a\n");
    repo.write("plain.txt", "p-dev\n");
    repo.commit_all("dev");
    repo.checkout("main");
    write_hook(
        &repo,
        "pre-commit",
        "#!/bin/sh\nmkdir -p docs\necho hook > docs/a.md\ngit add -f docs/a.md\n",
    );

    let head = repo.rev("HEAD");
    let out = run(&repo);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("提交后校验"), "{}", stderr(&out));
    assert_eq!(repo.rev("HEAD"), head);
    assert!(repo
        .git_ok(&["branch", "--list", "merge-safe/*"])
        .is_empty());
}

#[cfg(unix)]
#[test]
fn hook_leaving_untracked_file_rolls_back() {
    let repo = TestRepo::new();
    repo.write(".gitattributes", "docs/** merge=ours\n");
    repo.write("plain.txt", "p\n");
    repo.commit_all("base");
    repo.checkout_new("dev");
    repo.write("plain.txt", "p-dev\n");
    repo.commit_all("dev");
    repo.checkout("main");
    write_hook(&repo, "pre-commit", "#!/bin/sh\necho junk > junk.txt\n");

    let head = repo.rev("HEAD");
    let index = repo.git_ok(&["ls-files", "-s"]);
    let out = run(&repo);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("工作区不干净"), "{}", stderr(&out));
    assert_eq!(repo.rev("HEAD"), head);
    assert_eq!(repo.git_ok(&["ls-files", "-s"]), index);
    assert!(repo.git_ok(&["status", "--porcelain"]).is_empty());
    assert!(repo
        .git_ok(&["branch", "--list", "merge-safe/*"])
        .is_empty());
}

#[cfg(unix)]
#[test]
fn fast_forward_race_keeps_temp_branch() {
    let repo = TestRepo::new();
    repo.write(".gitattributes", "docs/** merge=ours\n");
    repo.write("plain.txt", "p\n");
    repo.commit_all("base");
    repo.checkout_new("dev");
    repo.write("plain.txt", "p-dev\n");
    repo.commit_all("dev");
    repo.checkout("main");
    write_hook(
        &repo,
        "post-commit",
        "#!/bin/sh\nnew=$(git commit-tree -p refs/heads/main -m race 'refs/heads/main^{tree}')\ngit update-ref refs/heads/main \"$new\"\n",
    );

    let out = run(&repo);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("fast-forward 失败"),
        "{}",
        stderr(&out)
    );
    let branches = repo.git_ok(&["branch", "--list", "merge-safe/*"]);
    assert!(!branches.is_empty(), "temp branch should be kept");
    assert!(stderr(&out).contains("merge-safe/tmp-"), "{}", stderr(&out));
}

#[test]
fn source_branch_never_changes() {
    let success = TestRepo::new();
    success.write(".gitattributes", "docs/** merge=ours\n");
    success.write("plain.txt", "p\n");
    success.commit_all("base");
    success.checkout_new("dev");
    success.write("plain.txt", "p-dev\n");
    success.commit_all("dev");
    success.checkout("main");
    let src = success.rev("dev");
    assert!(run(&success).status.success());
    assert_eq!(success.rev("dev"), src);

    let failure = conflict_fixture();
    let src = failure.rev("dev");
    assert_eq!(run(&failure).status.code(), Some(2));
    assert_eq!(failure.rev("dev"), src);
}

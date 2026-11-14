mod common;
use common::TestRepo;
use std::path::PathBuf;

fn valid() -> TestRepo {
    let repo = TestRepo::new();
    repo.write(".gitattributes", "docs/** merge=ours\n");
    repo.write("plain.txt", "p\n");
    repo.commit_all("base");
    repo.checkout_new("dev");
    repo.write("dev.txt", "d\n");
    repo.commit_all("dev");
    repo.checkout("main");
    repo.write("docs/a.md", "a\n");
    repo.commit_all("squash");
    repo
}

fn run(repo: &TestRepo, args: &[&str]) -> std::process::Output {
    repo.bin().args(args).output().unwrap()
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

#[test]
fn valid_repo_passes_dry_run() {
    let repo = valid();
    let out = run(&repo, &["--dry-run"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(!git_path(&repo, "merge-safe.lock").exists());
}

#[test]
fn detached_head_is_refused() {
    let repo = valid();
    repo.git_ok(&["checkout", "-q", "--detach", "HEAD"]);
    let out = run(&repo, &["--dry-run"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("detached HEAD"), "{}", stderr(&out));
}

#[test]
fn same_branch_is_refused() {
    let repo = valid();
    let out = run(&repo, &["--dry-run", "--source", "main"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("相同"), "{}", stderr(&out));
}

#[test]
fn missing_source_is_refused() {
    let repo = valid();
    let out = run(&repo, &["--dry-run", "--source", "nope"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("源分支不存在"), "{}", stderr(&out));
}

#[test]
fn in_progress_merge_is_refused() {
    let repo = valid();
    let head = repo.rev("HEAD");
    std::fs::write(git_path(&repo, "MERGE_HEAD"), format!("{head}\n")).unwrap();
    let out = run(&repo, &["--dry-run"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("进行中的操作"), "{}", stderr(&out));
}

#[test]
fn dirty_worktree_is_refused() {
    let repo = valid();
    repo.write("untracked.txt", "u\n");
    let out = run(&repo, &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("工作区不干净"), "{}", stderr(&out));
}

#[test]
fn merge_commit_is_refused() {
    let repo = valid();
    repo.git_ok(&["checkout", "-q", "-b", "side"]);
    repo.write("side.txt", "s\n");
    repo.commit_all("side");
    repo.checkout("main");
    repo.write("main.txt", "m\n");
    repo.commit_all("main");
    repo.git_ok(&["merge", "--no-ff", "-m", "merge", "side"]);
    let out = run(&repo, &["--dry-run"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("合并提交"), "{}", stderr(&out));
}

#[test]
fn source_ancestor_is_refused() {
    let repo = valid();
    repo.git_ok(&["reset", "--hard", "dev"]);
    let out = run(&repo, &["--dry-run"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("祖先"), "{}", stderr(&out));
}

#[test]
fn unrelated_history_is_refused() {
    let repo = valid();
    repo.git_ok(&["checkout", "-q", "--orphan", "other"]);
    repo.git_ok(&["rm", "-rq", "--cached", "."]);
    repo.write("other.txt", "o\n");
    repo.commit_all("other");
    let out = run(&repo, &["--dry-run"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("共同起点"), "{}", stderr(&out));
}

#[test]
fn info_attributes_is_refused() {
    let repo = valid();
    let p = git_path(&repo, "info/attributes");
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, "* merge=ours\n").unwrap();
    let out = run(&repo, &["--dry-run"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("info/attributes"), "{}", stderr(&out));
}

#[test]
fn existing_lock_is_refused() {
    let repo = valid();
    let p = git_path(&repo, "merge-safe.lock");
    std::fs::write(&p, "123\n").unwrap();
    let out = run(&repo, &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("merge-safe.lock"), "{}", stderr(&out));
}

#[test]
fn dry_run_skips_clean_and_lock_checks() {
    let repo = valid();
    let p = git_path(&repo, "merge-safe.lock");
    std::fs::write(&p, "123\n").unwrap();
    repo.write("untracked.txt", "u\n");
    let out = run(&repo, &["--dry-run"]);
    assert!(out.status.success(), "{}", stderr(&out));
}

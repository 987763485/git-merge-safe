mod common;
use common::TestRepo;
use std::path::PathBuf;

const RULES: &str = "docs/**            merge=ours\n\
AGENTS.md          merge=ours\n\
CLAUDE.md          merge=ours\n\
.claude/**         merge=ours\n\
.comet/**          merge=ours\n\
.superpowers/**    merge=ours\n\
.impeccable/**     merge=ours\n\
docs/index.html    -merge\n";

fn fixture() -> TestRepo {
    let repo = TestRepo::new();
    repo.write(".gitattributes", RULES);
    repo.write("docs/a.md", "a\n");
    repo.write("docs/index.html", "index\n");
    repo.write("AGENTS.md", "agents\n");
    repo.write("plain.txt", "plain\n");
    repo.commit_all("base");
    repo.checkout_new("dev");
    repo.write("docs/sub/b.md", "b\n");
    repo.write("CLAUDE.md", "claude\n");
    repo.write("docs/index.html", "index-dev\n");
    repo.commit_all("dev");
    repo.checkout("main");
    repo
}

fn run(repo: &TestRepo, args: &[&str]) -> std::process::Output {
    repo.bin().args(args).output().unwrap()
}

fn stdout(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
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
fn dry_run_lists_union_and_preserves_state() {
    let repo = fixture();
    let before_files = repo.git_ok(&["ls-files", "-s"]);
    let before_head = repo.rev("HEAD");

    let out = run(&repo, &["--dry-run"]);
    assert!(out.status.success());
    let text = stdout(&out);
    for expected in ["docs/a.md", "docs/sub/b.md", "AGENTS.md", "CLAUDE.md"] {
        assert!(text.contains(expected), "missing {expected} in:\n{text}");
    }
    assert!(!text.contains("docs/index.html"), "{text}");
    assert!(!text.contains("plain.txt"), "{text}");

    assert_eq!(repo.rev("HEAD"), before_head);
    assert_eq!(repo.git_ok(&["ls-files", "-s"]), before_files);
    assert!(repo.git_ok(&["status", "--porcelain"]).is_empty());
    assert!(repo
        .git_ok(&["branch", "--list", "merge-safe/*"])
        .is_empty());
}

#[test]
fn dry_run_ignores_dirty_worktree_and_lock() {
    let repo = fixture();
    let lock = git_path(&repo, "merge-safe.lock");
    std::fs::write(&lock, "123\n").unwrap();
    repo.write("untracked.txt", "u\n");

    let out = run(&repo, &["--dry-run"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("docs/a.md"));
}

#[test]
fn no_rules_reports_no_configuration() {
    let repo = TestRepo::new();
    repo.write("plain.txt", "p\n");
    repo.commit_all("base");
    repo.checkout_new("dev");
    repo.write("dev.txt", "d\n");
    repo.commit_all("dev");
    repo.checkout("main");

    let out = run(&repo, &["--dry-run"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("未配置排除规则"), "{}", stdout(&out));
}

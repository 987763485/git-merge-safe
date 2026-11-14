mod common;
use common::TestRepo;

fn run(repo: &TestRepo) -> std::process::Output {
    repo.bin().output().unwrap()
}

fn stdout(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn stderr(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

#[test]
fn basic_exclusion() {
    let repo = TestRepo::new();
    repo.write(".gitattributes", "docs/** merge=ours\nAGENTS.md merge=ours\n");
    repo.write("plain.txt", "base\n");
    repo.write("AGENTS.md", "v0\n");
    repo.commit_all("base");
    repo.checkout_new("dev");
    repo.write("docs/a.md", "a\n");
    repo.write("AGENTS.md", "v-dev\n");
    repo.write("plain.txt", "dev-change\n");
    repo.commit_all("dev");
    repo.checkout("main");

    let src_before = repo.rev("dev");
    let main_before = repo.rev("main");
    let commits_before = repo.git_ok(&["rev-list", "--count", "main"]);

    let out = run(&repo);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(repo.rev("dev"), src_before);
    assert_ne!(repo.rev("main"), main_before);
    assert_eq!(
        repo.git_ok(&["rev-list", "--count", "main"]),
        (commits_before.parse::<u32>().unwrap() + 1).to_string()
    );
    assert!(!repo.path().join("docs").exists());
    assert!(!repo.path().join("AGENTS.md").exists());
    assert_eq!(
        std::fs::read_to_string(repo.path().join("plain.txt")).unwrap(),
        "dev-change\n"
    );
    assert!(repo.git_ok(&["status", "--porcelain"]).is_empty());
    assert!(repo.git_ok(&["branch", "--list", "merge-safe/*"]).is_empty());
}

#[test]
fn both_branches_modified_excluded_path() {
    let repo = TestRepo::new();
    repo.write(".gitattributes", "AGENTS.md merge=ours\n");
    repo.write("AGENTS.md", "v0\n");
    repo.commit_all("base");
    repo.checkout_new("dev");
    repo.write("AGENTS.md", "v-dev\n");
    repo.write("dev.txt", "d\n");
    repo.commit_all("dev");
    repo.checkout("main");
    repo.write("AGENTS.md", "v-main\n");
    repo.commit_all("main");

    let out = run(&repo);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(!stderr(&out).contains("CONFLICT"), "{}", stderr(&out));
    assert!(!repo.path().join("AGENTS.md").exists());
    assert!(repo.path().join("dev.txt").exists());
}

#[test]
fn preexisting_untouched_excluded_path_is_removed() {
    let repo = TestRepo::new();
    repo.write(".gitattributes", "main-only.txt merge=ours\n");
    repo.write("plain.txt", "p\n");
    repo.commit_all("base");
    repo.checkout_new("dev");
    repo.write("dev.txt", "d\n");
    repo.commit_all("dev");
    repo.checkout("main");
    repo.write("main-only.txt", "m\n");
    repo.commit_all("main");

    let out = run(&repo);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(!repo.path().join("main-only.txt").exists());
    assert!(repo.path().join("dev.txt").exists());
}

#[test]
fn merge_exception_is_kept() {
    let repo = TestRepo::new();
    repo.write(
        ".gitattributes",
        "docs/** merge=ours\ndocs/index.html -merge\n",
    );
    repo.write("docs/index.html", "index\n");
    repo.commit_all("base");
    repo.checkout_new("dev");
    repo.write("docs/index.html", "index-dev\n");
    repo.commit_all("dev");
    repo.checkout("main");

    let out = run(&repo);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        std::fs::read_to_string(repo.path().join("docs/index.html")).unwrap(),
        "index-dev\n"
    );
}

#[test]
fn all_changes_excluded_means_no_content() {
    let repo = TestRepo::new();
    repo.write(".gitattributes", "docs/** merge=ours\n");
    repo.write("plain.txt", "p\n");
    repo.commit_all("base");
    repo.checkout_new("dev");
    repo.write("docs/a.md", "a\n");
    repo.commit_all("dev");
    repo.checkout("main");

    let head = repo.rev("HEAD");
    let out = run(&repo);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stdout(&out).contains("无内容可提交"), "{}", stdout(&out));
    assert_eq!(repo.rev("HEAD"), head);
    assert!(!repo.path().join("docs").exists());
    assert!(repo.git_ok(&["branch", "--list", "merge-safe/*"]).is_empty());
}

#[test]
fn repeat_run_reports_no_content() {
    let repo = TestRepo::new();
    repo.write(".gitattributes", "docs/** merge=ours\n");
    repo.write("plain.txt", "p\n");
    repo.commit_all("base");
    repo.checkout_new("dev");
    repo.write("docs/a.md", "a\n");
    repo.write("plain.txt", "p-dev\n");
    repo.commit_all("dev");
    repo.checkout("main");

    let first = run(&repo);
    assert!(first.status.success(), "{}", stderr(&first));
    let head_after_first = repo.rev("HEAD");

    let second = run(&repo);
    assert!(second.status.success(), "{}", stderr(&second));
    assert!(stdout(&second).contains("无内容可提交"), "{}", stdout(&second));
    assert_eq!(repo.rev("HEAD"), head_after_first);
}

#[test]
fn no_rules_merges_normally() {
    let repo = TestRepo::new();
    repo.write("plain.txt", "p\n");
    repo.commit_all("base");
    repo.checkout_new("dev");
    repo.write("plain.txt", "p-dev\n");
    repo.commit_all("dev");
    repo.checkout("main");

    let out = run(&repo);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        std::fs::read_to_string(repo.path().join("plain.txt")).unwrap(),
        "p-dev\n"
    );
    assert!(repo.git_ok(&["status", "--porcelain"]).is_empty());
}

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

fn basic_fixture() -> TestRepo {
    let repo = TestRepo::new();
    repo.write(".gitattributes", "docs/** merge=ours\n");
    repo.write("plain.txt", "p\n");
    repo.commit_all("base");
    repo.checkout_new("dev");
    repo.write("docs/a.md", "a\n");
    repo.write("plain.txt", "p-dev\n");
    repo.commit_all("dev");
    repo.checkout("main");
    repo
}

#[test]
fn space_in_path_is_removed() {
    let repo = TestRepo::new();
    repo.write(".gitattributes", "dir/** merge=ours\n");
    repo.write("plain.txt", "p\n");
    repo.commit_all("base");
    repo.checkout_new("dev");
    repo.write("dir/space name.txt", "s\n");
    repo.write("plain.txt", "p-dev\n");
    repo.commit_all("dev");
    repo.checkout("main");

    let out = run(&repo);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(!repo.path().join("dir/space name.txt").exists());
}

#[test]
fn quoted_pattern_is_honored() {
    let repo = TestRepo::new();
    repo.write(".gitattributes", "\"space name.txt\" merge=ours\n");
    repo.write("plain.txt", "p\n");
    repo.commit_all("base");
    repo.checkout_new("dev");
    repo.write("space name.txt", "s\n");
    repo.write("plain.txt", "p-dev\n");
    repo.commit_all("dev");
    repo.checkout("main");

    let out = run(&repo);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(!repo.path().join("space name.txt").exists());
}

#[test]
fn chinese_path_is_removed() {
    let repo = TestRepo::new();
    repo.write(".gitattributes", "docs/** merge=ours\n");
    repo.write("plain.txt", "p\n");
    repo.commit_all("base");
    repo.checkout_new("dev");
    repo.write("docs/文档.md", "c\n");
    repo.write("plain.txt", "p-dev\n");
    repo.commit_all("dev");
    repo.checkout("main");

    let out = run(&repo);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(!repo.path().join("docs/文档.md").exists());
}

#[cfg(unix)]
#[test]
fn newline_in_path_is_removed() {
    let repo = TestRepo::new();
    repo.write(".gitattributes", "dir/** merge=ours\n");
    repo.write("plain.txt", "p\n");
    repo.commit_all("base");
    repo.checkout_new("dev");
    repo.write("dir/line\nbreak.txt", "n\n");
    repo.write("plain.txt", "p-dev\n");
    repo.commit_all("dev");
    repo.checkout("main");

    let out = run(&repo);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(!repo.path().join("dir/line\nbreak.txt").exists());
}

#[cfg(unix)]
#[test]
fn symlink_is_removed_target_untouched() {
    use std::os::unix::fs::symlink;
    let repo = TestRepo::new();
    repo.write(".gitattributes", "link merge=ours\n");
    repo.write("plain.txt", "p\n");
    repo.commit_all("base");
    repo.checkout_new("dev");
    symlink("plain.txt", repo.path().join("link")).unwrap();
    repo.write("plain.txt", "p-dev\n");
    repo.commit_all("dev");
    repo.checkout("main");

    let out = run(&repo);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(std::fs::symlink_metadata(repo.path().join("link")).is_err());
    assert!(repo.path().join("plain.txt").exists());
}

#[test]
fn full_example_rules_end_to_end() {
    let repo = TestRepo::new();
    repo.write(
        ".gitattributes",
        "docs/**            merge=ours\n\
         AGENTS.md          merge=ours\n\
         CLAUDE.md          merge=ours\n\
         .claude/**         merge=ours\n\
         .comet/**          merge=ours\n\
         .superpowers/**    merge=ours\n\
         .impeccable/**     merge=ours\n\
         docs/index.html    -merge\n",
    );
    repo.write("docs/a.md", "a\n");
    repo.write("docs/index.html", "index\n");
    repo.write("AGENTS.md", "agents\n");
    repo.write("CLAUDE.md", "claude\n");
    repo.write("plain.txt", "p\n");
    repo.commit_all("base");
    repo.checkout_new("dev");
    repo.write("docs/sub/b.md", "b\n");
    repo.write("docs/index.html", "index-dev\n");
    repo.commit_all("dev");
    repo.checkout("main");

    let out = run(&repo);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(!repo.path().join("docs/a.md").exists());
    assert!(!repo.path().join("docs/sub/b.md").exists());
    assert!(!repo.path().join("AGENTS.md").exists());
    assert!(!repo.path().join("CLAUDE.md").exists());
    assert_eq!(
        std::fs::read_to_string(repo.path().join("docs/index.html")).unwrap(),
        "index-dev\n"
    );
    assert!(repo.path().join("plain.txt").exists());
}

#[test]
fn new_rule_only_on_dev_applies() {
    let repo = TestRepo::new();
    repo.write(".gitattributes", "");
    repo.write("AGENTS.md", "v0\n");
    repo.write("plain.txt", "p\n");
    repo.commit_all("base");
    repo.checkout_new("dev");
    repo.write(".gitattributes", "AGENTS.md merge=ours\n");
    repo.write("AGENTS.md", "v-dev\n");
    repo.commit_all("dev");
    repo.checkout("main");
    repo.write("AGENTS.md", "v-main\n");
    repo.commit_all("main");

    let out = run(&repo);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(!stderr(&out).contains("CONFLICT"), "{}", stderr(&out));
    assert!(!repo.path().join("AGENTS.md").exists());
}

#[test]
fn runs_from_subdirectory() {
    let repo = TestRepo::new();
    repo.write(".gitattributes", "docs/** merge=ours\n");
    repo.write("plain.txt", "p\n");
    repo.write("sub/keep.txt", "k\n");
    repo.commit_all("base");
    repo.checkout_new("dev");
    repo.write("docs/a.md", "a\n");
    repo.write("plain.txt", "p-dev\n");
    repo.commit_all("dev");
    repo.checkout("main");

    let out = repo
        .bin()
        .current_dir(repo.path().join("sub"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(!repo.path().join("docs").exists());
    assert_eq!(
        std::fs::read_to_string(repo.path().join("plain.txt")).unwrap(),
        "p-dev\n"
    );
}

#[test]
fn source_checked_out_in_another_worktree_is_untouched() {
    let repo = basic_fixture();
    let wt = tempfile::TempDir::new().unwrap();
    let wt_path = wt.path().join("dev-wt");
    repo.git_ok(&["worktree", "add", wt_path.to_str().unwrap(), "dev"]);
    let src = repo.rev("dev");

    let out = run(&repo);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        repo.git_ok(&["-C", wt_path.to_str().unwrap(), "rev-parse", "HEAD"]),
        src
    );
    assert!(repo
        .git_ok(&["-C", wt_path.to_str().unwrap(), "status", "--porcelain"])
        .is_empty());
}

#[test]
fn global_attributes_do_not_widen_the_removal() {
    let repo = basic_fixture();
    let global_dir = tempfile::TempDir::new().unwrap();
    let global = global_dir.path().join("global-attrs");
    std::fs::write(&global, "* merge=ours\n").unwrap();
    repo.git_ok(&["config", "core.attributesFile", global.to_str().unwrap()]);

    let out = run(&repo);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(!repo.path().join("docs").exists());
    assert_eq!(
        std::fs::read_to_string(repo.path().join("plain.txt")).unwrap(),
        "p-dev\n"
    );
}

#[test]
fn keep_temp_preserves_branch() {
    let repo = basic_fixture();
    let out = repo.bin().arg("--keep-temp").output().unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    let branches = repo.git_ok(&["branch", "--list", "merge-safe/*"]);
    assert!(branches.contains("merge-safe/tmp-"), "{branches}");
}

#[test]
fn checkout_dev_after_success() {
    let repo = basic_fixture();
    let out = run(&repo);
    assert!(out.status.success(), "{}", stderr(&out));
    repo.git_ok(&["checkout", "-q", "dev"]);
    assert!(repo.git_ok(&["status", "--porcelain"]).is_empty());
}

#[test]
fn verbose_prints_merge_command() {
    let repo = basic_fixture();
    let out = repo.bin().arg("--verbose").output().unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stderr(&out).contains("merge --squash"), "{}", stderr(&out));
}

#[test]
fn existing_lock_reports_its_path() {
    let repo = basic_fixture();
    let lock = git_path(&repo, "merge-safe.lock");
    std::fs::write(&lock, "123\n").unwrap();
    let out = run(&repo);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("merge-safe.lock"), "{}", stderr(&out));
}

#[test]
fn literal_pathspec_metacharacters_are_not_globs() {
    let repo = TestRepo::new();
    repo.write(
        ".gitattributes",
        "a[[]1].md merge=ours\n:weird.txt merge=ours\n",
    );
    repo.write("a1.md", "keep\n");
    repo.write("plain.txt", "p\n");
    repo.commit_all("base");
    repo.checkout_new("dev");
    repo.write("a[1].md", "excluded\n");
    repo.write(":weird.txt", "excluded\n");
    repo.write("plain.txt", "p-dev\n");
    repo.commit_all("dev");
    repo.checkout("main");

    let out = run(&repo);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(!repo.path().join("a[1].md").exists());
    assert!(!repo.path().join(":weird.txt").exists());
    assert!(repo.path().join("a1.md").exists());
}

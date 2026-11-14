mod common;
use common::TestRepo;
use git_merge_safe::git::{parse_git_version, split_nul, version_supported, Git};

fn repo_git() -> (TestRepo, Git) {
    let repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    repo.commit_all("base");
    let git = Git::discover_in(repo.path(), false).unwrap();
    (repo, git)
}

#[test]
fn rev_parse_returns_full_sha() {
    let (repo, git) = repo_git();
    let sha = git.rev_parse("HEAD").unwrap();
    assert_eq!(sha, repo.rev("HEAD"));
    assert_eq!(sha.len(), 40);
}

#[test]
fn current_branch_detects_detached_head() {
    let (repo, git) = repo_git();
    assert_eq!(git.current_branch().unwrap().as_deref(), Some("main"));
    repo.git_ok(&["checkout", "-q", "--detach", "HEAD"]);
    assert_eq!(git.current_branch().unwrap(), None);
}

#[test]
fn status_porcelain_reports_untracked() {
    let (repo, git) = repo_git();
    assert!(git.status_porcelain().unwrap().is_empty());
    repo.write("untracked.txt", "u\n");
    let s = String::from_utf8_lossy(&git.status_porcelain().unwrap()).to_string();
    assert!(s.starts_with("?? "), "{s:?}");
}

#[test]
fn count_merges_counts_real_merges() {
    let repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    repo.commit_all("base");
    repo.checkout_new("side");
    repo.write("side.txt", "s\n");
    repo.commit_all("side");
    repo.checkout("main");
    repo.write("main.txt", "m\n");
    repo.commit_all("main");
    let git = Git::discover_in(repo.path(), false).unwrap();
    assert_eq!(git.count_merges("main").unwrap(), 0);
    repo.git_ok(&["merge", "--no-ff", "-m", "merge", "side"]);
    assert_eq!(git.count_merges("main").unwrap(), 1);
}

#[test]
fn ancestry_before_and_after_fast_forward() {
    let (repo, git) = repo_git();
    repo.checkout_new("dev");
    repo.write("dev.txt", "d\n");
    repo.commit_all("dev");
    repo.checkout("main");
    assert!(!git.is_ancestor("dev", "main").unwrap());
    repo.git_ok(&["merge", "--ff-only", "dev"]);
    assert!(git.is_ancestor("dev", "main").unwrap());
}

#[test]
fn merge_bases_match_and_unrelated() {
    let (repo, git) = repo_git();
    repo.checkout_new("dev");
    repo.write("dev.txt", "d\n");
    repo.commit_all("dev");
    repo.checkout("main");
    assert_eq!(git.merge_bases("main", "dev").unwrap().len(), 1);
    repo.git_ok(&["checkout", "-q", "--orphan", "other"]);
    repo.git_ok(&["rm", "-rq", "--cached", "."]);
    repo.write("other.txt", "o\n");
    repo.commit_all("other");
    assert!(git.merge_bases("main", "other").unwrap().is_empty());
}

#[test]
fn ls_files_and_modes() {
    let repo = TestRepo::new();
    repo.write(".gitattributes", "docs/** merge=ours\n");
    repo.write("AGENTS.md", "agents\n");
    repo.write("docs/a.md", "a\n");
    repo.commit_all("base");
    let git = Git::discover_in(repo.path(), false).unwrap();

    let files = git.ls_files_z().unwrap();
    assert!(files.contains(&b"AGENTS.md".to_vec()));
    assert!(files.contains(&b".gitattributes".to_vec()));

    let staged = git.ls_files_stage_z().unwrap();
    let agents = staged.iter().find(|(p, _)| p == b"AGENTS.md").unwrap();
    assert_eq!(agents.1, "100644");

    let head = repo.rev("HEAD");
    let cacheinfo = format!("160000,{head},plugin");
    repo.git_ok(&["update-index", "--add", "--cacheinfo", &cacheinfo]);
    let staged = git.ls_files_stage_z().unwrap();
    let plugin = staged.iter().find(|(p, _)| p == b"plugin").unwrap();
    assert_eq!(plugin.1, "160000");
}

#[test]
fn ls_tree_and_show_file() {
    let repo = TestRepo::new();
    repo.write(".gitattributes", "docs/** merge=ours\n");
    repo.write("docs/a.md", "a\n");
    repo.commit_all("base");
    repo.checkout_new("dev");
    repo.write("docs/b.md", "b\n");
    repo.commit_all("dev");
    repo.checkout("main");
    let git = Git::discover_in(repo.path(), false).unwrap();

    assert!(git.ls_tree_z("dev").unwrap().contains(&b"docs/a.md".to_vec()));
    let attrs = git.show_file("dev", ".gitattributes").unwrap().unwrap();
    assert!(String::from_utf8_lossy(&attrs).contains("merge=ours"));
    assert!(git.show_file("dev", "missing").unwrap().is_none());
}

#[test]
fn diff_cached_and_branch_exists() {
    let (repo, git) = repo_git();
    assert!(git.diff_cached_is_empty().unwrap());
    repo.write("a.txt", "changed\n");
    repo.git_ok(&["add", "a.txt"]);
    assert!(!git.diff_cached_is_empty().unwrap());
    assert!(git.branch_exists("main").unwrap());
    assert!(!git.branch_exists("nope").unwrap());
}

#[test]
fn in_progress_and_info_attributes() {
    let (repo, git) = repo_git();
    assert!(git.in_progress_operation().unwrap().is_none());
    let head = repo.rev("HEAD");
    std::fs::write(git.abs_git_path("MERGE_HEAD").unwrap(), format!("{head}\n")).unwrap();
    assert_eq!(
        git.in_progress_operation().unwrap().as_deref(),
        Some("MERGE_HEAD")
    );

    assert!(!git.info_attributes_present().unwrap());
    let info = git.abs_git_path("info/attributes").unwrap();
    std::fs::create_dir_all(info.parent().unwrap()).unwrap();
    std::fs::write(&info, "* merge=ours\n").unwrap();
    assert!(git.info_attributes_present().unwrap());
}

#[test]
fn version_parsing() {
    assert_eq!(
        parse_git_version("git version 2.39.5 (Apple Git-154)"),
        Some((2, 39))
    );
    assert!(version_supported((2, 30)));
    assert!(!version_supported((2, 29)));
}

#[test]
fn nul_splitting() {
    assert_eq!(
        split_nul(b"a\0b\0"),
        vec![b"a".to_vec(), b"b".to_vec()]
    );
    assert!(split_nul(b"").is_empty());
}

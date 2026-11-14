mod common;
use common::TestRepo;
use git_merge_safe::git::Git;
use git_merge_safe::rules::{excluded_in_index, excluded_in_tree};

fn full_repo() -> (TestRepo, Git) {
    let repo = TestRepo::new();
    repo.write(
        ".gitattributes",
        "docs/**            merge=ours\n\
         AGENTS.md          merge=ours\n\
         CLAUDE.md          merge=ours\n\
         .cualde/**         merge=ours\n\
         .comet/**          merge=ours\n\
         .superpowers/**    merge=ours\n\
         .impeccable/**     merge=ours\n\
         docs/index.html    -merge\n",
    );
    repo.write("docs/a.md", "a\n");
    repo.write("docs/sub/b.md", "b\n");
    repo.write("docs/index.html", "index\n");
    repo.write("AGENTS.md", "agents\n");
    repo.write("CLAUDE.md", "claude\n");
    repo.write("plain.txt", "plain\n");
    repo.commit_all("base");
    let git = Git::discover_in(repo.path(), false).unwrap();
    (repo, git)
}

#[test]
fn full_example_matches_exactly() {
    let (_repo, git) = full_repo();
    let all = git.ls_files_z().unwrap();
    let mut excluded = excluded_in_index(&git, &all).unwrap();
    excluded.sort();
    assert_eq!(
        excluded,
        vec![
            b"AGENTS.md".to_vec(),
            b"CLAUDE.md".to_vec(),
            b"docs/a.md".to_vec(),
            b"docs/sub/b.md".to_vec(),
        ]
    );
}

#[test]
fn nested_gitattributes_are_honored() {
    let repo = TestRepo::new();
    repo.write(".gitattributes", "");
    repo.write("docs/.gitattributes", "*.md merge=ours\n");
    repo.write("docs/a.md", "a\n");
    repo.write("docs/a.txt", "t\n");
    repo.commit_all("base");
    let git = Git::discover_in(repo.path(), false).unwrap();
    let all = git.ls_files_z().unwrap();
    assert_eq!(
        excluded_in_index(&git, &all).unwrap(),
        vec![b"docs/a.md".to_vec()]
    );
}

#[test]
fn tree_evaluation_uses_the_tree_rules() {
    let (_repo, git) = full_repo();
    let paths = vec![b"AGENTS.md".to_vec(), b"plain.txt".to_vec()];
    assert_eq!(
        excluded_in_tree(&git, "HEAD", &paths).unwrap(),
        vec![b"AGENTS.md".to_vec()]
    );
}

#[test]
fn user_global_attributes_are_ignored() {
    let (repo, git) = full_repo();
    let global = repo.path().join("global-attrs");
    std::fs::write(&global, "* merge=ours\n").unwrap();
    repo.git_ok(&["config", "core.attributesFile", global.to_str().unwrap()]);

    let all = git.ls_files_z().unwrap();
    let mut excluded = excluded_in_index(&git, &all).unwrap();
    excluded.sort();
    assert_eq!(
        excluded,
        vec![
            b"AGENTS.md".to_vec(),
            b"CLAUDE.md".to_vec(),
            b"docs/a.md".to_vec(),
            b"docs/sub/b.md".to_vec(),
        ]
    );
}

#[test]
fn empty_input_returns_empty() {
    let (_repo, git) = full_repo();
    assert!(excluded_in_index(&git, &[]).unwrap().is_empty());
    assert!(excluded_in_tree(&git, "HEAD", &[]).unwrap().is_empty());
}

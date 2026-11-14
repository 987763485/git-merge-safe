mod common;
use common::TestRepo;
use git_merge_safe::git::Git;
use std::ffi::OsStr;

fn repo_git() -> (TestRepo, Git) {
    let repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    repo.commit_all("base");
    let git = Git::discover_in(repo.path(), false).unwrap();
    (repo, git)
}

#[test]
fn discover_finds_top_level_and_git_dir() {
    let (repo, git) = repo_git();
    assert_eq!(
        git.top_level().canonicalize().unwrap(),
        repo.path().canonicalize().unwrap()
    );
    assert!(git.git_dir().exists());
}

#[test]
fn run_str_captures_stdout() {
    let (_repo, git) = repo_git();
    let out = git.run_str(&["rev-parse", "HEAD"]).unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.ends_with('\n'));
    assert_eq!(text.trim().len(), 40);
}

#[test]
fn ok_reports_stderr_on_failure() {
    let (_repo, git) = repo_git();
    let out = git.run_str(&["cat-file", "-e", "nope"]).unwrap();
    assert!(!out.status.success());
    let err = git.ok(out).unwrap_err();
    assert!(err.to_string().to_lowercase().contains("fatal"), "{err}");
}

#[test]
fn run_with_input_pipes_stdin() {
    let (_repo, git) = repo_git();
    let args = [OsStr::new("hash-object"), OsStr::new("--stdin")];
    let out = git.run_with_input(&args, b"hello\n", &[]).unwrap();
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap().trim(),
        "ce013625030ba8dba906f756967f9e9ca394464a"
    );
}

#[test]
fn abs_git_path_is_absolute() {
    let (_repo, git) = repo_git();
    let p = git.abs_git_path("merge-safe.lock").unwrap();
    assert!(p.is_absolute());
    assert!(p.ends_with("merge-safe.lock"));
}

#[test]
fn large_stdin_does_not_deadlock() {
    let (_repo, git) = repo_git();
    let mut input = Vec::new();
    for i in 0..4000u64 {
        input.extend_from_slice(format!("{i:040x}\n").as_bytes());
    }
    let args = [OsStr::new("cat-file"), OsStr::new("--batch")];
    let out = git.run_with_input(&args, &input, &[]).unwrap();
    assert!(out.status.success());
    assert!(out.stdout.len() > 64 * 1024);
}

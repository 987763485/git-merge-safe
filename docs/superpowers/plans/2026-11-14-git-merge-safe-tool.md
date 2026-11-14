# git-merge-safe Tool Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `git-merge-safe` Rust CLI that squash-merges a source branch into the current branch while removing every path marked `merge=ours`, never modifying the current branch on failure and never modifying the source branch.

**Architecture:** A small binary around a `Git` runner that always executes with cwd at the repository top level. All rule evaluation is delegated to `git check-attr --cached` against a temporary index. All writes happen on a temporary branch and reach the current branch only through a final `--ff-only` merge.

**Tech Stack:** Rust 2021, `clap 4` (derive), `anyhow 1`; integration tests with `assert_cmd 2` + `tempfile 3`.

**Spec:** `docs/superpowers/specs/2026-11-14-git-merge-safe-design.md`

## Global Constraints

- Git ≥ 2.30; probe at startup, fail with a clear message if older.
- Runtime dependencies only `clap` and `anyhow`; dev-dependencies only `assert_cmd` and `tempfile`. No glob crate, no shell invocation.
- Every child `git` command passes argv directly and runs with `current_dir` = repository top level.
- Exclusion marker is exactly `merge=ours`; `-merge` is exempt. `unset` and `unspecified` are never excluded.
- Rule evaluation is isolated: `GIT_ATTR_NOSYSTEM=1` and `-c core.attributesFile=<git-path merge-safe/no-attributes>` (nonexistent file). Refuse to run when `<git-path info/attributes>` exists and is non-empty.
- On every failure path the current branch's HEAD, index and worktree are unchanged; on every path `git rev-parse <src>` is identical before and after.
- Temporary branch `merge-safe/tmp-<pid>-<random>`; lock file `<git-path merge-safe.lock>`.
- Exit codes: 0 success or nothing to commit, 1 preflight refusal, 2 git failure or severe source-branch violation.
- User-facing messages are Chinese, prefixed `[git-merge-safe]`.
- No push, no tag, no `git config` writes.

## Review Focus

1. Paths containing newlines, spaces or non-ASCII bytes — deleted exactly, never split or re-encoded. Tests: Task 4 parser unit tests; Task 9 end-to-end.
2. Symlinked excluded paths and gitlink entries — symlink removed as a file; gitlink refuses and rolls back. Tests: Task 9 (symlink), Task 8 (gitlink).
3. Hooks that re-add an excluded path or leave worktree files — post-commit verification rolls back. Test: Task 8.
4. Machine-local attribute sources (`core.attributesFile`, `.git/info/attributes`) — global file ignored, info file refused. Tests: Task 4, Task 5, Task 9.
5. Concurrent movement of the current branch between start and fast-forward — fast-forward fails, temp branch preserved, current branch not reset by the tool. Test: Task 8.

## File Structure

| File | Responsibility |
|---|---|
| `Cargo.toml` | Package `git-merge-safe`, binary of the same name, deps pinned to major versions. |
| `src/lib.rs` | Library root exposing modules so integration tests can use the same code as the binary. |
| `src/main.rs` | Parse CLI, dispatch to preflight + dry-run/flow, map `Failure` to exit codes. |
| `src/cli.rs` | `Cli` clap struct: `source`, `dry_run`, `keep_temp`, `verbose`. |
| `src/error.rs` | `Failure { Preflight, Git, Severe }`, `Result<T>`, exit-code mapping. |
| `src/git.rs` | `Git` runner + all read-only queries; cwd = top level. |
| `src/rules.rs` | `check-attr` invocation, NUL parsing, index and tree evaluation. |
| `src/temp.rs` | `TempFile` (write + remove on drop) and `RemoveOnDrop` (remove a path on drop). |
| `src/lock.rs` | Exclusive lock file with `create_new`, RAII removal. |
| `src/preflight.rs` | `Ctx` and every precondition, including the history-shape gate. |
| `src/dry_run.rs` | Candidate path union and preview output. |
| `src/flow.rs` | Temp branch, merge, deletion, commit, verification, ff, rollback, tripwire. |
| `tests/common/mod.rs` | `TestRepo` fixture helper. |
| `tests/*.rs` | Integration tests per area (CLI, runner, queries, rules, preflight, dry-run, flow success/failure, hardening). |
| `scripts/check-main.sh` | CI history/tip verification script (M4). |

---

### Task 1: Scaffold, CLI, error type, test harness

**Files:**
- Create: `Cargo.toml`
- Create: `src/lib.rs`
- Create: `src/main.rs`
- Create: `src/cli.rs`
- Create: `src/error.rs`
- Create: `tests/common/mod.rs`
- Test: `tests/cli.rs`

**Interfaces:**
- Produces:
  - `pub struct Cli { pub source: String, pub dry_run: bool, pub keep_temp: bool, pub verbose: bool }`, `Cli: clap::Parser`.
  - `pub enum Failure { Preflight(anyhow::Error), Git(anyhow::Error), Severe(anyhow::Error) }`, `pub type Result<T> = std::result::Result<T, Failure>`, `Failure::code(&self) -> i32`.
  - `tests/common::TestRepo` with `new()`, `path()`, `git(args) -> Output`, `git_ok(args) -> String`, `write(rel, content)`, `commit_all(msg)`, `checkout(branch)`, `checkout_new(branch)`, `rev(rev) -> String`, `bin() -> assert_cmd::Command`.

- [ ] **Step 1: Write the failing CLI tests**

`tests/cli.rs`:

```rust
use assert_cmd::Command;

#[test]
fn help_lists_all_options() {
    let out = Command::cargo_bin("git-merge-safe").unwrap().arg("--help").output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    for flag in ["--source", "--dry-run", "--keep-temp", "--verbose"] {
        assert!(text.contains(flag), "missing {flag} in:\n{text}");
    }
}

#[test]
fn version_prints_cargo_version() {
    let out = Command::cargo_bin("git-merge-safe").unwrap().arg("--version").output().unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains(env!("CARGO_PKG_VERSION")));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test cli`
Expected: compile error, binary/target not found.

- [ ] **Step 3: Create the crate and CLI**

`Cargo.toml`:

```toml
[package]
name = "git-merge-safe"
version = "0.1.0"
edition = "2021"
description = "Squash-merge a branch without bringing excluded paths into the current branch"
license = "MIT"

[dependencies]
clap = { version = "4", features = ["derive"] }
anyhow = "1"

[dev-dependencies]
assert_cmd = "2"
tempfile = "3"
```

`src/cli.rs`:

```rust
use clap::Parser;

#[derive(Debug, Clone, Parser)]
#[command(name = "git-merge-safe", version, about = "Squash-merge a branch without bringing excluded paths into the current branch")]
pub struct Cli {
    #[arg(long, default_value = "dev", value_name = "branch")]
    pub source: String,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub keep_temp: bool,
    #[arg(long)]
    pub verbose: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults() {
        let c = Cli::parse_from(["git-merge-safe"]);
        assert_eq!(c.source, "dev");
        assert!(!c.dry_run && !c.keep_temp && !c.verbose);
    }
}
```

`src/error.rs`:

```rust
use std::fmt;

#[derive(Debug)]
pub enum Failure {
    Preflight(anyhow::Error),
    Git(anyhow::Error),
    Severe(anyhow::Error),
}

pub type Result<T> = std::result::Result<T, Failure>;

impl Failure {
    pub fn code(&self) -> i32 {
        match self {
            Failure::Preflight(_) => 1,
            Failure::Git(_) | Failure::Severe(_) => 2,
        }
    }
    pub fn inner(&self) -> &anyhow::Error {
        match self {
            Failure::Preflight(e) | Failure::Git(e) | Failure::Severe(e) => e,
        }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner())
    }
}
```

`src/lib.rs`:

```rust
pub mod cli;
pub mod error;
```

`src/main.rs`:

```rust
use clap::Parser;
use git_merge_safe::cli::Cli;

fn main() {
    let _cli = Cli::parse();
}
```

- [ ] **Step 4: Run the CLI tests to verify they pass**

Run: `cargo test --test cli`
Expected: PASS (2 tests).

- [ ] **Step 5: Add the error-code unit test**

In `src/error.rs`, add:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exit_codes_match_contract() {
        assert_eq!(Failure::Preflight(anyhow::anyhow!("x")).code(), 1);
        assert_eq!(Failure::Git(anyhow::anyhow!("x")).code(), 2);
        assert_eq!(Failure::Severe(anyhow::anyhow!("x")).code(), 2);
    }
}
```

Run: `cargo test --lib`
Expected: PASS.

- [ ] **Step 6: Create the test harness**

`tests/common/mod.rs`:

```rust
use std::path::{Path, PathBuf};

pub struct TestRepo {
    pub dir: tempfile::TempDir,
}

impl TestRepo {
    pub fn new() -> Self {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = Self { dir };
        repo.git_ok(&["init", "-b", "main"]);
        repo.git_ok(&["config", "user.name", "Test"]);
        repo.git_ok(&["config", "user.email", "test@example.com"]);
        repo
    }

    pub fn path(&self) -> &Path { self.dir.path() }

    pub fn git(&self, args: &[&str]) -> std::process::Output {
        std::process::Command::new("git")
            .current_dir(self.path())
            .args(args)
            .output()
            .unwrap()
    }

    pub fn git_ok(&self, args: &[&str]) -> String {
        let out = self.git(args);
        assert!(
            out.status.success(),
            "git {args:?} failed\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    pub fn write(&self, rel: &str, content: &str) {
        let p: PathBuf = self.path().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    pub fn commit_all(&self, msg: &str) {
        self.git_ok(&["add", "-A"]);
        self.git_ok(&["commit", "-q", "-m", msg]);
    }

    pub fn checkout(&self, branch: &str) { self.git_ok(&["checkout", "-q", branch]); }
    pub fn checkout_new(&self, branch: &str) { self.git_ok(&["checkout", "-q", "-b", branch]); }
    pub fn rev(&self, rev: &str) -> String { self.git_ok(&["rev-parse", rev]) }

    pub fn bin(&self) -> assert_cmd::Command {
        let mut cmd = assert_cmd::Command::cargo_bin("git-merge-safe").unwrap();
        cmd.current_dir(self.path());
        cmd
    }
}
```

Add `tests/harness.rs`:

```rust
mod common;
use common::TestRepo;

#[test]
fn harness_creates_committed_repo() {
    let repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    repo.commit_all("base");
    assert_eq!(repo.rev("HEAD").len(), 40);
    assert!(repo.git_ok(&["status", "--porcelain"]).is_empty());
}
```

- [ ] **Step 7: Run all tests**

Run: `cargo test`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml src tests
git commit -m "feat: scaffold git-merge-safe CLI"
```

---

### Task 2: Git command runner

**Files:**
- Create: `src/git.rs`
- Modify: `src/lib.rs` (`pub mod git;`)
- Test: `tests/git_runner.rs`

**Interfaces:**
- Produces:
  - `pub struct Git { /* top_level, git_dir, verbose */ }`
  - `pub struct CmdOutput { pub status: std::process::ExitStatus, pub stdout: Vec<u8>, pub stderr: Vec<u8> }`
  - `Git::discover(verbose: bool) -> anyhow::Result<Git>`
  - `Git::discover_in(start: &Path, verbose: bool) -> anyhow::Result<Git>`
  - `Git::top_level(&self) -> &Path`, `Git::git_dir(&self) -> &Path`
  - `Git::abs_git_path(&self, rel: &str) -> anyhow::Result<PathBuf>`
  - `Git::run(&self, args: &[&std::ffi::OsStr]) -> anyhow::Result<CmdOutput>`
  - `Git::run_str(&self, args: &[&str]) -> anyhow::Result<CmdOutput>`
  - `Git::run_with_input(&self, args: &[&std::ffi::OsStr], stdin: &[u8], envs: &[(&str, &std::ffi::OsStr)]) -> anyhow::Result<CmdOutput>`
  - `Git::ok(&self, out: CmdOutput) -> anyhow::Result<CmdOutput>`

- [ ] **Step 1: Write the failing runner tests**

`tests/git_runner.rs`:

```rust
mod common;
use common::TestRepo;
use git_merge_safe::git::Git;

#[test]
fn discover_finds_top_level() {
    let repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    repo.commit_all("base");
    let git = Git::discover_in(repo.path(), false).unwrap();
    assert_eq!(git.top_level().canonicalize().unwrap(), repo.path().canonicalize().unwrap());
}
```

Put the remaining runner integration tests in `tests/git_runner.rs` using `TestRepo` and `Git::discover_in`; put the private `render_command` unit test in `src/git.rs` under `#[cfg(test)]`. No global cwd changes.

Tests to write:

- `discover_in` returns the temp repo's canonical top level and git dir.
- `run_str(["rev-parse","HEAD"])` stdout is 40 hex chars + `\n`.
- `ok` on a failing command returns `Err` whose message contains the stderr.
- `run_with_input(["hash-object","--stdin"], b"hello\n", &[])` stdout is `ce013625030ba8dba906f756967f9e9ca394464a`.
- `abs_git_path("merge-safe.lock")` is absolute and ends with `merge-safe.lock`.
- Verbose formatting: unit-test `fn render_command(args: &[&OsStr]) -> String` returns `git <arg> <arg>`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test git_runner`
Expected: compile error (`git_merge_safe` / `Git` missing).

- [ ] **Step 3: Implement the runner**

Implementation points:
- `discover_in(start, verbose)`: run `git rev-parse --git-dir` and `git rev-parse --show-toplevel` with `current_dir(start)`; convert relative results against `start`; store `top_level`, `git_dir` (canonicalize `top_level` only).
- All subsequent commands: `Command::new("git").current_dir(&self.top_level).args(args).output()`.
- `run_with_input`: spawn with `Stdio::piped()` for stdin/stdout/stderr; write stdin, drop it, `wait_with_output()`. Set the given envs on the command.
- Verbose: before running, if verbose, `eprintln!("[git-merge-safe] $ {}", render_command(args))`; after, `eprintln!("[git-merge-safe]   exit {}", code)`.
- `ok`: if `!status.success()`, return an `anyhow!` error containing the stderr (lossy) and exit code.
- `abs_git_path(rel)`: `git rev-parse --git-path <rel>`; if relative, join `top_level`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --test git_runner`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/git.rs src/main.rs tests/git_runner.rs
git commit -m "feat: add git command runner"
```

---

### Task 3: Read-only queries

**Files:**
- Modify: `src/git.rs`
- Test: `tests/git_queries.rs` (or `#[cfg(test)]` in `src/git.rs` if using `discover_in`)

**Interfaces:**
- Produces:
  - `Git::rev_parse(&self, rev: &str) -> anyhow::Result<String>`
  - `Git::current_branch(&self) -> anyhow::Result<Option<String>>`
  - `Git::status_porcelain(&self) -> anyhow::Result<Vec<u8>>`
  - `Git::count_merges(&self, rev: &str) -> anyhow::Result<u64>`
  - `Git::is_ancestor(&self, ancestor: &str, descendant: &str) -> anyhow::Result<bool>`
  - `Git::merge_bases(&self, a: &str, b: &str) -> anyhow::Result<Vec<String>>`
  - `Git::ls_files_z(&self) -> anyhow::Result<Vec<Vec<u8>>>`
  - `Git::ls_files_stage_z(&self) -> anyhow::Result<Vec<(Vec<u8>, String)>>` (path, mode)
  - `Git::ls_tree_z(&self, rev: &str) -> anyhow::Result<Vec<Vec<u8>>>`
  - `Git::show_file(&self, rev: &str, path: &str) -> anyhow::Result<Option<Vec<u8>>>`
  - `Git::diff_cached_is_empty(&self) -> anyhow::Result<bool>`
  - `Git::branch_exists(&self, name: &str) -> anyhow::Result<bool>`
  - `Git::in_progress_operation(&self) -> anyhow::Result<Option<String>>`
  - `Git::info_attributes_present(&self) -> anyhow::Result<bool>`
  - `pub fn parse_git_version(out: &str) -> Option<(u32, u32)>`
  - `pub fn version_supported(v: (u32, u32)) -> bool`
  - `pub fn split_nul(bytes: &[u8]) -> Vec<Vec<u8>>`

- [ ] **Step 1: Write the failing query tests**

Fixtures: a repo with `base` on main; `dev` with `docs/a.md`, changed `AGENTS.md`, `.gitattributes`; a conflicting change on main; an untracked file as needed.

Assertions:

- `rev_parse("HEAD")` equals `git rev-parse HEAD` and is 40 chars.
- `current_branch()` is `Some("main")`; after `checkout --detach HEAD`, it is `None`.
- `status_porcelain()` is empty after a commit; after writing `untracked.txt`, it starts with `?? `.
- `count_merges("main")` is 0 for a linear branch; after `git merge --no-ff`, it is 1.
- `is_ancestor("dev", "main")` is false; after `merge --ff-only dev`, true.
- `merge_bases("main", "dev")` has exactly 1 element; unrelated branches give 0.
- `ls_files_z()` contains `b"AGENTS.md"` and `b".gitattributes"`.
- `ls_files_stage_z()` returns mode `"100644"` for `AGENTS.md`; for an injected gitlink, mode `"160000"`.
- `ls_tree_z("dev")` contains `b"docs/a.md"`.
- `show_file("dev", ".gitattributes")` is `Some(bytes)` containing `merge=ours`; `show_file("dev", "missing")` is `None`.
- `diff_cached_is_empty()` is true after commit; false after `git add` of a change.
- `branch_exists("dev")` true, `branch_exists("nope")` false.
- `in_progress_operation()` is `None`; after writing `<git-path MERGE_HEAD>` with a valid SHA it is `Some("MERGE_HEAD")`.
- `info_attributes_present()` false; after writing `<git-path info/attributes>` with `* merge=ours\n`, true.
- `parse_git_version("git version 2.39.5 (Apple Git-154)") == Some((2,39))`; `version_supported((2,30))` true, `(2,29)` false.
- `split_nul(b"a\0b\0") == vec![b"a", b"b"]`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test git_queries`
Expected: compile error.

- [ ] **Step 3: Implement the queries**

Pinned commands:

- `current_branch`: `symbolic-ref --short HEAD`; non-zero → `None` unless stderr indicates a real error (treat exit 1 as detached).
- `status_porcelain`: `status --porcelain=v1 -z --untracked-files=normal`.
- `count_merges`: `rev-list --count --merges <rev>`.
- `is_ancestor`: `merge-base --is-ancestor a b`; exit 0 → true, 1 → false, other → error.
- `merge_bases`: `merge-base --all a b`; split lines; exit 1 → empty vec.
- `ls_files_z`: `ls-files -z`.
- `ls_files_stage_z`: `ls-files -s -z`; parse `<mode> <sha> <stage>\t<path>\0`.
- `ls_tree_z`: `ls-tree -r -z --name-only <rev>`.
- `show_file`: `show <rev>:<path>`; non-zero → `None` (this also covers missing paths).
- `diff_cached_is_empty`: `diff --cached --quiet`; 0 → true, 1 → false, other → error.
- `branch_exists`: `show-ref --verify --quiet refs/heads/<name>`; exit 0 → true, 1 → false.
- `in_progress_operation`: `rev-parse -q --verify MERGE_HEAD`, `CHERRY_PICK_HEAD`, `REVERT_HEAD`, `REBASE_HEAD`; check `abs_git_path("rebase-apply")` and `"rebase-merge"` are directories. Return the first name found.
- `info_attributes_present`: `abs_git_path("info/attributes")`; true when metadata succeeds and `len() > 0`.
- `parse_git_version`: split whitespace, take the third token, split `.`, parse first two.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --test git_queries`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/git.rs tests/git_queries.rs
git commit -m "feat: add read-only git queries"
```

---

### Task 4: Attribute evaluation

**Files:**
- Create: `src/rules.rs`
- Create: `src/temp.rs`
- Modify: `src/lib.rs` (`pub mod rules; pub mod temp;`)
- Test: `tests/rules.rs`

**Interfaces:**
- Produces:
  - `pub struct TempFile { /* path */ }`, `TempFile::new(dir: &Path, name: &str, contents: &[u8]) -> anyhow::Result<TempFile>`, `path(&self) -> &Path`; removes the file on drop.
  - `pub struct RemoveOnDrop { /* path */ }`, `RemoveOnDrop::new(path: PathBuf) -> Self`, `path(&self) -> &Path`; removes on drop.
  - `pub fn parse_check_attr_z(out: &[u8]) -> anyhow::Result<Vec<(Vec<u8>, String)>>`
  - `pub fn excluded_in_index(git: &Git, paths: &[Vec<u8>]) -> anyhow::Result<Vec<Vec<u8>>>`
  - `pub fn excluded_in_tree(git: &Git, rev: &str, paths: &[Vec<u8>]) -> anyhow::Result<Vec<Vec<u8>>>`

- [ ] **Step 1: Write the failing tests**

`tests/rules.rs` (integration via the library helper) plus a unit test in `src/rules.rs`:

- Parser unit:
  - input `b"a\0merge\0ours\0b\0merge\0unset\0c\0merge\0unspecified\0"` → `[(b"a","ours"), (b"b","unset"), (b"c","unspecified")]`.
  - malformed trailing bytes → `Err`.
- Integration fixture with the full example:

```
docs/**            merge=ours
AGENTS.md          merge=ours
CLAUDE.md          merge=ours
.claude/**         merge=ours
.comet/**          merge=ours
.superpowers/**    merge=ours
.impeccable/**     merge=ours
docs/index.html    -merge
```

Files: `docs/a.md`, `docs/sub/b.md`, `docs/index.html`, `AGENTS.md`, `CLAUDE.md`, `plain.txt`.
Expected `excluded_in_index` over all `ls_files_z()`: exactly `AGENTS.md`, `CLAUDE.md`, `docs/a.md`, `docs/sub/b.md`; not `docs/index.html`, not `plain.txt`, not `.gitattributes`.

- Nested rules: add `docs/.gitattributes` with `*.md merge=ours`; `docs/a.md` excluded even without the root rule.
- Tree evaluation: `excluded_in_tree("HEAD", &[b"AGENTS.md", b"plain.txt"])` returns `AGENTS.md` only, even though both paths are in the index.
- Global-attribute isolation: `git config core.attributesFile <file>` where `<file>` contains `* merge=ours\n`; `excluded_in_index` must still return only rule-marked paths.
- Empty input returns an empty vec without running a command (assert no panic).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test rules`
Expected: compile error.

- [ ] **Step 3: Implement the evaluator**

Command construction, used for both index and tree:

```text
git -c core.attributesFile=<git-path merge-safe/no-attributes>
    check-attr --cached --stdin -z merge --
```

- env: `GIT_ATTR_NOSYSTEM=1`; plus `GIT_INDEX_FILE=<temp>` for tree evaluation.
- stdin: the candidate paths joined by NUL with a trailing NUL.
- Parse triples; keep paths whose value is exactly `ours`.
- `excluded_in_tree`: create a `RemoveOnDrop` at `<git-path merge-safe/dry-<pid>-<n>>`, run `git read-tree <rev>` with `GIT_INDEX_FILE`, then evaluate, then drop.
- `TempFile::new` creates the parent directory, writes the file, returns the guard.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --test rules`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/rules.rs src/temp.rs src/main.rs tests/rules.rs
git commit -m "feat: evaluate merge=ours attributes via git check-attr"
```

---

### Task 5: Preflight checks and lock

**Files:**
- Create: `src/lock.rs`
- Create: `src/preflight.rs`
- Modify: `src/lib.rs` (`pub mod lock; pub mod preflight;`)
- Modify: `src/main.rs`
- Test: `tests/preflight.rs`
- Test: `tests/lock.rs`

**Interfaces:**
- Produces:
  - `pub struct Lock { /* path */ }`, `Lock::acquire(path: PathBuf) -> anyhow::Result<Lock>`, `path(&self) -> &Path`; removes on drop.
  - `pub struct Ctx { pub git: Git, pub cur: String, pub src: String, pub head0: String, pub src_head0: String }`
  - `pub fn check(cli: &Cli, dry_run: bool) -> crate::error::Result<(Ctx, Option<Lock>)>`

- [ ] **Step 1: Write the failing tests**

`tests/lock.rs`:

- `acquire` creates the file; dropping removes it.
- acquiring the same path twice returns `Err` mentioning the path.

`tests/preflight.rs` (run the binary; temporarily `git merge-safe` only runs preflight and exits 0):

- valid repo → `--dry-run` exits 0, no lock file left.
- detached HEAD → exit 1, stderr contains `detached`.
- `cur == src` → exit 1.
- missing source → exit 1.
- in-progress merge (`<git-path MERGE_HEAD>` holds a valid SHA) → exit 1.
- dirty worktree (untracked file) → exit 1.
- current branch has a real merge commit (`git merge --no-ff`) → exit 1, message mentions merge commit.
- source is an ancestor (`main` fast-forwarded to `dev`) → exit 1.
- unrelated histories → exit 1.
- `<git-path info/attributes>` non-empty → exit 1.
- lock file pre-created → exit 1, message contains the path.
- `--dry-run` with a dirty worktree AND a pre-created lock → exit 0 (both skipped).
- `--dry-run` with `<git-path info/attributes>` non-empty → exit 1 (not skipped).
- Git version parser: unit tests `parse_git_version` / `version_supported` (from Task 3) plus a preflight error path if the probe reports `< 2.30` (test the `version_supported` function, not the binary).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test preflight --test lock`
Expected: compile error / wrong exit codes.

- [ ] **Step 3: Implement lock and preflight**

`src/lock.rs`: `OpenOptions::new().write(true).create_new(true)`; on `AlreadyExists`, return an error containing the path and the recovery hint; write `pid` + newline; `Drop` removes.

`src/preflight.rs` order (each failure wrapped as `Failure::Preflight` with a Chinese message):

1. `Git::discover(cli.verbose)`.
2. `git --version` → `parse_git_version`; require `version_supported`.
3. `current_branch()` → `Some(cur)`, else "detached HEAD".
4. `cur != src`.
5. `rev_parse("refs/heads/<src>^{commit}")` (local branch only).
6. `in_progress_operation()` is `None`.
7. If `!dry_run`: `status_porcelain()` empty.
8. History shape: `count_merges(cur) == 0`; `!is_ancestor(src, cur)`; `merge_bases(cur, src).len() == 1`.
9. If `!dry_run`: acquire lock at `abs_git_path("merge-safe.lock")`.
10. `!info_attributes_present()`.
11. `head0 = rev_parse("HEAD")`, `src_head0 = rev_parse(src)`.

`src/main.rs`: parse CLI → `preflight::check(&cli, cli.dry_run)` → on Ok, exit 0 for now; on Err, print `[git-merge-safe] <message>` to stderr and exit `failure.code()`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --test preflight --test lock`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/lock.rs src/preflight.rs src/main.rs tests/preflight.rs tests/lock.rs
git commit -m "feat: add preflight checks and exclusive lock"
```

---

### Task 6: Dry-run

**Files:**
- Create: `src/dry_run.rs`
- Modify: `src/lib.rs` (`pub mod dry_run;`)
- Modify: `src/main.rs`
- Test: `tests/dry_run.rs`

**Interfaces:**
- Produces:
  - `pub fn run(ctx: &Ctx, cli: &Cli) -> crate::error::Result<()>`
  - `pub fn candidate_paths(git: &Git, cur: &str, src: &str) -> anyhow::Result<Vec<Vec<u8>>>`

- [ ] **Step 1: Write the failing tests**

- Fixture: full example rules; main has `docs/a.md`, `AGENTS.md`; dev adds `docs/sub/b.md`, `CLAUDE.md`, modifies `docs/index.html`.
  - Run `--dry-run`:
    - stdout contains `docs/a.md`, `docs/sub/b.md`, `AGENTS.md`, `CLAUDE.md`.
    - stdout does not contain `docs/index.html` or `plain.txt`.
    - `git status --porcelain` empty; `git ls-files -s` identical before/after; `HEAD` identical; `git branch --list 'merge-safe/*'` empty.
  - Exit code 0.
- Dirty worktree + pre-created lock → `--dry-run` exits 0 and still prints paths.
- Repo with no `merge=ours` rules → stdout contains `未配置排除规则`.
- Candidate union includes a path that exists only on cur and one only on src.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test dry_run`
Expected: no output / wrong exit.

- [ ] **Step 3: Implement dry-run**

- `candidate_paths`: union of `ls_tree_z(cur)` and `ls_tree_z(src)`, sorted, deduplicated.
- Evaluate the candidate list against both trees with `excluded_in_tree`, union the results, sort, dedupe.
- Print `[git-merge-safe] 将移除: <path>` per path (UTF-8 lossy), then `[git-merge-safe] 共 N 个路径（预览为超集，实际以合并结果为准）`; if N == 0 and no rules produced any hit, print `[git-merge-safe] 未配置排除规则`.
- No writes, no branch, no lock.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --test dry_run`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/dry_run.rs src/main.rs tests/dry_run.rs
git commit -m "feat: add dry-run preview"
```

---

### Task 7: Core successful flow

**Files:**
- Create: `src/flow.rs`
- Create: `src/report.rs`
- Modify: `src/lib.rs` (`pub mod flow; pub mod report;`)
- Modify: `src/main.rs` (dispatch: dry-run → `dry_run::run`, else `flow::run`)
- Test: `tests/flow_success.rs`

**Interfaces:**
- Produces:
  - `pub fn run(ctx: Ctx, cli: &Cli, lock: Lock) -> crate::error::Result<()>`
  - `fn unique_temp_branch(git: &Git) -> anyhow::Result<String>`
  - `fn removal_set(git: &Git) -> anyhow::Result<Vec<Vec<u8>>>` (index paths minus gitlinks check)
  - `fn rollback(git: &Git, cur: &str, head0: &str, tmp: &str) -> anyhow::Result<()>`
- `report::info(msg)` / `report::warn(msg)` print with the `[git-merge-safe]` prefix.

- [ ] **Step 1: Write the failing success tests**

`tests/flow_success.rs` cases:

1. **Basic exclusion**
   - main: `.gitattributes` with `docs/** merge=ours` / `AGENTS.md merge=ours`; `plain.txt`; `AGENTS.md` v0. dev: adds `docs/a.md`, changes `AGENTS.md` to v-dev, changes `plain.txt`.
   - Run: exit 0; `docs` absent; `AGENTS.md` absent; `plain.txt` == `dev-change\n`; `git status --porcelain` empty; no `merge-safe/*` branch; `rev-parse dev` unchanged; main commit count +1.
2. **Both branches modified the excluded path** → exit 0, path absent, no conflict output on stderr.
3. **Excluded path exists on main, dev never touches it** (`main-only.txt` marked in `.gitattributes`, added on main) → absent after the run.
4. **`-merge` exception**: `docs/** merge=ours`, `docs/index.html -merge`; dev changes `docs/index.html`; after run file exists with dev content.
5. **All changes excluded**: dev adds only `docs/a.md` → exit 0, stdout contains `无内容可提交`, main HEAD unchanged.
6. **Repeat run**: run the binary twice; second run exits 0, main HEAD unchanged after the first run's commit.
7. **No rules**: merge proceeds, exit 0, normal files merged.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test flow_success`
Expected: exit 0 but nothing changed → assertions fail.

- [ ] **Step 3: Implement the flow**

Pinned sequence:

1. `tmp = unique_temp_branch(git)` → `merge-safe/tmp-<pid>-<nanos-hex>`; regenerate until `branch_exists` is false.
2. `git switch -c <tmp>`; on error → `Failure::Git`.
3. Write `git show <src>:.gitattributes` (if present) to `TempFile` at `<git-path merge-safe/src-attrs-<pid>>`; if absent, use the nonexistent `<git-path merge-safe/no-attributes>`.
4. Merge: `git -c merge.ours.driver=true -c core.attributesFile=<attrs> merge --squash <src>` with env `GIT_ATTR_NOSYSTEM=1`.
   - Non-zero → print captured git output, `rollback`, return `Failure::Git`.
5. `removal_set`: `ls_files_z` → `excluded_in_index`; if any excluded path has mode `160000` in `ls_files_stage_z`, return an error (rollback).
6. If non-empty: write the NUL-joined paths to a `TempFile` at `<git-path merge-safe/delete-<pid>>`; run `git rm -q --cached --ignore-unmatch --pathspec-from-file=<file> --pathspec-file-nul`.
7. Delete worktree files: for each path, `fs::remove_file(top_level.join(os_from_git_path(p)))`; `NotFound` ignored; any other error → rollback + `Failure::Git`. Then prune empty parent directories bottom-up, stopping at `top_level`.
8. `diff_cached_is_empty()`: if true → `rollback`, print `无内容可提交`, return Ok(()) (exit 0).
9. `git commit -q -m "merge <src> (squash) without excluded paths"`; non-zero → rollback + `Failure::Git`.
10. Post-commit verification: `ls_tree_z(tmp)` → `excluded_in_tree(tmp, paths)` must be empty; `status_porcelain()` must be empty. Otherwise rollback + `Failure::Git` (message mentions verification).
11. Source tripwire: `rev_parse(src) == src_head0`, else `Failure::Severe` (keep `tmp`, do not delete it).
12. `git switch <cur>`; `git merge --ff-only <tmp>`; on error → keep `tmp`, print its name and manual instructions, return `Failure::Git`.
13. Source tripwire again.
14. If `!keep_temp`: `git branch -D <tmp>` (ignore failure with a warning).
15. Final `status_porcelain` empty; non-empty → warning only.
16. Print summary: removed count and paths, new HEAD, `工作区干净；源分支 <src> 未变动`.

`rollback(git, cur, head0, tmp)`:
- If `current_branch() == Some(tmp)`: `reset --hard <head0>`, then `switch <cur>`.
- `branch -D <tmp>` ignoring failure.
- Any failure inside rollback → return an error with the manual recovery steps.

Path bytes helper (put in `src/git.rs`):
```rust
#[cfg(unix)]
pub fn os_from_git_path(p: &[u8]) -> std::ffi::OsString {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(p).to_os_string()
}
#[cfg(not(unix))]
pub fn os_from_git_path(p: &[u8]) -> std::ffi::OsString {
    String::from_utf8_lossy(p).into_owned().into()
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --test flow_success`
Expected: PASS.

- [ ] **Step 5: Run the full suite**

Run: `cargo test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/flow.rs src/report.rs src/main.rs src/git.rs tests/flow_success.rs
git commit -m "feat: implement squash merge with full path exclusion"
```

---

### Task 8: Failure paths and guards

**Files:**
- Modify: `src/flow.rs`
- Test: `tests/flow_failure.rs`

**Interfaces:**
- Consumes all Task 7 interfaces; no new public API required.
- Produces internal `fn verify_source(expected: &str, actual: &str) -> anyhow::Result<()>`.

- [ ] **Step 1: Write the failing failure-path tests**

1. **Non-excluded conflict**: both branches modify `plain.txt` differently → exit 2; `rev-parse HEAD`, `ls-files -s`, `status --porcelain` identical before/after; no `merge-safe/*` branch; stderr contains `CONFLICT`.
2. **`.gitattributes` conflict**: both branches change the rules differently → exit 2, same unchanged-state assertions.
3. **Gitlink**: add `plugin` as a gitlink (`git update-index --add --cacheinfo 160000,<HEAD>,plugin`) on main, rule `plugin merge=ours` → exit 2; stderr mentions `子模块`; rollback complete.
4. **Hook re-adds excluded path**: `pre-commit` hook creates `docs/a.md` and `git add -f`s it → exit 2; main HEAD unchanged; no `merge-safe/*` branch; stderr mentions verification.
5. **Hook leaves an untracked file**: `pre-commit` hook writes `junk.txt` (not staged) → exit 2; main's HEAD and index unchanged; no `merge-safe/*`; the tool's message mentions the dirty worktree.
6. **Fast-forward race**: `post-commit` hook creates a new commit on `refs/heads/main` (`git commit-tree -p refs/heads/main -m race refs/heads/main^{tree}` + `git update-ref`), then the tool's ff must fail → exit 2; `git branch --list 'merge-safe/*'` non-empty; stderr contains the temp branch name.
7. **Source tripwire**: unit test `verify_source("a","b")` is `Err`; end-to-end assert `rev-parse dev` identical after a success run and after a conflict run.

Gate tests 4–6 with `#[cfg(unix)]` (they rely on shell hooks).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test flow_failure`
Expected: failures/wrong exit codes.

- [ ] **Step 3: Implement the guards**

- Wrap every git failure after branch creation with `rollback` before returning.
- `gitlink` detection: in `removal_set`, any excluded path with mode `160000` → `anyhow::bail!("命中子模块路径，未做任何修改")`.
- Post-commit verification failure message: `提交后校验失败：排除路径或未跟踪文件仍存在，已回滚`.
- ff failure: do not call rollback; keep `tmp`; message `fast-forward 失败，已保留临时分支 <tmp>；main 未被本工具修改`.
- `verify_source`: compare strings; on mismatch return an error mentioning the source branch.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --test flow_failure`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/flow.rs tests/flow_failure.rs
git commit -m "feat: roll back cleanly on conflicts, hooks and races"
```

---

### Task 9: Hardening matrix

**Files:**
- Test: `tests/hardening.rs`
- Modify: `src/flow.rs` / `src/git.rs` only if a test exposes a defect.

- [ ] **Step 1: Write the tests**

1. **Spaces**: `dir/space name.txt` added on dev under `dir/** merge=ours` → removed.
2. **Quoted pattern**: `.gitattributes` line `"space name.txt" merge=ours`; file added on dev → removed.
3. **Chinese**: `docs/文档.md` under `docs/** merge=ours` → removed.
4. **Newline (unix only)**: `dir/line\nbreak.txt` under `dir/** merge=ours` → removed.
5. **Symlink (unix only)**: dev adds a symlink `link` marked `merge=ours` → removed, target untouched.
6. **Full example rules** (the eight-line block from the spec) with `docs/index.html -merge` → all marked paths gone, `docs/index.html` present.
7. **New rule only on dev**: main lacks `AGENTS.md merge=ours`; dev adds the rule and both modify `AGENTS.md` → exit 0, no conflict, path removed.
8. **Run from a subdirectory**: tracked `sub/keep.txt`; run the binary with `current_dir(repo/sub)` → success, same result.
9. **Source in another worktree**: create a separate `tempfile::TempDir` `wt`; `git worktree add <wt>/dev-wt dev`; run on main → success; `git -C <wt>/dev-wt rev-parse HEAD` and `git -C <wt>/dev-wt status --porcelain` unchanged.
10. **Global attributes ignored end-to-end**: `git config core.attributesFile <file>` with `* merge=ours\n`; run → only rule-marked paths removed; `plain.txt` remains.
11. **`--keep-temp`**: branch `merge-safe/*` exists after success; default run leaves none.
12. **Checkout dev after success**: `git checkout dev` exits 0.
13. **`--verbose`**: stderr contains the merge command (`merge --squash`).
14. **Existing lock stale message**: pre-create `<git-path merge-safe.lock>`; exit 1; stderr contains the path.

- [ ] **Step 2: Run the tests**

Run: `cargo test --test hardening`
Expected: all PASS. If any fails, fix the owning code path, not the test.

- [ ] **Step 3: Commit**

```bash
git add tests/hardening.rs src
git commit -m "test: cover special paths, worktrees and attribute isolation"
```

---

### Task 10: CI verification script, README alignment, final pass

**Files:**
- Create: `scripts/check-main.sh`
- Create: `scripts/tests/test-check-main.sh`
- Modify: `README.md` (only where actual behavior differs)
- Test: shell test script

**Interfaces:**
- `scripts/check-main.sh [branch]` (default `main`): exits 0 when no path in the branch's tip tree or history evaluates to `merge=ours`; otherwise prints offending paths and exits 1.

- [ ] **Step 1: Write the failing shell test**

`scripts/tests/test-check-main.sh` creates temp repos with `set -e` and asserts:

- repo whose main never contained `docs/**` rules → exit 0.
- repo with `docs/a.md` in main's history and `docs/** merge=ours` → exit 1, output contains `docs/a.md`.
- same repo with `docs/index.html -merge` and only `docs/index.html` in history → exit 0.
- repo where `AGENTS.md` was removed from the tip but exists in history → exit 1.

Run: `bash scripts/tests/test-check-main.sh`
Expected: FAIL (script missing).

- [ ] **Step 2: Implement the script**

```sh
#!/bin/sh
set -eu
branch="${1:-main}"
cd "$(git rev-parse --show-toplevel)"
noattrs="$(git rev-parse --git-path merge-safe/no-attributes)"
check() {
  GIT_ATTR_NOSYSTEM=1 git -c core.attributesFile="$noattrs" \
    check-attr --cached --stdin -z merge --
}
tip_paths=$(git ls-tree -r --name-only "$branch")
hist_paths=$(git log --name-only --format= "$branch" | sort -u)
offenders=$(
  { printf '%s\n' "$tip_paths"; printf '%s\n' "$hist_paths"; } |
  awk 'NF' | sort -u |
  while IFS= read -r p; do
    printf '%s\0' "$p"
  done | check | tr '\0' '\n' | paste - - - 2>/dev/null | awk '$3=="ours"{print $1}' || true
)
if [ -n "$offenders" ]; then
  echo "[check-main] 发现禁止路径出现在 $branch 的树或历史中:" >&2
  printf '%s\n' "$offenders" >&2
  exit 1
fi
echo "[check-main] $branch 未发现 merge=ours 路径"
```

(Adjust the NUL/paste pipeline during implementation if it is brittle; the required behavior is the test.)

- [ ] **Step 3: Run the shell test**

Run: `bash scripts/tests/test-check-main.sh`
Expected: PASS.

- [ ] **Step 4: Align README**

Diff the README against the implemented CLI and behavior. Update only mismatches (flag names, exit codes, lock path, messages). Do not add requirements or roadmap sections.

- [ ] **Step 5: Final verification**

Run:

```bash
cargo fmt --check
cargo test
cargo build --release
bash scripts/tests/test-check-main.sh
```

Expected: all pass. If `cargo fmt --check` reports differences, run `cargo fmt` and re-run.

- [ ] **Step 6: Commit**

```bash
git add scripts README.md
git commit -m "feat: add CI history check and align README"
```

---


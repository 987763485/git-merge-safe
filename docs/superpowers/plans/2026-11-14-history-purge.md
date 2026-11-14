# History Purge Script Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver a reviewed script and runbook that rewrite the target branch's history to remove every path marked `merge=ours`, while leaving the source branch byte-for-byte untouched; the team executes it, not the tool.

**Architecture:** A POSIX shell script with three modes — check, print-command, execute. It never rewrites refs itself; it validates preconditions, generates the exact `git filter-repo` invocation (including a filename callback for `-merge` exceptions), runs it in a fresh mirror clone, verifies the result against a backup mirror, and optionally force-pushes only the target branch.

**Tech Stack:** POSIX `sh`, `git`, `git-filter-repo` (execute mode), `python3` (execute mode with `-merge` exceptions). Tests are a shell script driving fixture repositories.

**Spec:** `docs/superpowers/specs/2026-11-14-git-merge-safe-design.md` (§6)

## Global Constraints

- The source branch is read-only: the script never checks it out, never rewrites it, never pushes it. After every mode, `git rev-parse <source>` must equal its value before the script ran.
- Only `refs/heads/<main>` (and explicitly named tags via `--tag`) may be rewritten or pushed. Never `--all`, never `--mirror`.
- Any precondition failure stops before writing anything, with a message naming the failed check.
- `-merge` paths (for example `docs/index.html`) are **not** excluded; they must survive the rewrite.
- The runbook (not the script) covers platform-side remnants (PR refs, cached objects).
- Exit codes: 0 success, 1 precondition or verification failure.

## Review Focus

1. `-merge` exception paths inside an excluded directory — preserved through the rewrite. Test: Task 1 `--print-command` includes the callback and the kept path; Task 2 verification fails if a kept path disappears.
2. Commits containing excluded paths that are also reachable from the source branch — refuse before rewriting. Test: Task 1 shared-commit fixture.
3. A real merge commit on the target branch — refuse. Test: Task 1.
4. Tags pointing into rewritten history — either rewritten explicitly or reported. Test: Task 1 `--print-command` includes `--refs refs/tags/<tag>` when `--tag` is passed.
5. Failure after a no-op rewrite — verification must catch it and refuse to push. Test: Task 2 stub-filter-repo case.

## File Structure

| File | Responsibility |
|---|---|
| `scripts/purge-main-history.sh` | Argument parsing, precondition checks, command generation, execute/verify/push orchestration. |
| `scripts/tests/test-purge.sh` | Shell tests with fixture repositories and a stub `git-filter-repo`. |
| `docs/history-purge.md` | Operator runbook: preconditions, backup, commands, verification, team reset, caveats. |

---

### Task 1: Check mode and command generation

**Files:**
- Create: `scripts/purge-main-history.sh`
- Create: `scripts/tests/test-purge.sh`

**Interfaces:**
- Produces CLI:
  ```
  purge-main-history.sh [--repo <path>] [--main <branch>] [--source <branch>]
    --exclude <path>... [--keep <path>...] [--tag <name>...]
    [--check | --print-command | --execute] [--push]
  ```
  Defaults: `--repo .`, `--main main`, `--source dev`, mode `--check`.
- `--check` exits 0 when every precondition passes, 1 otherwise.
- `--print-command` prints the checks, the `git filter-repo` argv (shell-quoted), the verification commands, and the push command; exits 0 only when checks pass.

- [ ] **Step 1: Write the failing check/print tests**

`scripts/tests/test-purge.sh` (bash, `set -eu`):

```bash
#!/usr/bin/env bash
set -eu
here="$(cd "$(dirname "$0")/.." && pwd)"
script="$here/purge-main-history.sh"
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT

make_repo() { # $1=dir  $2=shared_path(yes/no)
  git init -q -b main "$1"
  git -C "$1" config user.name T; git -C "$1" config user.email t@example.com
  mkdir -p "$1/docs"
  if [ "$2" = yes ]; then echo shared > "$1/docs/shared.md"; fi
  echo base > "$1/plain.txt"; echo "docs/** merge=ours" > "$1/.gitattributes"
  git -C "$1" add -A; git -C "$1" commit -qm base
  git -C "$1" checkout -qb dev
  echo dev > "$1/dev.txt"; git -C "$1" add -A; git -C "$1" commit -qm dev
  git -C "$1" checkout -q main
  echo a > "$1/docs/a.md"; echo agents > "$1/AGENTS.md"
  git -C "$1" add -A; git -C "$1" commit -qm "squash 1"
}
expect_ok() { "$@" >"$tmp/out" 2>&1 || { echo "expected success: $*"; cat "$tmp/out"; exit 1; }; }
expect_fail() { if "$@" >"$tmp/out" 2>&1; then echo "expected failure: $*"; cat "$tmp/out"; exit 1; fi; }

# valid repo passes
make_repo "$tmp/ok" no
expect_ok sh "$script" --repo "$tmp/ok" --exclude docs --exclude AGENTS.md --check

# print-command contains the pinned flags and paths
sh "$script" --repo "$tmp/ok" --exclude docs --exclude AGENTS.md --print-command >"$tmp/plan"
grep -q -- '--refs main' "$tmp/plan"
grep -q -- '--invert-paths' "$tmp/plan"
grep -q -- '--path docs' "$tmp/plan"
grep -q -- '--path AGENTS.md' "$tmp/plan"
grep -q -- 'push --force origin refs/heads/main' "$tmp/plan"
if grep -q -- '--all\|--mirror' "$tmp/plan"; then echo "--all/--mirror must not appear"; exit 1; fi

# shared commit containing an excluded path -> refuse
make_repo "$tmp/shared" yes
expect_fail sh "$script" --repo "$tmp/shared" --exclude docs --exclude AGENTS.md --check

# real merge commit on main -> refuse
make_repo "$tmp/merged" no
git -C "$tmp/merged" checkout -q -b side main
echo side > "$tmp/merged/side.txt"; git -C "$tmp/merged" add -A; git -C "$tmp/merged" commit -qm side
git -C "$tmp/merged" checkout -q main
git -C "$tmp/merged" merge -q --no-ff side -m merge
expect_fail sh "$script" --repo "$tmp/merged" --exclude docs --exclude AGENTS.md --check

# source is an ancestor -> refuse
make_repo "$tmp/ff" no
git -C "$tmp/ff" merge -q --ff-only dev
expect_fail sh "$script" --repo "$tmp/ff" --exclude docs --exclude AGENTS.md --check

# -merge exception and tags appear in the generated plan
make_repo "$tmp/keep" no
git -C "$tmp/keep" tag v1
sh "$script" --repo "$tmp/keep" --exclude docs --exclude AGENTS.md \
  --keep docs/index.html --tag v1 --print-command >"$tmp/keep-plan"
grep -q -- '--filename-callback' "$tmp/keep-plan"
grep -q -- 'docs/index.html' "$tmp/keep-plan"
grep -q -- 'refs/tags/v1' "$tmp/keep-plan"

echo "task 1 tests passed"
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `bash scripts/tests/test-purge.sh`
Expected: FAIL (`scripts/purge-main-history.sh: not found`).

- [ ] **Step 3: Implement argument parsing, checks and command generation**

Implementation points:

- Parse flags with a `while [ $# -gt 0 ]` loop; collect repeatable `--exclude`, `--keep`, `--tag` into newline-separated variables.
- Resolve `--repo` to an absolute path; run every git command as `git -C "$repo"`.
- Checks, each printing `FAIL: <reason>` and exiting 1 on failure:
  1. `git rev-parse --git-dir`, `refs/heads/<main>`, `refs/heads/<source>` resolve.
  2. `main != source`.
  3. `git rev-list --count --merges <main>` == 0.
  4. `git merge-base --is-ancestor <source> <main>` exits non-zero.
  5. `git merge-base --all <main> <source>` prints exactly one SHA; call it `base`.
  6. `git ls-tree -r --name-only "$base"` contains none of the `--exclude` paths (match exact path or `<path>/` prefix).
  7. For each `--exclude` path, every commit from `git rev-list <main> -- <path>` is not an ancestor of `<source>` (`git merge-base --is-ancestor <sha> <source>` must exit non-zero).
- `--print-command` output sections:
  - `checks: ok`
  - `rewrite: git filter-repo --refs <main> [--refs refs/tags/<tag>...] --invert-paths --path <exclude>...` when `--keep` is empty.
  - With `--keep`: `--filename-callback '<body>'` and a JSON dump of excluded and kept paths so the operator can review them; the body reads `GMS_EXCLUDED` / `GMS_KEPT` files.
  - `verify: ...` (the four checks from Task 2).
  - `push: git -C <filtered> push --force origin refs/heads/<main>:refs/heads/<main>` plus one line per tag.
- `--execute` is not implemented in this task; print `not implemented` and exit 1 if selected.

- [ ] **Step 4: Run the test to verify it passes**

Run: `bash scripts/tests/test-purge.sh`
Expected: `task 1 tests passed`.

- [ ] **Step 5: Commit**

```bash
git add scripts/purge-main-history.sh scripts/tests/test-purge.sh
git commit -m "feat: add history purge checks and command generation"
```

---

### Task 2: Execute, verify, push, and the runbook

**Files:**
- Modify: `scripts/purge-main-history.sh`
- Modify: `scripts/tests/test-purge.sh` (add execute cases)
- Create: `docs/history-purge.md`

**Interfaces:**
- Consumes Task 1's modes and variables.
- Produces `--execute` (with optional `--push`) that:
  1. requires `git filter-repo` on `PATH`;
  2. requires `python3` when `--keep` is non-empty;
  3. creates a temporary directory with `backup.git` and `filtered.git` mirror clones;
  4. runs the generated rewrite inside `filtered.git`;
  5. verifies; on failure prints the reason, keeps both directories, exits 1, never pushes;
  6. on success prints both directory paths, the verification summary, and (with `--push`) pushes only `<main>` and named tags.

- [ ] **Step 1: Write the failing execute tests**

Append to `scripts/tests/test-purge.sh`:

```bash
# execute with a no-op filter-repo must fail verification and not push
make_repo "$tmp/noop" no
mkdir -p "$tmp/stub"
cat > "$tmp/stub/git-filter-repo" <<'EOF'
#!/bin/sh
exit 0
EOF
chmod +x "$tmp/stub/git-filter-repo"
src_before="$(git -C "$tmp/noop" rev-parse dev)"
if PATH="$tmp/stub:$PATH" sh "$script" --repo "$tmp/noop" \
    --exclude docs --exclude AGENTS.md --execute --push >"$tmp/exec" 2>&1; then
  echo "expected verification failure"; cat "$tmp/exec"; exit 1
fi
grep -q 'FAIL' "$tmp/exec"
grep -q 'not pushed\|未推送' "$tmp/exec"
[ "$(git -C "$tmp/noop" rev-parse dev)" = "$src_before" ]
```

Note: this test asserts the verification gate, not a real rewrite. If `git filter-repo` is installed, also add an optional end-to-end case that runs the real tool on a fixture and asserts `git -C filtered.git log main -- docs AGENTS.md` is empty and `docs/index.html` is preserved; skip it when `command -v git-filter-repo` fails.

- [ ] **Step 2: Run the test to verify it fails**

Run: `bash scripts/tests/test-purge.sh`
Expected: FAIL (execute prints `not implemented`).

- [ ] **Step 3: Implement execute and verification**

Implementation points:

- `mktemp -d` a work dir; `backup.git` and `filtered.git` via `git clone --mirror "$repo"`.
- Record `source_before="$(git -C "$repo" rev-parse "$source")"` and `base_before="$(git -C "$repo" merge-base "$main" "$source")"`.
- Write the callback JSON files when `--keep` is non-empty:
  - `excluded.json` / `kept.json` via `printf '%s\n'` piped to `python3 -c 'import json,sys; print(json.dumps(sys.stdin.read().splitlines()))'`.
  - Callback body (passed as one argv via `--filename-callback`):
    ```python
    import json, os
    excluded = json.load(open(os.environ['GMS_EXCLUDED']))
    kept = set(json.load(open(os.environ['GMS_KEPT'])))
    name = filename.decode('utf-8', 'surrogateescape')
    if name in kept:
        return filename
    for e in excluded:
        if name == e or name.startswith(e + '/'):
            return None
    return filename
    ```
  - Export `GMS_EXCLUDED` / `GMS_KEPT`.
  - Before relying on the callback, confirm the body semantics against `git filter-repo --help` for the installed version: the body receives `filename` as bytes and returning `None` removes the path.
- Run inside `filtered.git`:
  - no `--keep`: `git filter-repo --refs <main> [--refs refs/tags/<tag>...] --invert-paths --path <exclude>...`
  - with `--keep`: `git filter-repo --refs <main> [...] --filename-callback "<body>"` (no `--invert-paths`, since the callback returns `None` for removals).
- Verification (all must pass):
  1. `git -C filtered.git log --oneline <main> -- <exclude...>` is empty.
  2. `git -C filtered.git rev-parse <source>` equals `source_before`.
  3. `git -C filtered.git merge-base <main> <source>` equals `base_before`.
  4. Tip content: sorted `git -C filtered.git ls-tree -r --name-only <main>` equals the backup's `git -C backup.git ls-tree -r --name-only <main>` with excluded prefixes removed. (Keep exceptions remain because they are not excluded.)
  5. Each `--keep` path present on the original tip is present on the rewritten tip.
- On any failure: print `FAIL: <check>`, `未推送`, print both mirror paths, exit 1.
- On success with `--push`: `git -C filtered.git push --force origin refs/heads/<main>:refs/heads/<main>`; for each `--tag`, `git -C filtered.git push --force origin refs/tags/<tag>:refs/tags/<tag>`. Never `--all`/`--mirror`.
- Print the team reset block:
  ```
  git fetch --all --prune
  git checkout <main> && git reset --hard origin/<main>
  ```
  and repeat that the source branch was not touched.

- [ ] **Step 4: Write the runbook**

`docs/history-purge.md` sections:

1. Scope: what is removed, what is preserved (`-merge`), what is out of scope (source branch, platform caches).
2. Preconditions and how to run `--check`.
3. Backup: the script's mirror backup path, plus an independent copy step.
4. Review: `--print-command`, including the excluded/kept JSON and the callback.
5. Execute and verify; what each verification proves.
6. Push: only `<main>` and named tags; branch protection must be lifted temporarily.
7. Team reset commands; branches based on the old main must be rebased.
8. Caveats: PR refs and remote object caches; bypass merges reintroduce old commits into main's ancestry; tags not named via `--tag` still point at old commits.

- [ ] **Step 5: Run the tests**

Run: `bash scripts/tests/test-purge.sh`
Expected: all cases pass.

- [ ] **Step 6: Commit**

```bash
git add scripts/purge-main-history.sh scripts/tests/test-purge.sh docs/history-purge.md
git commit -m "feat: add history purge execution, verification and runbook"
```

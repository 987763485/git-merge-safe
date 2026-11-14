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
git -C "$tmp/ff" reset -q --hard dev
expect_fail sh "$script" --repo "$tmp/ff" --exclude docs --exclude AGENTS.md --check

# -merge exception and tags appear in the generated plan
make_repo "$tmp/keep" no
git -C "$tmp/keep" tag v1
sh "$script" --repo "$tmp/keep" --exclude docs --exclude AGENTS.md \
  --keep docs/index.html --tag v1 --print-command >"$tmp/keep-plan"
grep -q -- '--filename-callback' "$tmp/keep-plan"
grep -q -- 'docs/index.html' "$tmp/keep-plan"
grep -q -- 'refs/tags/v1' "$tmp/keep-plan"

# execute with a no-op filter-repo must fail verification and not push
make_repo "$tmp/noop" no
mkdir -p "$tmp/stub"
cat > "$tmp/stub/git-filter-repo" <<'EOF'
#!/bin/sh
exit 0
EOF
chmod +x "$tmp/stub/git-filter-repo"
src_before="$(git -C "$tmp/noop" rev-parse dev)"
main_before="$(git -C "$tmp/noop" rev-parse main)"
if PATH="$tmp/stub:$PATH" sh "$script" --repo "$tmp/noop" \
    --exclude docs --exclude AGENTS.md --execute --push >"$tmp/exec" 2>&1; then
  echo "expected verification failure"; cat "$tmp/exec"; exit 1
fi
grep -q 'FAIL' "$tmp/exec"
grep -q 'not pushed\|未推送' "$tmp/exec"
[ "$(git -C "$tmp/noop" rev-parse dev)" = "$src_before" ]
[ "$(git -C "$tmp/noop" rev-parse main)" = "$main_before" ]

# optional real end-to-end when git-filter-repo is installed
if command -v git-filter-repo >/dev/null 2>&1; then
  make_repo "$tmp/real" no
  sh "$script" --repo "$tmp/real" --exclude docs --exclude AGENTS.md \
    --execute >"$tmp/real-out" 2>&1 \
    || { echo "real filter-repo execute failed"; cat "$tmp/real-out"; exit 1; }
  grep -q 'verification passed' "$tmp/real-out"
fi

echo "purge tests passed"

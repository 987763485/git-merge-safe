#!/usr/bin/env bash
set -eu
here="$(cd "$(dirname "$0")/.." && pwd)"
script="$here/check-main.sh"
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT

mk() {
  git init -q -b main "$1"
  git -C "$1" config user.name T
  git -C "$1" config user.email t@example.com
}

# pass: rules exist, no matching path in tree or history
mk "$tmp/pass"
echo "docs/** merge=ours" > "$tmp/pass/.gitattributes"
echo p > "$tmp/pass/plain.txt"
git -C "$tmp/pass" add -A; git -C "$tmp/pass" commit -qm base
(cd "$tmp/pass" && sh "$script" main) >/dev/null

# fail: docs/a.md in history but removed from the tip
mk "$tmp/hist"
echo "docs/** merge=ours" > "$tmp/hist/.gitattributes"
mkdir -p "$tmp/hist/docs"; echo a > "$tmp/hist/docs/a.md"
git -C "$tmp/hist" add -A; git -C "$tmp/hist" commit -qm add
git -C "$tmp/hist" rm -q docs/a.md; git -C "$tmp/hist" commit -qm rm
if (cd "$tmp/hist" && sh "$script" main) >"$tmp/out" 2>&1; then
  echo "expected failure for history-only path"; cat "$tmp/out"; exit 1
fi
grep -q 'docs/a.md' "$tmp/out"

# pass: -merge exception is not an offender
mk "$tmp/kept"
printf 'docs/** merge=ours\ndocs/index.html -merge\n' > "$tmp/kept/.gitattributes"
mkdir -p "$tmp/kept/docs"; echo i > "$tmp/kept/docs/index.html"
git -C "$tmp/kept" add -A; git -C "$tmp/kept" commit -qm base
(cd "$tmp/kept" && sh "$script" main) >/dev/null

# fail: AGENTS.md removed from the tip but present in history
mk "$tmp/agents"
echo "AGENTS.md merge=ours" > "$tmp/agents/.gitattributes"
echo a > "$tmp/agents/AGENTS.md"
git -C "$tmp/agents" add -A; git -C "$tmp/agents" commit -qm add
git -C "$tmp/agents" rm -q AGENTS.md; git -C "$tmp/agents" commit -qm rm
if (cd "$tmp/agents" && sh "$script" main) >"$tmp/out2" 2>&1; then
  echo "expected failure for history-only AGENTS.md"; cat "$tmp/out2"; exit 1
fi
grep -q 'AGENTS.md' "$tmp/out2"

echo "check-main tests passed"

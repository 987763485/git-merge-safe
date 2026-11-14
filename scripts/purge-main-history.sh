#!/bin/sh
set -eu

usage() {
  cat <<'EOF'
usage: purge-main-history.sh [--repo <path>] [--main <branch>] [--source <branch>]
  --exclude <path>... [--keep <path>...] [--tag <name>...]
  [--check | --print-command | --execute] [--push]

  --check          verify every precondition, change nothing (default)
  --print-command  print the rewrite/verify/push commands, change nothing
  --execute        run the rewrite in mirror clones (requires git-filter-repo)
  --push           with --execute, force-push the main branch and named tags only
  --keep <path>    a path inside an excluded directory that must survive
EOF
}

repo=.
main=main
source=dev
mode=check
push=0
NL='
'
excludes=""
keeps=""
tags=""

die() { echo "FAIL: $*" >&2; exit 1; }

while [ $# -gt 0 ]; do
  case "$1" in
    --repo) repo=$2; shift 2 ;;
    --main) main=$2; shift 2 ;;
    --source) source=$2; shift 2 ;;
    --exclude) excludes="${excludes}$2${NL}"; shift 2 ;;
    --keep) keeps="${keeps}$2${NL}"; shift 2 ;;
    --tag) tags="${tags}$2${NL}"; shift 2 ;;
    --check) mode=check; shift ;;
    --print-command) mode=print; shift ;;
    --execute) mode=execute; shift ;;
    --push) push=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

[ -n "$excludes" ] || die "at least one --exclude path is required"

repo=$(cd "$repo" 2>/dev/null && pwd) || die "repository path not found"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM
printf '%s' "$excludes" > "$tmp/excludes"
printf '%s' "$keeps" > "$tmp/keeps"
printf '%s' "$tags" > "$tmp/tags"

# --- preconditions -----------------------------------------------------------

git -C "$repo" rev-parse --git-dir >/dev/null 2>&1 || die "not a git repository: $repo"
git -C "$repo" rev-parse --verify --quiet "refs/heads/$main^{commit}" >/dev/null \
  || die "main branch not found: $main"
git -C "$repo" rev-parse --verify --quiet "refs/heads/$source^{commit}" >/dev/null \
  || die "source branch not found: $source"

[ "$main" != "$source" ] || die "main and source are the same branch: $main"

merges=$(git -C "$repo" rev-list --count --merges "$main")
[ "$merges" -eq 0 ] || die "$main contains $merges merge commit(s); refusing"

if git -C "$repo" merge-base --is-ancestor "$source" "$main"; then
  die "$source is an ancestor of $main; refusing"
else
  rc=$?
  [ "$rc" -eq 1 ] || die "git merge-base failed while checking ancestry"
fi

bases=$(git -C "$repo" merge-base --all "$main" "$source") \
  || die "no merge base between $main and $source"
base_count=$(printf '%s\n' "$bases" | awk 'NF { n++ } END { print n + 0 }')
[ "$base_count" -eq 1 ] || die "expected exactly one merge base, found $base_count"
base=$bases
git -C "$repo" ls-tree -r --name-only "$base" > "$tmp/base-tree"

while IFS= read -r p; do
  [ -n "$p" ] || continue
  if awk -v p="$p" '$0 == p || index($0, p "/") == 1 { found = 1 } END { exit !found }' \
      "$tmp/base-tree"; then
    die "excluded path exists in the merge base: $p"
  fi
done < "$tmp/excludes"

while IFS= read -r p; do
  [ -n "$p" ] || continue
  for sha in $(git -C "$repo" rev-list "$main" -- "$p"); do
    if git -C "$repo" merge-base --is-ancestor "$sha" "$source"; then
      die "commit $sha touching $p is reachable from $source; refusing"
    fi
  done
done < "$tmp/excludes"

# --- command generation ------------------------------------------------------

rewrite="git filter-repo --refs $main"
while IFS= read -r t; do
  [ -n "$t" ] || continue
  rewrite="$rewrite --refs refs/tags/$t"
done < "$tmp/tags"

if [ -z "$keeps" ]; then
  while IFS= read -r p; do
    [ -n "$p" ] || continue
    rewrite="$rewrite --path $p"
  done < "$tmp/excludes"
  rewrite="$rewrite --invert-paths"
else
  callback='import json, os
excluded = json.load(open(os.environ["GMS_EXCLUDED"]))
kept = set(json.load(open(os.environ["GMS_KEPT"])))
name = filename.decode("utf-8", "surrogateescape")
if name in kept:
    return filename
for e in excluded:
    if name == e or name.startswith(e + "/"):
        return None
return filename'
  rewrite="$rewrite --filename-callback '$callback'"
fi

if [ "$mode" = execute ]; then
  echo "FAIL: --execute is not implemented yet" >&2
  exit 1
fi

echo "checks: ok"
if [ "$mode" = print ]; then
  echo "rewrite: $rewrite"
  if [ -n "$keeps" ]; then
    echo "excluded paths:"
    while IFS= read -r p; do
      if [ -n "$p" ]; then echo "  $p"; fi
    done < "$tmp/excludes"
    echo "kept paths:"
    while IFS= read -r p; do
      if [ -n "$p" ]; then echo "  $p"; fi
    done < "$tmp/keeps"
  fi
  echo "verify:"
  echo "  the rewritten $main contains no excluded path in any commit"
  echo "  $source resolves to the same commit before and after"
  echo "  the merge base of $main and $source is unchanged"
  echo "  the rewritten tip has exactly the backup tip's paths minus excluded paths"
  echo "  every kept path still exists"
  echo "push:"
  echo "  git -C <filtered> push --force origin refs/heads/$main:refs/heads/$main"
  while IFS= read -r t; do
    if [ -n "$t" ]; then
      echo "  git -C <filtered> push --force origin refs/tags/$t:refs/tags/$t"
    fi
  done < "$tmp/tags"
fi

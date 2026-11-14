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

if [ "$push" = 1 ] && [ "$mode" != execute ]; then
  die "--push requires --execute"
fi

# --- execute ----------------------------------------------------------------

if [ "$mode" = execute ]; then
  command -v git-filter-repo >/dev/null 2>&1 || die "git-filter-repo is required for --execute"
  if [ -n "$keeps" ]; then
    command -v python3 >/dev/null 2>&1 || die "python3 is required when --keep is used"
    python3 -c 'import json,sys; print(json.dumps([l for l in open(sys.argv[1]).read().splitlines() if l]))' "$tmp/excludes" > "$tmp/excluded.json"
    python3 -c 'import json,sys; print(json.dumps([l for l in open(sys.argv[1]).read().splitlines() if l]))' "$tmp/keeps" > "$tmp/kept.json"
    export GMS_EXCLUDED="$tmp/excluded.json"
    export GMS_KEPT="$tmp/kept.json"
  fi

  source_before=$(git -C "$repo" rev-parse "$source")
  work=$(mktemp -d)
  backup="$work/backup.git"
  filtered="$work/filtered.git"

  fail_verify() {
    echo "FAIL: $*" >&2
    echo "mirrors kept: $backup $filtered" >&2
    echo "未推送" >&2
    exit 1
  }

  git clone --mirror "$repo" "$backup" >/dev/null 2>&1 || die "backup clone failed"
  git clone --mirror "$repo" "$filtered" >/dev/null 2>&1 || die "filtered clone failed"

  (
    cd "$filtered" || exit 1
    set -- filter-repo --refs "$main"
    while IFS= read -r t; do
      [ -n "$t" ] || continue
      set -- "$@" --refs "refs/tags/$t"
    done < "$tmp/tags"
    if [ -n "$keeps" ]; then
      set -- "$@" --filename-callback "$callback"
    else
      set -- "$@" --invert-paths
      while IFS= read -r p; do
        [ -n "$p" ] || continue
        set -- "$@" --path "$p"
      done < "$tmp/excludes"
    fi
    git "$@"
  ) > "$work/filter-repo.log" 2>&1 || {
    tail -n 20 "$work/filter-repo.log" >&2
    fail_verify "git filter-repo failed"
  }

  while IFS= read -r p; do
    [ -n "$p" ] || continue
    if [ -n "$(git -C "$filtered" rev-list "$main" -- "$p")" ]; then
      fail_verify "excluded path still present in history: $p"
    fi
  done < "$tmp/excludes"

  git -C "$filtered" rev-parse --verify --quiet "refs/heads/$source" >/dev/null \
    || fail_verify "source branch missing in filtered clone"
  [ "$(git -C "$filtered" rev-parse "$source")" = "$source_before" ] \
    || fail_verify "source branch moved"

  filtered_base=$(git -C "$filtered" merge-base "$main" "$source") \
    || fail_verify "merge base missing in filtered clone"
  [ "$filtered_base" = "$base" ] || fail_verify "merge base changed"

  git -C "$backup" ls-tree -r --name-only "$main" > "$work/orig.txt"
  git -C "$filtered" ls-tree -r --name-only "$main" > "$work/new.txt"
  awk -v exf="$tmp/excludes" -v kef="$tmp/keeps" '
    BEGIN {
      while ((getline line < exf) > 0) if (line != "") ex[++n] = line
      while ((getline line < kef) > 0) if (line != "") keep[line] = 1
    }
    {
      p = $0
      if (p in keep) { print; next }
      for (i = 1; i <= n; i++) {
        e = ex[i]
        if (p == e || index(p, e "/") == 1) next
      }
      print
    }' "$work/orig.txt" | sort > "$work/expected.txt"
  sort "$work/new.txt" > "$work/actual.txt"
  cmp -s "$work/expected.txt" "$work/actual.txt" \
    || fail_verify "rewritten tip differs from expected"

  while IFS= read -r k; do
    [ -n "$k" ] || continue
    if grep -Fxq "$k" "$work/orig.txt"; then
      grep -Fxq "$k" "$work/actual.txt" || fail_verify "kept path missing: $k"
    fi
  done < "$tmp/keeps"

  echo "verification passed"
  echo "backup: $backup"
  echo "filtered: $filtered"
  if [ "$push" = 1 ]; then
    git -C "$filtered" push --force origin "refs/heads/$main:refs/heads/$main" \
      || die "push failed; mirrors kept at $backup $filtered"
    while IFS= read -r t; do
      [ -n "$t" ] || continue
      git -C "$filtered" push --force origin "refs/tags/$t:refs/tags/$t" \
        || die "tag push failed: $t; mirrors kept at $backup $filtered"
    done < "$tmp/tags"
    echo "pushed: $main and named tags"
  else
    echo "not pushed (review the mirrors, then push explicitly)"
  fi
  echo "team reset:"
  echo "  git fetch --all --prune"
  echo "  git checkout $main && git reset --hard origin/$main"
  echo "source branch $source was not touched"
  exit 0
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

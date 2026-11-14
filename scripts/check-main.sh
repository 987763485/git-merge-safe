#!/bin/sh
# CI helper: fail when any path in the branch's tip tree or history is marked
# `merge=ours`. Rules are read from the current index; machine-local attribute
# files are neutralized. Paths containing newlines are not supported by this
# line-oriented helper (the tool itself handles them).
set -eu

branch="${1:-main}"
root=$(git rev-parse --show-toplevel)
cd "$root"

git rev-parse --verify --quiet "refs/heads/$branch^{commit}" >/dev/null \
  || { echo "[check-main] unknown branch: $branch" >&2; exit 2; }

info=$(git rev-parse --git-path info/attributes)
if [ -s "$info" ]; then
  echo "[check-main] 检测到 .git/info/attributes；无法给出确定性结论" >&2
  exit 2
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

# Raw (unquoted) paths so non-ASCII and spaces compare against .gitattributes.
{
  git -c core.quotePath=false ls-tree -r --name-only "$branch"
  git -c core.quotePath=false log --name-only --format= "$branch"
} | awk 'NF' | sort -u > "$tmp/paths"

noattrs=$(git rev-parse --git-path merge-safe/no-attributes)
while IFS= read -r p; do
  if [ -n "$p" ]; then printf '%s\0' "$p"; fi
done < "$tmp/paths" |
  GIT_ATTR_NOSYSTEM=1 git -c core.attributesFile="$noattrs" \
    check-attr --cached --stdin -z merge -- > "$tmp/attrs" || {
      echo "[check-main] check-attr 失败" >&2
      exit 2
    }

tr '\0' '\n' < "$tmp/attrs" > "$tmp/attrs.txt"
offenders=$(
  while IFS= read -r path && IFS= read -r attr && IFS= read -r value; do
    if [ "$value" = "ours" ]; then printf '%s\n' "$path"; fi
  done < "$tmp/attrs.txt"
)

if [ -n "$offenders" ]; then
  echo "[check-main] 发现禁止路径出现在 $branch 的树或历史中:" >&2
  printf '%s\n' "$offenders" >&2
  exit 1
fi

echo "[check-main] $branch 未发现 merge=ours 路径"

# 清理 main 历史操作手册

本手册用于一次性重写目标分支（默认 `main`）的历史，使 `merge=ours` 标记的路径在该分支的任何提交里都查不到，同时**完全不改动 squash 的来源分支**（默认 `dev`）。

执行者：团队运维/维护者。脚本：`scripts/purge-main-history.sh`。

## 1. 范围

- **会被移除**：`merge=ours` 标记的路径，例如 `docs/`、`AGENTS.md`、`CLAUDE.md`、`.cualde/`、`.comet/`、`.superpowers/`、`.impeccable/`。清单由命令行的 `--exclude` 显式给出，脚本不会自行解析 `.gitattributes`。
- **会被保留**：`-merge` 取消标记的例外路径，例如 `docs/index.html`，通过 `--keep` 给出（精确文件路径）。
- **不处理**：`dev` 的历史与内容、远端平台的 PR 引用与缓存对象、未通过 `--tag` 指定的标签。

## 2. 前置条件与检查

依赖：`git`；`--execute` 需要 `git-filter-repo`；`--keep` 需要 `python3`。

在**目标仓库的镜像克隆**上操作（不要在主工作区直接跑）：

```bash
git clone --mirror <remote-url> work.git
```

先跑检查，不做任何修改：

```bash
sh scripts/purge-main-history.sh --repo work.git \
  --exclude docs --exclude AGENTS.md --exclude CLAUDE.md \
  --exclude .cualde --exclude .comet --exclude .superpowers --exclude .impeccable \
  --keep docs/index.html --check
```

检查项（任一失败即退出，码 1）：

1. `--repo` 是 Git 仓库，`main` 与 `dev` 都是本地分支，且不是同一个分支。
2. `main` 没有真正的合并提交（只接受线性历史）。
3. `dev` 不是 `main` 的祖先。
4. `main` 与 `dev` 恰好有一个共同起点。
5. 共同起点的树里没有 `--exclude` 路径。
6. 触及任一 `--exclude` 路径的 `main` 提交都不在 `dev` 上。

第 5、6 条是「不影响 dev」的硬门槛：只要有一个包含禁止路径的提交与 dev 共享，就停止并人工评估。

## 3. 备份

脚本在 `--execute` 时会建立一个 `backup.git` 镜像克隆并打印路径。除此之外，再独立留一份：

```bash
git clone --mirror <remote-url> backup-<date>.git
```

保留到清理结果被团队确认之后再删除。

## 4. 审阅计划

```bash
sh scripts/purge-main-history.sh --repo work.git --exclude ... --keep docs/index.html --print-command
```

输出包含：

- 完整的 `git filter-repo` 命令（`--refs main`、按需的 `--refs refs/tags/<tag>`、`--invert-paths --path ...`，或 `--keep` 时的 `--filename-callback`）；
- 排除路径与保留路径的清单；
- 验证步骤与推送命令。

`--keep` 模式使用一个 Python 回调：回调按 `GMS_EXCLUDED` / `GMS_KEPT` 两个 JSON 文件判定，命中排除路径返回 `None`（移除），命中保留路径原样返回。执行前用 `git filter-repo --help` 确认所装版本的 `--filename-callback` 语义（`filename` 为 bytes，返回 `None` 表示移除）。

## 5. 执行与验证

```bash
sh scripts/purge-main-history.sh --repo work.git --exclude ... --keep docs/index.html \
  --tag v1.2.0 --execute
```

脚本会：

1. 从 `work.git` 克隆出 `backup.git` 与 `filtered.git`；
2. 在 `filtered.git` 里运行 `git filter-repo`；
3. 逐项验证，全部通过才打印 `verification passed`，否则打印 `FAIL`、保留两个镜像并退出（绝不会推送）。

验证项及含义：

| 验证 | 证明什么 |
|---|---|
| `rev-list main -- <exclude>` 为空 | 目标分支的任何提交里都没有禁止路径 |
| `rev-parse dev` 与执行前相同 | 来源分支一个字节都没动 |
| `merge-base main dev` 与执行前相同 | 共同起点未变，后续 squash 合并仍可用 |
| 新 tip 的路径 = 备份 tip 的路径去掉排除项、保留 `--keep` 项 | 除禁止路径外内容完全一致 |
| 每个原本存在的 `--keep` 路径仍存在 | `-merge` 例外没有被误删 |

失败时两个镜像保留在打印的路径下，供排查；修复后重新执行。

## 6. 推送

脚本的 `--push` 会把 `filtered.git` 推回它的来源（即 `work.git` 这个本地镜像）。真正发布到远端，再执行：

```bash
git -C work.git push --force origin refs/heads/main:refs/heads/main
git -C work.git push --force origin refs/tags/v1.2.0:refs/tags/v1.2.0   # 仅 --tag 指定的
```

规则：

- 只推 `main` 和通过 `--tag` 明确列出的标签。
- **绝不要** `--all` 或 `--mirror`。
- 推送前临时放开分支保护，推完恢复。
- 未通过 `--tag` 指定的标签仍指向旧提交，需要单独决定保留/删除/重指。

## 7. 团队动作

每位同事：

```bash
git fetch --all --prune
git checkout main && git reset --hard origin/main
```

- 从旧 `main` 拉出的本地分支需要 rebase。
- `dev` 不需要任何动作；它的提交与工作区都没有被改动。

## 8. 已知边界

- 远端平台的 PR 引用、缓存对象可能需要平台侧配合才能彻底清除；本手册保证的是分支历史（`git log main -- <路径>`）为空。
- 清理之后，任何绕过 `git-merge-safe` 的 `git merge dev`（非 squash）都会把旧提交带回 `main` 的祖先；请配合 CI 检查。
- 重新引入的提交只能再次清理；脚本的 `--check` 会重新拒绝共享提交。
- `--exclude` / `--keep` 的路径清单以换行分隔，不支持含换行的路径。
- 脚本只重写 `main`（以及显式 `--tag` 的标签）；不要用其他工具对 `dev` 做同类操作。

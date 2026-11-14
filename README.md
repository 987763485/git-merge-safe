# git-merge-safe

在 squash 合并时按 `.gitattributes` 规则排除指定路径的 Git 子命令。

## 简介

`git-merge-safe` 把一个分支（默认 `dev`）squash 合并到当前分支，同时确保 `.gitattributes` 中标记为 `merge=ours` 的路径不会进入当前分支。

它补上了 Git 原生行为的一个缺口：`git merge --squash` 不支持按路径排除。`git-merge-safe` 把合并过程放在临时分支上完成，最后只对当前分支做 fast-forward；任何失败都不会改动当前分支。

适用场景：

- 开发分支上有只服务于开发的目录或文件（文档、内部说明、工具配置），它们不应出现在发布分支上。
- 排除规则需要集中维护、可评审、可版本管理。
- 合并失败时当前分支必须保持原样，能够安全重试。

## 特性

- 一条命令完成 squash 合并与路径排除。
- 规则来自 `.gitattributes`，使用标准 `merge=ours` 属性；`-merge` 可为单个文件开例外。
- 排除是完整的：命中路径不会出现在当前分支的提交、索引和工作区里；当前分支上原本存在的也会被移除。
- 临时分支隔离：当前分支在最后一步之前完全不动，失败自动回滚。
- 不修改 git config、不 push、不打 tag、不改变源分支。
- 跨平台：Linux、macOS、Windows。

## 环境要求

- Git 2.30 或更高版本。
- 从源码构建时需要稳定版 Rust 工具链。

## 安装

从源码构建：

```bash
git clone <repository-url>
cd git-merge-safe
cargo build --release
install -m 0755 target/release/git-merge-safe /usr/local/bin/
```

Windows 下将 `target\release\git-merge-safe.exe` 所在目录加入 `PATH`。

确认安装：

```bash
git merge-safe --version
git merge-safe --help
```

Git 会自动把 `git merge-safe` 解析为 `PATH` 中的 `git-merge-safe`，无需额外配置别名。

## 快速开始

在源分支（默认 `dev`）维护 `.gitattributes`：

```
docs/**            merge=ours
AGENTS.md          merge=ours
CLAUDE.md          merge=ours
.cualde/**         merge=ours
.comet/**          merge=ours
.superpowers/**    merge=ours
.impeccable/**     merge=ours
docs/index.html    -merge
```

切换到目标分支，先预览：

```bash
git checkout main
git merge-safe --dry-run
```

执行合并：

```bash
git merge-safe
```

```
[git-merge-safe] 环境检查通过
[git-merge-safe] 合并 dev 成功（临时分支，未改动 main）
[git-merge-safe] 已从索引与工作区移除：docs/a.md, docs/b.md, AGENTS.md
[git-merge-safe] 保留（-merge）：docs/index.html
[git-merge-safe] 已 fast-forward main 到 abc1234
[git-merge-safe] 工作区干净；源分支 dev 未变动
```

## 规则配置

`merge=ours`：禁止该路径从源分支进入当前分支。合并后它不会留在当前分支的索引和工作区里；如果它原本就在当前分支上，也会被移除。

`-merge`：在禁止范围内为单个文件开例外，让它正常参与合并。

规则由 Git 自己解释，`.gitattributes` 的所有标准语义都适用：目录通配、子目录中的 `.gitattributes`、含空格路径的引号写法等。工具不维护第二套匹配规则。

`merge=ours` 同时也是 Git 的 keep-ours 合并驱动名。`git-merge-safe` 在自己的合并命令上临时注入 `merge.ours.driver=true`，不需要在仓库或全局配置该驱动，也不会修改任何配置。`-merge` 的路径按普通文件合并；如果它在两边都被修改，会像普通文件一样产生冲突，工具整体回滚。

判定只使用版本化的 `.gitattributes`。系统级和用户级属性文件会被屏蔽；如果仓库中存在 `.git/info/attributes`，工具会拒绝运行，因为它会改变判定结果且无法被忽略。

## 命令参考

```
git merge-safe [选项]

选项：
  --source <branch>   源分支，默认 dev
  --dry-run           预览将被移除的路径，不做任何修改
  --keep-temp         成功后保留临时分支，便于排查
  --verbose           输出每条 Git 命令及其退出码
  -h, --help          显示帮助
  -V, --version       显示版本
```

### 运行前的检查

工具在执行前确认：

- 当前在 Git 仓库中，且处于某个分支（不是 detached HEAD）。
- 当前分支不是源分支。
- 源分支存在且是本地分支。
- 没有进行中的合并、变基或拣选。
- 工作区完全干净（包括没有未跟踪文件；被忽略的文件不影响）。
- 当前分支的历史是线性的，源分支不是它的祖先，两者恰好有一个共同起点。
- 没有其他 `git-merge-safe` 实例在运行（排他锁）。
- 仓库中没有 `.git/info/attributes`。

任何一项不满足都会立即退出，不做修改。

## 行为说明

1. 从当前 HEAD 创建临时分支。
2. 在临时分支上执行 squash 合并；源分支的规则作为补充属性源，使本次新增的规则同样生效。
3. 枚举合并后的索引，移除所有命中 `merge=ours` 的路径，索引与工作区文件一并处理。
4. 在临时分支上提交，并校验提交结果。
5. 切回当前分支，执行 fast-forward，删除临时分支。
6. 输出摘要。

整个过程里，当前分支的 HEAD、索引和工作区在最后一步之前都不变；源分支始终只读。工具不推送、不打 tag、不修改 git config。

## 退出码

| 退出码 | 含义 |
|---|---|
| 0 | 成功，或没有可提交的内容 |
| 1 | 前置检查不通过（工作区不干净、环境形态不符、锁被占用等） |
| 2 | Git 操作失败（合并冲突、fast-forward 失败等）；当前分支已回滚，或按要求保留临时分支 |

## 中断恢复

如果进程被强制终止，可能留下临时分支、半合并的索引或锁文件。按以下步骤恢复：

```bash
git branch --list 'merge-safe/*'
git switch <your-branch>        # 若失败，先执行 git reset --hard
git reset --hard
git branch -D merge-safe/tmp-*
rm -f "$(git rev-parse --git-path merge-safe.lock)"
```

运行 `git status` 确认工作区与预期一致后再继续。

## 限制

- 不支持对子模块（gitlink）路径做排除；命中时会报错并回滚。
- 子目录中的 `.gitattributes` 新规则不参与本次合并前的冲突判定（合并后的判定仍然正确）。
- 钩子照常执行；如果钩子把命中路径加回提交或在工作区留下文件，本次运行会回滚。
- 进程被 SIGKILL 时无法自动回滚，见「中断恢复」。

## 参与贡献

欢迎提交 issue 和 pull request。提交前请确保现有测试通过，并为行为变更补充测试。

## 许可证

MIT License，详见 [LICENSE](LICENSE)。

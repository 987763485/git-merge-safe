# git-merge-safe 设计文档

- 日期：2026-11-14
- 状态：待批准
- 输入：`PRD.md`（2026-11-14 版）与随后的讨论
- 验证环境：git 2.39.5（Apple Git-154），macOS

## 1. 意图与为什么

`dev` 上正常提交着 `docs/` 与 `AGENTS.md`，它们只服务于开发，不能出现在 `main` 上。
要求的最终状态比"不进入"更强：**main 的任何提交里都不能有这些路径**，而 dev 完全不受影响。

因此工作拆成两件相互独立的事：

1. **一次性清理**：重写 main 的历史，让这些路径在 main 的任何提交里都查不到；dev 一个字都不动。
2. **以后不再脏**：一个 Rust 命令行工具 `git-merge-safe`，每次把 dev squash 合进 main 时，保证新提交里没有这些路径，并且 main 的工作区保持干净、工具可以反复运行。

清理是一次性操作，要 force push、要全组配合，不该放进日常命令里；工具只负责日常合并。清理以**脚本 + 操作手册**交付，由团队在真实仓库上执行，工具开发不依赖它先执行。两者按此顺序执行：先清理，再上工具。

## 2. 最终行为（一次成功运行）

在 main 上执行 `git merge-safe`：

- 把 dev 的改动 squash 合并进 main，命名规则之外的路径全部保留；
- 所有命中 `.gitattributes` 中 `merge=ours` 的路径**不出现在 main 的新提交、索引和工作区里**——包括这次 dev 新增的、dev 改过的，以及本来就在 main 上、dev 根本没碰的；
- 合并期间所有工作都在临时分支上完成，main 的 HEAD、索引、工作区在最后一刻之前完全不动；
- 最后 main 只做一次 fast-forward；
- 成功后工作区干净，可以立即再跑一次，也可以正常 `git checkout dev`；
- 任何失败（包括非排除路径上的冲突）都让 main 回到执行前的状态，临时分支被删除。

## 3. 目标 / 非目标 / 反目标

**目标**

- 一次命令完成 squash 合并，且 main 的历史、索引、工作区都不含命中规则的路径。
- 规则沿用 `.gitattributes` 中已有的 `merge=ours` 约定（与 `merge.ours.driver` 兼容）：它表示禁止该路径从源分支进入当前分支，`-merge` 为单个文件撤销禁止、正常合并；新增或修改规则不需要改工具。
- 运行前识别环境是否符合「当前分支的历史全部由源分支 squash 而来」的结构条件，不符合就拒绝运行。
- **源分支绝对不受影响**：任何操作都不改变源分支的 ref、reflog 指向、索引与工作区；运行前后 `git rev-parse <src>` 必须一致。
- 失败路径下 main 的 HEAD、索引、工作区与执行前完全一致。
- 成功后无临时分支、无残留文件、无锁文件。
- 一次性清理 main 历史后，dev 的历史、内容、合并关系都不变。
- Windows / macOS / Linux 行为一致。

**非目标**

- 不改 dev、不推送、不在日常合并里重写历史。
- 不实现 Git 钩子；不处理 `.gitignore` 语义。
- 不支持对子模块（gitlink）路径做排除。
- 不做"只撤出索引、文件留着"的语义——那是旧 PRD 的做法，已被替换。
- 不自动解决排除路径之外的任何冲突。

**反目标（技术上跑通也算失败）**

- 静默吞掉非排除路径上的冲突。
- 删除命中规则之外的文件。
- 把 main 或工作区留在需要用户手工收拾的状态。
- 让用户在合并后需要手动 `git rm --cached`、`git clean` 或删除残留目录。
- 让排除路径以任何形式"暂时"进入 main 的历史。
- 任何操作改变源分支的 ref、reflog、索引或工作区。

## 4. 已验证的 Git 行为（本设计的地基）

以下均在 git 2.39.5 上实测，不是推测：

| 行为 | 实测结果 | 对设计的影响 |
|---|---|---|
| `merge --squash` 更新工作区与索引 | 是，干净合并后两边都变 | 排除必须同时处理索引和工作区 |
| `restore --staged` 对不存在的 pathspec | exit 1 | 不能把规则 glob 直接交给 Git |
| `merge --abort` 对 squash 合并 | `fatal: no merge to abort`，exit 128 | 回滚只能用 `reset --hard`（在临时分支上安全） |
| `merge=ours` 与 `-merge` 的判定 | `check-attr` 对前者返回 `ours`，对后者返回 `unset`；`docs/**` 之后写 `docs/index.html -merge` 能精确取消该文件 | 判定只认 `ours`；`unset` 与 `unspecified` 都保留 |
| `git -c merge.ours.driver=true merge` | 双方都改、add/add 都保留"ours"且零冲突；`-merge` 的路径照常合并 | 用它避免排除路径上的冲突，并兼容仓库已有的 `merge.ours.driver` 约定 |
| `core.attributesFile` / `.git/info/attributes` | 两者都会影响 `check-attr --cached`；`-c core.attributesFile=<不存在>` 能屏蔽用户级文件，`info/attributes` 没有环境开关 | 屏蔽前者；后者存在即拒绝 |
| 只在 dev 上新增的 `merge=ours` 规则 | 单独用驱动不阻止冲突；把 dev 的 `.gitattributes` 作为 `core.attributesFile` 传入后冲突被按 ours 解决，且 in-tree 规则优先 | 合并时附加源分支规则 |
| `git check-attr --cached` | 读索引里的 `.gitattributes`；遵守 `GIT_INDEX_FILE`；路径不在索引里也能判定 | 规则判定完全交给 Git，包括子目录 `.gitattributes` |
| `git rm --cached` 作用于合并中被删除的路径 | `fatal: pathspec did not match`，exit 128 | 用 `--ignore-unmatch`，且删除集合来自枚举而非规则 |
| 含空格的路径 | 枚举与删除用 `-z` 无问题；`.gitattributes` 里写规则时模式需加引号 | README 要写明规则写法 |
| 只清理索引、保留工作区文件 | 下次运行的前置检查会失败、`checkout dev` 会报 untracked 覆盖 | 排除必须连工作区文件一起删 |

完整流程（driver 合并 → 全量枚举 → `git rm --cached` → 删工作区文件 → 提交 → 切回 → `--ff-only`）已端到端跑通：结束后 main 的树里没有排除路径、工作区为空、状态干净、`git checkout dev` 正常。

## 5. 方案与取舍

### 5.1 清理 main 历史

- **A（选中）逐提交重写**：`git filter-repo --refs main --invert-paths --path docs --path AGENTS.md`。main 的提交结构保留，每个提交里去掉这些路径，哈希全变。dev 的提交不共享这些 squash 提交，因此不受影响。
- **B 从共同起点重建 main**：把当前内容去掉这些路径后做成一个提交挂到共同起点上。没有第三方依赖、最容易验证彻底，但 main 的历史被压成一条，tag 与 release 会断。

选 A：同样能让 `git log main -- docs` 为空，但保住每次合并的记录；只有完全不在意 main 历史时 B 才更划算。

### 5.2 工具实现

- **方案一（选中）原地临时分支 + 驱动注入 + Git 判定规则**：冲突处理、属性匹配都交给 Git；代码最少；全流程已实测。
- **方案二 冲突后再处理**：不依赖 driver 注入，但要为内容冲突与 modify/delete 各写一套清理逻辑，代码路径翻倍，容易漏状态。
- **方案三 独立 worktree 隔离**：抗强杀最好，但多一个目录、Windows 上有路径长度与权限风险，而硬约束只要求"失败路径干净"，不要求抵御 kill -9。留作以后的加固项。

## 6. 工作项一：清理 main 历史（一次性）

本工作项的交付物是**可执行脚本 + 操作手册**，不是由工具执行；清理在真实仓库上由团队按手册运行。脚本接受仓库镜像路径与远端作为参数，不假定具体环境。

### 6.1 前置验证（任何一步不满足就停止）

```bash
# 所有含这些路径的 main 提交
git log --oneline main -- docs AGENTS.md
# 最早的那个提交是否也在 dev 上；只要有一个共享，就无法满足"不影响 dev"
git branch -a --contains <earliest-sha>
# main 与 dev 的共同起点本身不含这些路径（起点会用原 SHA 保留下来）
git merge-base main dev
git ls-tree -r --name-only $(git merge-base main dev) -- docs AGENTS.md
# 环境形态（与工具的 preflight 相同）
git rev-list --count --merges main              # 必须为 0
git merge-base --is-ancestor dev main; echo $?  # 必须非 0
git merge-base --all main dev                   # 恰好一个
# 例外路径（规则里用 -merge 取消的，如 docs/index.html）是否在 main 的历史里
git log --oneline main -- docs/index.html       # 在的话，清理不能按目录删
```

### 6.2 备份

```bash
git clone --mirror <remote-url> main-backup-<date>.git
```

### 6.3 重写

`git filter-repo` 要求 fresh clone；在备份之外单独做一份：

```bash
git clone --mirror <remote-url> filtered.git
cd filtered.git
git filter-repo --refs main --invert-paths \
  --path docs --path AGENTS.md --path CLAUDE.md \
  --path .claude --path .comet --path .superpowers --path .impeccable
# 注意：--path docs 会把 docs/index.html 一起删掉；例外路径的处理见下一条。
```

- 规则里 `-merge` 取消的路径（例如 `docs/index.html`）不是排除路径。如果它在 main 的历史里且必须保留，就不能用 `--path docs` 整目录删除，改用 `git filter-repo --filename-callback` 按文件判定（保留例外、删除其余）；否则这些文件会从 main 的历史里消失。
- 若 main 上有 tag 指向被重写的提交，把 tag 一起纳入重写（按 `git filter-repo --help` 的写法为每个 tag 追加 `--refs`），否则这些 tag 仍指向含旧路径的提交；不需要保留的 tag 直接删除。
- 不要用 `--all`：那会把 dev 也纳入重写，违反"不影响 dev"。
- 没有 Python 的环境可以用内建 `git filter-branch --index-filter 'git rm -r --cached --ignore-unmatch docs AGENTS.md' -- main` 作为退路（慢、已弃用，但同样只动 main）。

### 6.4 验证（在 filtered.git 里）

```bash
git log --oneline main -- docs AGENTS.md        # 必须为空
git diff <old-main-tip> main -- . ':(exclude)docs' ':(exclude)AGENTS.md'   # 必须为空
git merge-base main dev                         # 必须与清理前相同
git rev-parse dev                               # 必须与清理前相同：dev 的 ref、树与历史都不变
```

`old-main-tip` 从备份仓库取。第二条确认除这两个路径外，内容与清理前的 main 完全一致。

### 6.5 推送与团队动作

```bash
git push --force origin main        # 有 tag 被重写时再 push --tags
# 只推 main。不要用 --all 或 --mirror；推送后 dev 的 SHA 必须与推送前一致。
```

每个同事在自己本地：

```bash
git fetch --all --prune
git checkout main && git reset --hard origin/main
```

从旧 main 拉出的本地分支需要 rebase；dev 不需要任何动作，但它的 merge-base 依赖上面第 6.4 步的验证结果。

### 6.6 已知边界

- 远端平台（GitHub/GitLab）可能保留旧 PR 引用与缓存对象；"查不到"保证的是分支历史与 `git log main`，对象级彻底清除需要平台侧配合。
- 清理后如果有人在 main 上直接 `git merge dev`（非 squash），dev 的旧提交会进入 main 的祖先，`git log main -- docs` 会重新出现这些路径。这正是必须只通过工具更新 main、并在 CI 加检查的原因。
- 清理完成前，旧 main 的提交仍可从备份、同事的 reflog 和远端缓存里找到。

## 7. 工作项二：git-merge-safe 工具

### 7.1 项目与依赖

- Rust 二进制，名字 `git-merge-safe`，放到 `PATH` 后可执行 `git merge-safe`。
- 依赖：`clap`（derive）、`anyhow`。不要 `glob`——规则判定交给 Git。临时索引文件用标准库创建在 git 目录下。
- 所有 Git 调用用 `std::process::Command` 直接传 argv，不经 shell。捕获 stdout/stderr 与退出码。
- 最低 git 版本 2.30（`switch`、`check-attr --cached`、`--pathspec-from-file`）；启动时探测，不满足则明确报错。
- 输出为中文，统一前缀 `[git-merge-safe]`；`--verbose` 额外打印每条 Git 命令的 argv 和退出码。

建议模块：`cli`、`git`（命令封装）、`preflight`、`flow`（主流程与回滚）、`rules`（check-attr 与 dry-run）、`report`。集成测试见 7.9。

### 7.2 命令入口

```
git merge-safe [--source <branch>] [--dry-run] [--keep-temp] [--verbose]
git merge-safe --help | --version
```

- `--source` 默认 `dev`。
- `--dry-run`：只输出本次会被删除的路径，不建分支、不合并、不动索引与工作区。
- `--keep-temp`：成功后保留临时分支，便于排查。
- 不提供 `--temp-branch`：临时分支名自动生成，`merge-safe/tmp-<pid>-<random>`，避免用户起名冲突。

### 7.3 前置检查（按顺序，任何一项失败即退出，码 1）

1. 在 Git 仓库内（`git rev-parse --git-dir`）。
2. 当前是分支而非 detached HEAD（`git symbolic-ref --short HEAD`）。
3. 当前分支 ≠ `--source`。
4. 源分支存在且是本地分支（`git rev-parse --verify refs/heads/<src>^{commit}`）。
5. 没有进行中的合并/变基/拣选（`MERGE_HEAD`、`CHERRY_PICK_HEAD`、`REVERT_HEAD`、`REBASE_HEAD`、`rebase-apply`、`rebase-merge` 均不存在）。
6. 工作区完全干净：`git status --porcelain=v1 -z --untracked-files=normal` 输出为空。被忽略的文件不影响。
7. **环境形态**，即「当前分支的历史全部由源分支 squash 而来」中可检查的部分：
   - `git rev-list --count --merges <cur>` 必须为 0：没有真正的合并提交；squash 合并只产生线性历史。
   - `git merge-base --is-ancestor <src> <cur>` 必须返回非 0：源分支不是当前分支的祖先；否则说明曾经直接合并或快进过源分支。
   - `git merge-base --all <cur> <src>` 必须恰好给出一个提交：两者有共同起点；历史无关时拒绝。
   - Git 不记录提交是否由 squash 产生，无法逐条证明；以上三条是让本流程成立的、可检查的结构条件。任一不满足就拒绝运行，并说明是哪一条不满足。
8. 拿到排他锁：`<git-dir>/merge-safe.lock` 用 `create_new` 创建，写入 pid 与时间；已存在则报错并提示"确认没有其他实例在跑后删除该文件"。正常退出（含错误路径）时由 RAII 删除。
9. **属性源隔离**：`<git-dir>/info/attributes` 不存在或为空。它无法通过环境变量屏蔽，且会静默改变判定结果。

### 7.4 主流程（非 dry-run）

记 `cur` = 当前分支，`src` = 源分支，`head0` = 当前 HEAD，`src_head0` = `git rev-parse <src>`，`tmp` = 临时分支名。

1. **创建临时分支**：`git switch -c <tmp>`（基于 `head0`）。失败则清理后退出。
2. **squash 合并**：`git -c merge.ours.driver=true -c core.attributesFile=<src-attrs> merge --squash <src>`。
   - `<src-attrs>` 是 `git show <src>:.gitattributes` 写出的临时文件（源分支没有这个文件时用一个不存在的路径）。它让这次才在 dev 上新增的规则也能作用于冲突驱动；main 上已有的 in-tree 规则优先级更高；`-merge` 的路径按普通文件正常合并。
   - 驱动与补充属性源都只对这一条命令生效，不写 config，也不依赖同事是否配置过 `merge.ours.driver`。
   - 退出码非 0：记下 Git 输出，执行回滚，把 Git 的原始输出和退出码返回给用户，工具退出码 2。
   - 排除路径上的冲突不会发生（driver 保留 main 一侧），新增、修改、add/add 都验证过。
3. **枚举删除集合**：
   - `git ls-files -z` 取索引中全部路径；
   - `git check-attr --cached --stdin -z merge --`（路径经 stdin 以 NUL 分隔传入），解析 `路径\0属性\0值\0` 三元组，取值为 `exclude` 的路径；
   - 删除集合是**全量的**：不来自合并 diff，因此 main 上原有且 dev 未触碰的路径也会被删。
   - 若集合中有 gitlink（模式 160000），报错并回滚：不支持删除子模块路径。
4. **撤出索引**：把删除集合写成 NUL 分隔的临时文件，执行 `git rm -q --cached --ignore-unmatch --pathspec-from-file=<file> --pathspec-file-nul`。集合为空则跳过。
5. **删除工作区文件**：对集合中每个路径 `remove_file`（符号链接一并处理），忽略 NotFound；随后自底向上删除空目录，直到仓库根为止。任何其他 IO 错误都回滚——否则提交后 main 会留下未跟踪文件，违反"成功即干净"。
6. **提交**：先 `git diff --cached --quiet` 判断是否有内容可提交。
   - 无内容（源分支没有可合并的改动，或全部被排除）：回滚临时分支，提示"无内容可提交"，退出码 0。
   - 有内容：`git commit -q -m "merge <src> (squash) without excluded paths"`。钩子照常执行；钩子失败按失败回滚。
7. **提交后校验**：用 `git ls-tree -r -z <tmp>` 枚举新提交的树；树里仍有命中规则的路径（钩子把排除路径加回了提交），或 `git status --porcelain` 不为空（钩子在工作区留下文件）时，按失败回滚。
8. **切回并快进**：`git switch <cur>` → `git merge --ff-only <tmp>`。
   - 快进失败（main 被并发移动）：保留临时分支，打印临时分支名与手动处理步骤，退出码 2。
9. **删除临时分支**：`git branch -D <tmp>`；`--keep-temp` 时跳过。
10. **收尾校验**：`git status --porcelain` 应为空；不为空只告警（理论上不会发生）。
11. **摘要**：源分支、被排除的路径数与列表（过长时截断并提示 `--verbose`）、新 HEAD、提示工作区干净。

**源分支保护（贯穿全程）**

- 所有写操作只发生在临时分支和最后对当前分支的快进上；不写 git config（driver 只用 `-c` 对单条命令临时注入）、不 push、不打 tag、不改 `.gitattributes`、不切换或重设源分支。
- 成功路径和每一条失败路径结束时都断言 `git rev-parse <src>` 等于 `src_head0`。不相等按严重错误处理：打印诊断、保留临时分支与现场、退出码 2，不自动清理。

### 7.5 回滚

第 1–6 步任一步失败：

1. 若当前在 `tmp`：`git reset --hard <head0>`（`merge --abort` 对 squash 无效，不要调用）。
2. `git switch <cur>`。
3. `git branch -D <tmp>`（忽略失败）。
4. 删除锁文件（RAII 已覆盖，此处仅兜底）。

结果必须与执行前完全一致：main 的 HEAD 不变、索引不变、工作区不变、没有 `merge-safe/*` 分支、没有锁文件。

### 7.6 进程被强杀后的恢复（写进 README）

```bash
git branch --list 'merge-safe/*'
git switch <cur>            # 若失败：git reset --hard 之后再切
git reset --hard <cur>
git branch -D merge-safe/tmp-*
rm -f "$(git rev-parse --git-path merge-safe.lock)"
```

工具启动时若发现 `merge-safe/*` 分支，用告警形式列出，但不自动删除。

### 7.7 规则来源与判定

- 合并路径里**不解析** `.gitattributes`：合并后的索引里已经是 dev 版（或合并结果）的规则，`git check-attr --cached` 就是唯一裁判，子目录 `.gitattributes`、引号模式、字符类都自动正确。
- 属性名固定为 `merge`，值精确匹配 `ours`（区分大小写）：`ours` 表示禁止该路径从源分支进入当前分支，删除集合就是值为 `ours` 的路径。`-merge` 让 `check-attr` 返回 `unset`，表示在禁止范围内为这个文件开例外、正常合并；没有规则返回 `unspecified`。两者都不在删除集合里。`merge=ours` 同时是 Git 的 keep-ours 驱动名和本工具的禁止标记，README 要写明这层双关；`merge.ours.driver` 是本地/全局配置，不随仓库分发，工具在合并命令上临时注入 `true`，因此不要求同事预先配置。
- `.gitattributes` 自己也是普通文件，会随合并进入 main；如果希望它也不出现在 main，在规则里把它加上即可（不特殊处理）。
- `.gitattributes` 在合并中冲突时，合并失败、整体回滚——这是可接受的边界，README 说明。
- 判定环境必须隔离：所有 `check-attr` 调用设置 `GIT_ATTR_NOSYSTEM=1`，并用 `-c core.attributesFile=<git-dir>/merge-safe/no-attributes`（不存在的路径）屏蔽用户级属性文件。`<git-dir>/info/attributes` 无法用环境变量屏蔽且会静默改变删除集合，因此它存在且非空时拒绝运行（见 7.3 第 9 项）。

### 7.8 `--dry-run`

不建分支、不合并、不碰真实索引与工作区：

1. 候选路径 = `git ls-tree -r -z --name-only <src>` 与 `<cur>` 的并集（`--dry-run` 在合并前执行，这是合并结果路径集合的超集）。
2. 为 `<src>` 与 `<cur>` 各建一个临时索引文件（`GIT_INDEX_FILE=<git-dir>/merge-safe-dry-<pid>-<n>`），分别 `git read-tree <tree-ish>`。
3. 用对应的临时索引执行 `git check-attr --cached --stdin -z merge --`，对候选路径判定，取两次结果的并集。
4. 打印排序后的路径列表，说明"实际删除集合以合并后的结果为准，这里是超集"。
5. 删除临时索引文件；失败不影响真实状态。

已实测：`check-attr --cached` 遵守 `GIT_INDEX_FILE`，且能判定不在该索引里的路径；`read-tree` 不会动工作区。

`--dry-run` 运行除"工作区干净"和排他锁之外的全部前置检查（含环境形态）；它只写 `.git/` 下按 pid 命名的临时索引文件，不建分支、不碰真实索引与工作区。

### 7.9 测试计划

集成测试全部在临时仓库里跑真实 git（`assert_cmd` + 临时目录），CI 矩阵覆盖 Linux / macOS / Windows。

1. 全新 dev-only 目录被排除，其余变更进入 main。
2. 排除路径在 main 与 dev 都修改：零冲突，且该路径从 main 删除。
3. 排除路径在 main 上存在、dev 未触碰：仍从 main 删除（全量语义）。
4. 排除路径在 dev 被删除：不报 pathspec 错误。
5. add/add 的排除路径：零冲突，最终不在 main。
6. 非排除路径冲突：main 的 HEAD/索引/工作区不变，无临时分支，退出码非 0。
7. 工作区不干净（含未跟踪文件）：不建分支、不合并，退出码 1。
8. 当前分支 == 源分支：拒绝。
9. 没有排除规则：正常合并并提示。
10. 源分支没有新改动：无内容可提交，退出码 0，main 不变。
11. 成功后可立即再次运行；成功后 `git checkout dev` 不报 untracked 覆盖。
12. `--dry-run`：不产生分支、不改索引与工作区，输出正确。
13. `--keep-temp`：保留临时分支；默认删除。
14. 含空格、中文、换行的路径。
15. 含空格路径在 `.gitattributes` 中的引号写法。
16. 符号链接作为排除路径。
17. 锁文件已存在时拒绝运行。
18. 子模块路径命中规则时明确报错并回滚。
19. 合并中 `.gitattributes` 冲突时回滚。
20. `--verbose` 输出每条 Git 命令（不校验完整文案，只校验关键命令出现）。
21. 当前分支存在真正的 merge 提交：拒绝运行，退出码 1。
22. 源分支是当前分支的祖先（曾经直接合并过 dev）：拒绝运行。
23. 与源分支没有共同历史：拒绝运行。
24. 线性历史且与源分支有共同起点：通过环境检查。
25. 成功路径和每一条失败路径结束后，源分支 SHA 不变。
26. 源分支在另一个 worktree 检出时，运行成功且该 worktree 的内容与 HEAD 不被改动。
27. 规则采用完整示例（`docs/**`、`AGENTS.md`、`CLAUDE.md`、`.claude/**`、`.comet/**`、`.superpowers/**`、`.impeccable/**` 为 `merge=ours`）：这些路径被删除。
28. `docs/index.html -merge`：该文件保留，其余 `docs/` 路径被删除，`docs/` 目录因仍有文件而保留。
29. `-merge` 的路径在 main 与 dev 都修改时按普通文件冲突并整体回滚，不静默处理。
30. 仓库没有配置 `merge.ours.driver` 时，工具仍零冲突完成，且运行后 git config 不变。
31. 用户全局属性文件（`core.attributesFile`）里写 `* merge=ours`：删除集合不受影响。
32. `.git/info/attributes` 存在且非空：拒绝运行。
33. 排除规则这次才在 dev 上新增：本次合并仍零冲突（补充属性源生效）。
34. 钩子把排除路径加回提交或在工作区留下文件：提交后校验发现并回滚。

### 7.10 里程碑

| 阶段 | 内容 | 产出 |
|---|---|---|
| M0 | 按第 6 节交付清理脚本与操作手册；由团队在真实仓库执行，全组重置 main | 脚本 + 手册 + 干净的 main 历史 |
| M1 | 骨架、preflight、Git 调用封装、锁 | 可编译 CLI |
| M2 | 临时分支流程、driver 合并、全量删除、回滚 | 可用的核心 |
| M3 | dry-run、verbose、三平台集成测试 | 发布版 |
| M4 | README（安装、规则写法、恢复步骤）、CI 检查：main tip 的树与 main 历史中出现过的所有路径，按当前规则判定，不得命中 | 文档 + CI |

## 8. 边界与错误处理

| 场景 | 行为 |
|---|---|
| 当前分支 == 源分支 | 退出 1 |
| 当前分支有 merge 提交 / 源分支是它的祖先 / 无共同起点 | 退出 1，说明环境不是「全部由源分支 squash 而来」 |
| 运行后发现源分支 SHA 变化 | 严重错误：保留现场，退出 2，提示人工检查 |
| `.git/info/attributes` 存在且非空 | 退出 1，提示移走后再运行 |
| 工作区不干净（含未跟踪） | 退出 1，不建分支 |
| 已在进行中的合并/变基 | 退出 1 |
| 锁文件存在 | 退出 1，提示如何确认并清理 |
| 临时分支名冲突 | 自动重新生成 |
| 源分支不存在 / detached HEAD | 退出 1 |
| 没有排除规则 | 正常合并，提示"未配置排除规则" |
| `-merge` 取消的路径 | 正常合并并保留；冲突按普通文件处理 |
| 排除集合为空 | 正常合并，提交 |
| 排除后无可提交内容 | 回滚，提示，退出 0 |
| 非排除路径冲突 | 回滚，原样返回 Git 输出，退出 2 |
| `.gitattributes` 冲突 | 同上 |
| 命中路径是子模块 | 回滚，明确报错 |
| 工作区文件删除失败 | 回滚 |
| fast-forward 失败 | 保留临时分支，提示手动处理，退出 2 |
| 成功 | 摘要 + 工作区干净 |

## 9. 已知限制

- 不抵御进程被 SIGKILL：此时可能留下临时分支或半合并索引，README 给出恢复步骤。
- 排除路径上的 rename/modify 一类跨路径冲突仍可能让合并失败并整体回滚；不会静默处理。
- `--dry-run` 输出是超集，不是严格结果。
- 子模块路径不支持排除。
- 清理只能保证 main 的分支历史干净；远端平台的 PR 引用、缓存对象需要平台侧配合。
- 清理后，任何绕过工具的 `git merge dev`（非 squash）都会把旧提交带回 main 的祖先。
- 环境检查只能验证结构条件（线性、源分支非祖先、有共同起点）；Git 不记录提交是否由 squash 产生，不能逐条证明。
- 合并前只把源分支根目录的 `.gitattributes` 作为补充属性源；dev 上子目录的新规则不会参与本次冲突驱动（合并后的判定仍然正确）。
- CI 的历史检查用当前规则集近似评估历史路径；历史中已被删除的规则无法还原。

## 10. 留给实现者的决定

以下由实现者按工程判断决定，不改变上面任何行为：

- crate 的具体版本、模块划分细节、错误类型组织。
- pathspec 临时文件的命名与清理方式；批量大小（本设计用 `--pathspec-from-file`，无参数长度问题）。
- 测试框架与 CI 配置细节；Windows 上文件删除的权限处理细节。
- `--verbose` 的具体日志格式；正常输出中列表截断的长度。
- 临时分支随机后缀的生成方式。
- filter-repo 不可用时的退路脚本的具体写法。

## 11. 与 PRD 的差异（PRD 以本文为准）

- 删除旧版第 4 步"前置移除"：实测会因暂存删除或 untracked 残留直接让合并失败，或把内容冲突变成 modify/delete 冲突；改用 `git -c merge.ours.driver=true`；规则标记同时从自定义的 `exclude` 改为仓库已有的 `merge=ours`，`-merge` 用于逐个取消，判定只认 `ours`。
- 排除操作从 `git restore --staged` 改为"全量枚举 + `git rm --cached` + 删除工作区文件"：满足"main 上不留这些路径"和"成功后工作区干净"。
- 规则判定从自研 glob 改为 `git check-attr --cached`。
- `git merge --abort` 对 squash 无效，回滚统一用 `reset --hard`。
- 新增一次性历史清理（PRD 未覆盖）。
- 新增环境形态前置检查：当前分支无 merge 提交、源分支不是其祖先、两者有共同起点，否则拒绝运行。
- 合并时把源分支根目录的 `.gitattributes` 作为补充属性源，解决"新规则只在 dev 上时仍会冲突"的问题。
- 判定时屏蔽系统/用户级属性文件；`.git/info/attributes` 存在则拒绝运行。
- 不再提供"只影响暂存区、不动工作区"的语义。
- 去掉 `--temp-branch` 参数。

## 12. 风险

| 风险 | 对策 |
|---|---|
| 有人绕过工具直接合并 | CI 检查 main 树；migration 手册写明只通过工具更新 main |
| 历史重写后同事本地状态混乱 | 手册给出逐人重置步骤，清理前全组同步 |
| force push 被保护分支阻止 | 清理时临时放开保护，推完恢复 |
| 远端缓存仍可检索旧对象 | 手册写明平台侧边界 |
| 排除规则写错导致误删 | `--dry-run` + `--verbose`；删除的是精确路径且工作区干净、可从历史恢复 |
| 临时分支流程中途崩溃 | 失败路径统一回滚 + README 恢复步骤 |
| 误伤源分支 | 写操作只碰临时分支与当前分支；driver 只用 `-c` 临时注入；不 push/tag；运行前后断言 `rev-parse <src>` 一致，不一致报严重错误 |
| 机器本地属性文件改变删除集合 | 屏蔽系统/用户级属性文件；`.git/info/attributes` 存在即拒绝运行 |

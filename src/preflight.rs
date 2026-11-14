use crate::cli::Cli;
use crate::error::{Failure, Result};
use crate::git::{parse_git_version, version_supported, Git};
use crate::lock::Lock;

pub struct Ctx {
    pub git: Git,
    pub cur: String,
    pub src: String,
    pub head0: String,
    pub src_head0: String,
}

pub fn check(cli: &Cli, dry_run: bool) -> Result<(Ctx, Option<Lock>)> {
    check_inner(cli, dry_run).map_err(Failure::Preflight)
}

fn check_inner(cli: &Cli, dry_run: bool) -> anyhow::Result<(Ctx, Option<Lock>)> {
    let git = Git::discover(cli.verbose)?;

    let version_out = git.ok(git.run_str(&["--version"])?)?;
    let version = parse_git_version(&String::from_utf8_lossy(&version_out.stdout))
        .ok_or_else(|| anyhow::anyhow!("无法解析 git 版本"))?;
    if !version_supported(version) {
        anyhow::bail!(
            "需要 git 2.30 或更高版本，当前为 {}.{}",
            version.0,
            version.1
        );
    }

    let cur = git
        .current_branch()?
        .ok_or_else(|| anyhow::anyhow!("当前处于 detached HEAD，无法合并"))?;
    let src = cli.source.clone();
    if cur == src {
        anyhow::bail!("当前分支与源分支相同：{cur}");
    }
    git.rev_parse(&format!("refs/heads/{src}^{{commit}}"))
        .map_err(|_| anyhow::anyhow!("源分支不存在或不是本地分支：{src}"))?;

    if let Some(op) = git.in_progress_operation()? {
        anyhow::bail!("检测到进行中的操作（{op}），请先完成或中止");
    }

    if !dry_run && !git.status_porcelain()?.is_empty() {
        anyhow::bail!("工作区不干净，请先提交或 stash");
    }

    let merges = git.count_merges(&cur)?;
    if merges != 0 {
        anyhow::bail!("当前分支包含 {merges} 个合并提交，不是线性历史");
    }
    if git.is_ancestor(&src, &cur)? {
        anyhow::bail!("源分支 {src} 是当前分支的祖先，环境形态不符");
    }
    let bases = git.merge_bases(&cur, &src)?;
    if bases.len() != 1 {
        anyhow::bail!("当前分支与源分支的共同起点数量为 {}，应为 1", bases.len());
    }

    let lock = if dry_run {
        None
    } else {
        Some(Lock::acquire(git.abs_git_path("merge-safe.lock")?)?)
    };

    if git.info_attributes_present()? {
        anyhow::bail!(
            "检测到 .git/info/attributes；工具要求规则只来自版本化的 .gitattributes，请先移走它"
        );
    }

    let head0 = git.rev_parse("HEAD")?;
    let src_head0 = git.rev_parse(&src)?;
    Ok((
        Ctx {
            git,
            cur,
            src,
            head0,
            src_head0,
        },
        lock,
    ))
}

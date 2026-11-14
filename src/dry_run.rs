use crate::cli::Cli;
use crate::error::{Failure, Result};
use crate::git::Git;
use crate::preflight::Ctx;
use crate::rules::excluded_in_tree;

pub fn candidate_paths(git: &Git, cur: &str, src: &str) -> anyhow::Result<Vec<Vec<u8>>> {
    let mut paths = git.ls_tree_z(cur)?;
    paths.extend(git.ls_tree_z(src)?);
    paths.sort();
    paths.dedup();
    Ok(paths)
}

pub fn run(ctx: &Ctx, _cli: &Cli) -> Result<()> {
    run_inner(ctx).map_err(Failure::Git)
}

fn run_inner(ctx: &Ctx) -> anyhow::Result<()> {
    let candidates = candidate_paths(&ctx.git, &ctx.cur, &ctx.src)?;
    let mut excluded = excluded_in_tree(&ctx.git, &ctx.cur, &candidates)?;
    excluded.extend(excluded_in_tree(&ctx.git, &ctx.src, &candidates)?);
    excluded.sort();
    excluded.dedup();

    if excluded.is_empty() {
        println!("[git-merge-safe] 未配置排除规则");
    } else {
        for path in &excluded {
            println!(
                "[git-merge-safe] 将移除: {}",
                String::from_utf8_lossy(path)
            );
        }
        println!(
            "[git-merge-safe] 共 {} 个路径（预览为超集，实际以合并结果为准）",
            excluded.len()
        );
    }
    Ok(())
}

use crate::cli::Cli;
use crate::error::{Failure, Result};
use crate::git::{os_from_git_path, Git};
use crate::lock::Lock;
use crate::preflight::Ctx;
use crate::report;
use crate::rules::{excluded_in_index, excluded_in_tree};
use crate::temp::TempFile;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

pub fn run(ctx: Ctx, cli: &Cli, _lock: Lock) -> Result<()> {
    execute(&ctx, cli).map_err(|e| {
        if e.downcast_ref::<SourceChanged>().is_some() {
            Failure::Severe(e)
        } else {
            Failure::Git(e)
        }
    })
}

#[derive(Debug)]
struct SourceChanged(String);

impl std::fmt::Display for SourceChanged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for SourceChanged {}

enum Step {
    Done(Vec<Vec<u8>>),
    NoContent,
}

fn execute(ctx: &Ctx, cli: &Cli) -> anyhow::Result<()> {
    let git = &ctx.git;
    let tmp = unique_temp_branch(git)?;

    let step = match prepare(git, ctx, &tmp) {
        Ok(step) => step,
        Err(e) => {
            return match rollback(git, &ctx.cur, &ctx.head0, &tmp) {
                Ok(()) => Err(e),
                Err(rb) => Err(anyhow::anyhow!("{e}；回滚失败：{rb}")),
            };
        }
    };

    match step {
        Step::NoContent => {
            rollback(git, &ctx.cur, &ctx.head0, &tmp)?;
            report::info("无内容可提交");
            Ok(())
        }
        Step::Done(excluded) => {
            let src_now = git.rev_parse(&ctx.src)?;
            verify_source(&ctx.src_head0, &src_now)?;
            if let Err(e) = git.ok(git.run_str(&["switch", &ctx.cur])?) {
                eprintln!(
                    "[git-merge-safe] 切回 {} 失败：已保留临时分支 {}（合并结果）与 {} 的合并前位置 {}",
                    ctx.cur, tmp, ctx.cur, ctx.head0
                );
                eprintln!(
                    "[git-merge-safe] 恢复：git status；必要时 git branch -f {} {}，然后 git switch {} && git merge --ff-only {} && git branch -D {}",
                    ctx.cur, ctx.head0, ctx.cur, tmp, tmp
                );
                return Err(e);
            }
            let ff = git.ok(git.run_str(&["merge", "--ff-only", &tmp])?);
            if let Err(e) = ff {
                eprintln!(
                    "[git-merge-safe] fast-forward 失败，已保留临时分支 {tmp}；main 未被本工具修改"
                );
                return Err(e);
            }
            let src_now = git.rev_parse(&ctx.src)?;
            verify_source(&ctx.src_head0, &src_now)?;
            if !cli.keep_temp {
                if let Err(e) = git.run_str(&["branch", "-D", &tmp]) {
                    report::warn(&format!("删除临时分支失败：{e}"));
                }
            }
            if !git.status_porcelain()?.is_empty() {
                report::warn("合并后工作区不干净，请检查");
            }
            report::info(&format!("已从索引与工作区移除 {} 个路径", excluded.len()));
            for p in &excluded {
                report::info(&format!("  {}", String::from_utf8_lossy(p)));
            }
            report::info(&format!(
                "已 fast-forward {} 到 {}",
                ctx.cur,
                git.rev_parse("HEAD")?
            ));
            report::info(&format!("工作区干净；源分支 {} 未变动", ctx.src));
            Ok(())
        }
    }
}

fn prepare(git: &Git, ctx: &Ctx, tmp: &str) -> anyhow::Result<Step> {
    git.ok(git.run_str(&["switch", "-c", tmp])?)?;

    let attrs_dir = git.abs_git_path("merge-safe")?;
    std::fs::create_dir_all(&attrs_dir)?;
    let _attrs_guard;
    let attrs: PathBuf;
    match git.show_file(&ctx.src, ".gitattributes")? {
        Some(bytes) => {
            let f = TempFile::new(
                &attrs_dir,
                &format!("src-attrs-{}", std::process::id()),
                &bytes,
            )?;
            attrs = f.path().to_path_buf();
            _attrs_guard = Some(f);
        }
        None => {
            attrs = git.abs_git_path("merge-safe/no-attributes")?;
            _attrs_guard = None;
        }
    }

    let mut cfg = OsString::from("core.attributesFile=");
    cfg.push(attrs.as_os_str());
    let args: Vec<&OsStr> = vec![
        OsStr::new("-c"),
        OsStr::new("merge.ours.driver=true"),
        OsStr::new("-c"),
        cfg.as_os_str(),
        OsStr::new("merge"),
        OsStr::new("--squash"),
        OsStr::new(&ctx.src),
    ];
    let envs: Vec<(&str, &OsStr)> = vec![("GIT_ATTR_NOSYSTEM", OsStr::new("1"))];
    let out = git.run_with_input(&args, &[], &envs)?;
    if !out.status.success() {
        let stdout = String::from_utf8_lossy(&out.stdout);
        if !stdout.trim().is_empty() {
            eprintln!("{}", stdout.trim_end());
        }
        let stderr = String::from_utf8_lossy(&out.stderr);
        if !stderr.trim().is_empty() {
            eprintln!("{}", stderr.trim_end());
        }
        anyhow::bail!("git merge --squash 失败");
    }

    let mut excluded = removal_set(git)?;
    excluded.sort();
    excluded.dedup();

    if !excluded.is_empty() {
        let delete_file = TempFile::new(
            &attrs_dir,
            &format!("delete-{}", std::process::id()),
            &nul_list(&excluded),
        )?;
        let args: Vec<&OsStr> = vec![
            OsStr::new("--literal-pathspecs"),
            OsStr::new("rm"),
            OsStr::new("-q"),
            OsStr::new("--cached"),
            OsStr::new("--ignore-unmatch"),
            OsStr::new("--pathspec-from-file"),
            delete_file.path().as_os_str(),
            OsStr::new("--pathspec-file-nul"),
        ];
        git.ok(git.run(&args)?)?;
        remove_from_worktree(git, &excluded)?;
    }

    if git.diff_cached_is_empty()? {
        return Ok(Step::NoContent);
    }

    let message = format!("merge {} (squash) without excluded paths", ctx.src);
    git.ok(git.run_str(&["commit", "-q", "-m", &message])?)?;

    let tree_paths = git.ls_tree_z(tmp)?;
    let leftovers = excluded_in_tree(git, tmp, &tree_paths)?;
    if !leftovers.is_empty() {
        anyhow::bail!("提交后校验失败：排除路径仍在提交中");
    }
    if !git.status_porcelain()?.is_empty() {
        anyhow::bail!("提交后校验失败：工作区不干净");
    }

    Ok(Step::Done(excluded))
}

fn removal_set(git: &Git) -> anyhow::Result<Vec<Vec<u8>>> {
    let paths = git.ls_files_z()?;
    let excluded = excluded_in_index(git, &paths)?;
    let staged = git.ls_files_stage_z()?;
    for p in &excluded {
        if let Some((_, mode)) = staged.iter().find(|(q, _)| q == p) {
            if mode.as_str() == "160000" {
                anyhow::bail!("命中子模块路径，未做任何修改");
            }
        }
    }
    Ok(excluded)
}

fn nul_list(paths: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for p in paths {
        out.extend_from_slice(p);
        out.push(0);
    }
    out
}

fn remove_from_worktree(git: &Git, paths: &[Vec<u8>]) -> anyhow::Result<()> {
    for p in paths {
        let abs = git.top_level().join(os_from_git_path(p));
        match std::fs::remove_file(&abs) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => anyhow::bail!("删除工作区文件失败 {}：{e}", abs.display()),
        }
        prune_empty_parents(&abs, git.top_level());
    }
    Ok(())
}

fn prune_empty_parents(file: &Path, top: &Path) {
    let mut dir = file.parent();
    while let Some(d) = dir {
        if d == top || !d.starts_with(top) {
            break;
        }
        if std::fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
}

fn unique_temp_branch(git: &Git) -> anyhow::Result<String> {
    for _ in 0..8 {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let name = format!("merge-safe/tmp-{}-{nanos:08x}", std::process::id());
        if !git.branch_exists(&name)? {
            return Ok(name);
        }
    }
    anyhow::bail!("无法生成唯一的临时分支名")
}

fn rollback(git: &Git, cur: &str, head0: &str, tmp: &str) -> anyhow::Result<()> {
    if git.current_branch()?.as_deref() == Some(tmp) {
        if let Err(e) = git.ok(git.run_str(&["reset", "--hard", head0])?) {
            anyhow::bail!("回滚失败：{e}；请手动执行 git reset --hard {head0} && git switch {cur}");
        }
        if let Err(e) = git.ok(git.run_str(&["switch", cur])?) {
            anyhow::bail!("回滚切回 {cur} 失败：{e}");
        }
    }
    // The worktree was clean before the run, so any untracked file now was
    // created during it (typically by a hook). Remove those without touching
    // ignored files, which may legitimately pre-exist.
    if let Err(e) = git.ok(git.run_str(&["clean", "-fd"])?) {
        anyhow::bail!("回滚清理未跟踪文件失败：{e}；请手动执行 git clean -fd");
    }
    let _ = git.run_str(&["branch", "-D", tmp]);
    Ok(())
}

fn verify_source(expected: &str, actual: &str) -> anyhow::Result<()> {
    if expected != actual {
        return Err(anyhow::Error::new(SourceChanged(format!(
            "源分支 HEAD 发生变化（{expected} → {actual}）"
        ))));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_change_is_detected() {
        assert!(verify_source("a", "a").is_ok());
        assert!(verify_source("a", "b").is_err());
    }
}

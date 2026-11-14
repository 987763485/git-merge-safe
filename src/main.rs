use clap::Parser;
use git_merge_safe::cli::Cli;
use git_merge_safe::dry_run;
use git_merge_safe::error::{Failure, Result};
use git_merge_safe::flow;
use git_merge_safe::preflight;

fn main() {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(()) => {}
        Err(failure) => {
            eprintln!("[git-merge-safe] {failure}");
            std::process::exit(failure.code());
        }
    }
}

fn run(cli: &Cli) -> Result<()> {
    if cli.message.iter().any(|m| m.trim().is_empty()) {
        return Err(Failure::Preflight(anyhow::anyhow!("--message 不能为空")));
    }
    let (ctx, lock) = preflight::check(cli, cli.dry_run)?;
    if cli.dry_run {
        dry_run::run(&ctx, cli)?;
        return Ok(());
    }
    let lock = lock.ok_or_else(|| Failure::Preflight(anyhow::anyhow!("内部错误：缺少排他锁")))?;
    flow::run(ctx, cli, lock)
}

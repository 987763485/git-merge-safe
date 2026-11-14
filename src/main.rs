use clap::Parser;
use git_merge_safe::cli::Cli;
use git_merge_safe::dry_run;
use git_merge_safe::error::Result;
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
    let (ctx, _lock) = preflight::check(cli, cli.dry_run)?;
    if cli.dry_run {
        dry_run::run(&ctx, cli)?;
    }
    Ok(())
}

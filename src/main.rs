use clap::Parser;
use git_merge_safe::cli::Cli;
use git_merge_safe::preflight;

fn main() {
    let cli = Cli::parse();
    match preflight::check(&cli, cli.dry_run) {
        Ok(_) => {}
        Err(failure) => {
            eprintln!("[git-merge-safe] {failure}");
            std::process::exit(failure.code());
        }
    }
}

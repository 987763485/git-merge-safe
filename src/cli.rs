use clap::Parser;

#[derive(Debug, Clone, Parser)]
#[command(name = "git-merge-safe", version, about = "Squash-merge a branch without bringing excluded paths into the current branch")]
pub struct Cli {
    #[arg(long, default_value = "dev", value_name = "branch")]
    pub source: String,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub keep_temp: bool,
    #[arg(long)]
    pub verbose: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults() {
        let c = Cli::parse_from(["git-merge-safe"]);
        assert_eq!(c.source, "dev");
        assert!(!c.dry_run && !c.keep_temp && !c.verbose);
    }
}

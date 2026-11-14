use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug)]
pub struct Git {
    top_level: PathBuf,
    git_dir: PathBuf,
    verbose: bool,
}

#[derive(Debug)]
pub struct CmdOutput {
    pub status: std::process::ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl Git {
    pub fn discover(verbose: bool) -> anyhow::Result<Self> {
        Self::discover_in(&std::env::current_dir()?, verbose)
    }

    pub fn discover_in(start: &Path, verbose: bool) -> anyhow::Result<Self> {
        let git_dir_raw = run_capture(start, &["rev-parse", "--git-dir"])?;
        let top_raw = run_capture(start, &["rev-parse", "--show-toplevel"])?;
        let top_level = resolve(start, top_raw.trim());
        let git_dir = resolve(start, git_dir_raw.trim());
        Ok(Self {
            top_level,
            git_dir,
            verbose,
        })
    }

    pub fn top_level(&self) -> &Path {
        &self.top_level
    }

    pub fn git_dir(&self) -> &Path {
        &self.git_dir
    }

    pub fn abs_git_path(&self, rel: &str) -> anyhow::Result<PathBuf> {
        let out = self.ok(self.run_str(&["rev-parse", "--git-path", rel])?)?;
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let p = PathBuf::from(text);
        Ok(if p.is_absolute() {
            p
        } else {
            self.top_level.join(p)
        })
    }

    pub fn run(&self, args: &[&OsStr]) -> anyhow::Result<CmdOutput> {
        self.run_with_input(args, &[], &[])
    }

    pub fn run_str(&self, args: &[&str]) -> anyhow::Result<CmdOutput> {
        let owned: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
        self.run(&owned)
    }

    pub fn run_with_input(
        &self,
        args: &[&OsStr],
        stdin: &[u8],
        envs: &[(&str, &OsStr)],
    ) -> anyhow::Result<CmdOutput> {
        if self.verbose {
            eprintln!("[git-merge-safe] $ {}", render_command(args));
        }
        let mut cmd = Command::new("git");
        cmd.current_dir(&self.top_level)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in envs {
            cmd.env(key, value);
        }
        if stdin.is_empty() {
            cmd.stdin(Stdio::null());
        } else {
            cmd.stdin(Stdio::piped());
        }

        let mut child = cmd.spawn()?;
        if !stdin.is_empty() {
            if let Some(mut si) = child.stdin.take() {
                si.write_all(stdin)?;
            }
        }
        let out = child.wait_with_output()?;
        if self.verbose {
            eprintln!(
                "[git-merge-safe]   exit {}",
                out.status.code().unwrap_or(-1)
            );
        }
        Ok(CmdOutput {
            status: out.status,
            stdout: out.stdout,
            stderr: out.stderr,
        })
    }

    pub fn ok(&self, out: CmdOutput) -> anyhow::Result<CmdOutput> {
        if out.status.success() {
            Ok(out)
        } else {
            let code = out.status.code().unwrap_or(-1);
            anyhow::bail!(
                "git 命令失败（退出码 {code}）：{}",
                String::from_utf8_lossy(&out.stderr).trim()
            )
        }
    }
}

fn resolve(base: &Path, p: &str) -> PathBuf {
    let pb = PathBuf::from(p);
    if pb.is_absolute() {
        pb
    } else {
        base.join(pb)
    }
}

fn run_capture(cwd: &Path, args: &[&str]) -> anyhow::Result<String> {
    let out = Command::new("git").current_dir(cwd).args(args).output()?;
    if !out.status.success() {
        anyhow::bail!(
            "git {} 失败：{}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn render_command(args: &[&OsStr]) -> String {
    let mut s = String::from("git");
    for a in args {
        s.push(' ');
        s.push_str(&a.to_string_lossy());
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_command_joins_argv() {
        let args = [OsStr::new("rev-parse"), OsStr::new("HEAD")];
        assert_eq!(render_command(&args), "git rev-parse HEAD");
    }

    #[test]
    fn empty_render_is_git() {
        assert_eq!(render_command(&[]), "git");
    }
}

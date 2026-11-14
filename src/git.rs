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

    pub fn rev_parse(&self, rev: &str) -> anyhow::Result<String> {
        let out = self.ok(self.run_str(&["rev-parse", rev])?)?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    pub fn current_branch(&self) -> anyhow::Result<Option<String>> {
        let out = self.run_str(&["symbolic-ref", "--short", "HEAD"])?;
        if out.status.success() {
            return Ok(Some(
                String::from_utf8_lossy(&out.stdout).trim().to_string(),
            ));
        }
        let stderr = String::from_utf8_lossy(&out.stderr);
        if stderr.contains("not a symbolic ref") {
            Ok(None)
        } else {
            anyhow::bail!("git symbolic-ref 失败：{}", stderr.trim())
        }
    }

    pub fn status_porcelain(&self) -> anyhow::Result<Vec<u8>> {
        let out = self.ok(self.run_str(&[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=normal",
        ])?)?;
        Ok(out.stdout)
    }

    pub fn count_merges(&self, rev: &str) -> anyhow::Result<u64> {
        let out = self.ok(self.run_str(&["rev-list", "--count", "--merges", rev])?)?;
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        text.parse::<u64>()
            .map_err(|e| anyhow::anyhow!("解析 rev-list 计数失败：{e}"))
    }

    pub fn is_ancestor(&self, ancestor: &str, descendant: &str) -> anyhow::Result<bool> {
        let out = self.run_str(&["merge-base", "--is-ancestor", ancestor, descendant])?;
        match out.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => anyhow::bail!(
                "git merge-base --is-ancestor 失败：{}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        }
    }

    pub fn merge_bases(&self, a: &str, b: &str) -> anyhow::Result<Vec<String>> {
        let out = self.run_str(&["merge-base", "--all", a, b])?;
        match out.status.code() {
            Some(0) => Ok(String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()),
            Some(1) => Ok(Vec::new()),
            _ => anyhow::bail!(
                "git merge-base 失败：{}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        }
    }

    pub fn ls_files_z(&self) -> anyhow::Result<Vec<Vec<u8>>> {
        let out = self.ok(self.run_str(&["ls-files", "-z"])?)?;
        Ok(split_nul(&out.stdout))
    }

    pub fn ls_files_stage_z(&self) -> anyhow::Result<Vec<(Vec<u8>, String)>> {
        let out = self.ok(self.run_str(&["ls-files", "-s", "-z"])?)?;
        let mut result = Vec::new();
        for record in split_nul(&out.stdout) {
            let tab = record
                .iter()
                .position(|&b| b == b'\t')
                .ok_or_else(|| anyhow::anyhow!("无法解析 ls-files -s 输出"))?;
            let meta = String::from_utf8_lossy(&record[..tab]);
            let mode = meta.split_whitespace().next().unwrap_or("").to_string();
            result.push((record[tab + 1..].to_vec(), mode));
        }
        Ok(result)
    }

    pub fn ls_tree_z(&self, rev: &str) -> anyhow::Result<Vec<Vec<u8>>> {
        let out = self.ok(self.run_str(&["ls-tree", "-r", "-z", "--name-only", rev])?)?;
        Ok(split_nul(&out.stdout))
    }

    pub fn show_file(&self, rev: &str, path: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let spec = format!("{rev}:{path}");
        let out = self.run_str(&["show", &spec])?;
        if out.status.success() {
            Ok(Some(out.stdout))
        } else {
            Ok(None)
        }
    }

    pub fn diff_cached_is_empty(&self) -> anyhow::Result<bool> {
        let out = self.run_str(&["diff", "--cached", "--quiet"])?;
        match out.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => anyhow::bail!(
                "git diff --cached --quiet 失败：{}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        }
    }

    pub fn branch_exists(&self, name: &str) -> anyhow::Result<bool> {
        let refname = format!("refs/heads/{name}");
        let out = self.run_str(&["show-ref", "--verify", "--quiet", &refname])?;
        match out.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => anyhow::bail!(
                "git show-ref 失败：{}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        }
    }

    pub fn in_progress_operation(&self) -> anyhow::Result<Option<String>> {
        for name in ["MERGE_HEAD", "CHERRY_PICK_HEAD", "REVERT_HEAD", "REBASE_HEAD"] {
            let out = self.run_str(&["rev-parse", "-q", "--verify", name])?;
            if out.status.success() {
                return Ok(Some(name.to_string()));
            }
            match out.status.code() {
                Some(1) | Some(128) => {}
                _ => {
                    anyhow::bail!(
                        "git rev-parse 失败：{}",
                        String::from_utf8_lossy(&out.stderr).trim()
                    )
                }
            }
        }
        for name in ["rebase-apply", "rebase-merge"] {
            if self.abs_git_path(name)?.is_dir() {
                return Ok(Some(name.to_string()));
            }
        }
        Ok(None)
    }

    pub fn info_attributes_present(&self) -> anyhow::Result<bool> {
        let path = self.abs_git_path("info/attributes")?;
        Ok(std::fs::metadata(&path)
            .map(|m| m.len() > 0)
            .unwrap_or(false))
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

#[cfg(unix)]
pub fn os_from_git_path(p: &[u8]) -> std::ffi::OsString {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(p).to_os_string()
}

#[cfg(not(unix))]
pub fn os_from_git_path(p: &[u8]) -> std::ffi::OsString {
    String::from_utf8_lossy(p).into_owned().into()
}

pub fn split_nul(bytes: &[u8]) -> Vec<Vec<u8>> {    let mut out: Vec<Vec<u8>> = bytes.split(|&b| b == 0).map(|s| s.to_vec()).collect();
    if out.last().map(|v| v.is_empty()).unwrap_or(false) {
        out.pop();
    }
    out
}

pub fn parse_git_version(out: &str) -> Option<(u32, u32)> {
    let token = out.split_whitespace().nth(2)?;
    let mut parts = token.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

pub fn version_supported(v: (u32, u32)) -> bool {
    v >= (2, 30)
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

use std::path::{Path, PathBuf};

pub struct TestRepo {
    pub dir: tempfile::TempDir,
}

impl TestRepo {
    pub fn new() -> Self {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = Self { dir };
        repo.git_ok(&["init", "-b", "main"]);
        repo.git_ok(&["config", "user.name", "Test"]);
        repo.git_ok(&["config", "user.email", "test@example.com"]);
        repo
    }

    pub fn path(&self) -> &Path { self.dir.path() }

    pub fn git(&self, args: &[&str]) -> std::process::Output {
        std::process::Command::new("git")
            .current_dir(self.path())
            .args(args)
            .output()
            .unwrap()
    }

    pub fn git_ok(&self, args: &[&str]) -> String {
        let out = self.git(args);
        assert!(
            out.status.success(),
            "git {args:?} failed\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    pub fn write(&self, rel: &str, content: &str) {
        let p: PathBuf = self.path().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    pub fn commit_all(&self, msg: &str) {
        self.git_ok(&["add", "-A"]);
        self.git_ok(&["commit", "-q", "-m", msg]);
    }

    pub fn checkout(&self, branch: &str) { self.git_ok(&["checkout", "-q", branch]); }
    pub fn checkout_new(&self, branch: &str) { self.git_ok(&["checkout", "-q", "-b", branch]); }
    pub fn rev(&self, rev: &str) -> String { self.git_ok(&["rev-parse", rev]) }

    pub fn bin(&self) -> assert_cmd::Command {
        let mut cmd = assert_cmd::Command::cargo_bin("git-merge-safe").unwrap();
        cmd.current_dir(self.path());
        cmd
    }
}

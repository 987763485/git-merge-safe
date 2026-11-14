use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct Lock {
    path: PathBuf,
}

impl Lock {
    pub fn acquire(path: PathBuf) -> anyhow::Result<Self> {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                let _ = writeln!(file, "{}", std::process::id());
                Ok(Self { path })
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                anyhow::bail!(
                    "另一个 git-merge-safe 正在运行，或上次异常退出；确认后删除 {} 再重试",
                    path.display()
                )
            }
            Err(e) => Err(e.into()),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

use crate::git::{split_nul, Git};
use crate::temp::RemoveOnDrop;
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

pub fn parse_check_attr_z(out: &[u8]) -> anyhow::Result<Vec<(Vec<u8>, String)>> {
    let parts = split_nul(out);
    if parts.len() % 3 != 0 {
        anyhow::bail!("check-attr 输出不是完整的三元组");
    }
    let mut result = Vec::with_capacity(parts.len() / 3);
    for chunk in parts.chunks(3) {
        result.push((
            chunk[0].clone(),
            String::from_utf8_lossy(&chunk[2]).to_string(),
        ));
    }
    Ok(result)
}

fn check_attr(git: &Git, index: Option<&Path>, paths: &[Vec<u8>]) -> anyhow::Result<Vec<Vec<u8>>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }

    let noattrs = git.abs_git_path("merge-safe/no-attributes")?;
    let mut cfg = OsString::from("core.attributesFile=");
    cfg.push(noattrs.as_os_str());

    let args: Vec<&OsStr> = vec![
        OsStr::new("-c"),
        cfg.as_os_str(),
        OsStr::new("check-attr"),
        OsStr::new("--cached"),
        OsStr::new("--stdin"),
        OsStr::new("-z"),
        OsStr::new("merge"),
        OsStr::new("--"),
    ];

    let mut stdin = Vec::new();
    for p in paths {
        stdin.extend_from_slice(p);
        stdin.push(0);
    }

    let mut envs: Vec<(&str, OsString)> = vec![("GIT_ATTR_NOSYSTEM", OsString::from("1"))];
    if let Some(idx) = index {
        envs.push(("GIT_INDEX_FILE", idx.as_os_str().to_os_string()));
    }
    let env_refs: Vec<(&str, &OsStr)> = envs.iter().map(|(k, v)| (*k, v.as_os_str())).collect();

    let out = git.run_with_input(&args, &stdin, &env_refs)?;
    let out = git.ok(out)?;
    Ok(parse_check_attr_z(&out.stdout)?
        .into_iter()
        .filter(|(_, value)| value == "ours")
        .map(|(path, _)| path)
        .collect())
}

pub fn excluded_in_index(git: &Git, paths: &[Vec<u8>]) -> anyhow::Result<Vec<Vec<u8>>> {
    check_attr(git, None, paths)
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn excluded_in_tree(git: &Git, rev: &str, paths: &[Vec<u8>]) -> anyhow::Result<Vec<Vec<u8>>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let index_path = git.abs_git_path(&format!("merge-safe/dry-{}-{n}", std::process::id()))?;
    if let Some(parent) = index_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _ = std::fs::remove_file(&index_path);
    let guard = RemoveOnDrop::new(index_path);

    let index_os = guard.path().as_os_str().to_os_string();
    let args: Vec<&OsStr> = vec![OsStr::new("read-tree"), OsStr::new(rev)];
    let envs: Vec<(&str, &OsStr)> = vec![("GIT_INDEX_FILE", index_os.as_os_str())];
    let out = git.run_with_input(&args, &[], &envs)?;
    git.ok(out)?;

    check_attr(git, Some(guard.path()), paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_triples() {
        let pairs = parse_check_attr_z(
            b"a\0merge\0ours\0b\0merge\0unset\0c\0merge\0unspecified\0",
        )
        .unwrap();
        assert_eq!(
            pairs,
            vec![
                (b"a".to_vec(), "ours".to_string()),
                (b"b".to_vec(), "unset".to_string()),
                (b"c".to_vec(), "unspecified".to_string()),
            ]
        );
    }

    #[test]
    fn rejects_partial_records() {
        assert!(parse_check_attr_z(b"a\0merge\0").is_err());
    }
}

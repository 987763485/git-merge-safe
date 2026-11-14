use git_merge_safe::lock::Lock;

#[test]
fn acquire_creates_and_drop_removes() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("merge-safe.lock");
    {
        let lock = Lock::acquire(path.clone()).unwrap();
        assert!(path.exists());
        assert_eq!(lock.path(), path);
    }
    assert!(!path.exists());
}

#[test]
fn second_acquire_fails_with_path() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("merge-safe.lock");
    let _lock = Lock::acquire(path.clone()).unwrap();
    let err = Lock::acquire(path.clone()).unwrap_err();
    assert!(err.to_string().contains("merge-safe.lock"), "{err}");
}

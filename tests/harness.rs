mod common;
use common::TestRepo;

#[test]
fn harness_creates_committed_repo() {
    let repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    repo.commit_all("base");
    assert_eq!(repo.rev("HEAD").len(), 40);
    assert!(repo.git_ok(&["status", "--porcelain"]).is_empty());
}

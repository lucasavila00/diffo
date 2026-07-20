use std::{
    ffi::OsString,
    fs,
    os::unix::{ffi::OsStringExt as _, fs::symlink},
    path::{Path, PathBuf},
    process::Command,
};

use diffo_core::{ExplorerFileContent, Repository};

use super::{git, test_repository};
use crate::GitRepositorySource;

#[test]
fn lists_regular_worktree_files_independently_of_git() {
    let repo = test_repository();
    fs::write(repo.path().join("untracked.txt"), "new\n").expect("write untracked file");
    fs::write(repo.path().join("removed.txt"), "remove\n").expect("write removable file");
    git(repo.path(), &["add", "removed.txt"]);
    git(repo.path(), &["commit", "-m", "add removable file"]);
    fs::remove_file(repo.path().join("removed.txt")).expect("remove tracked file");
    fs::write(repo.path().join("ignored.txt"), "ignored\n").expect("write ignored file");
    fs::write(repo.path().join(".hidden"), "hidden\n").expect("write hidden file");
    fs::write(repo.path().join(".gitignore"), "ignored.txt\n").expect("write ignore file");
    fs::create_dir(repo.path().join("empty")).expect("create empty directory");
    symlink("tracked.txt", repo.path().join("linked.txt")).expect("create file symlink");
    let non_utf8 = PathBuf::from(OsString::from_vec(b"non-utf8-\xff".to_vec()));
    fs::write(repo.path().join(&non_utf8), "bytes\n").expect("write non-UTF-8 path");
    assert!(
        Command::new("mkfifo")
            .arg(repo.path().join("pipe"))
            .status()
            .expect("create fifo")
            .success()
    );
    let source = GitRepositorySource::new(repo.path());

    let paths = source.explorer_paths().expect("Explorer paths");

    assert!(paths.contains(&PathBuf::from("tracked.txt")));
    assert!(paths.contains(&PathBuf::from("untracked.txt")));
    assert!(paths.contains(&PathBuf::from(".gitignore")));
    assert!(paths.contains(&PathBuf::from("ignored.txt")));
    assert!(paths.contains(&PathBuf::from(".hidden")));
    assert!(paths.contains(&PathBuf::from("linked.txt")));
    assert!(paths.contains(&non_utf8));
    assert!(!paths.contains(&PathBuf::from("removed.txt")));
    assert!(!paths.iter().any(|path| path.starts_with(".git")));
    assert!(!paths.contains(&PathBuf::from("empty")));
    assert!(!paths.contains(&PathBuf::from("pipe")));
}

#[test]
fn renders_ignored_files_without_a_git_change_gutter() {
    let repo = test_repository();
    fs::write(repo.path().join(".gitignore"), "ignored.txt\n").expect("write ignore file");
    fs::write(repo.path().join("ignored.txt"), "ignored\n").expect("write ignored file");
    let source = GitRepositorySource::new(repo.path());

    let file = source
        .explorer_file(Path::new("ignored.txt"))
        .expect("ignored file");

    assert_eq!(
        file.content,
        ExplorerFileContent::Text("ignored\n".to_owned())
    );
    assert!(file.patch.is_empty());
}

#[test]
fn reads_worktree_contents_and_rejects_removed_paths() {
    let repo = test_repository();
    let source = GitRepositorySource::new(repo.path());
    fs::write(repo.path().join("tracked.txt"), "changed\n").expect("modify file");

    let changed = source
        .explorer_file(Path::new("tracked.txt"))
        .expect("changed file");
    assert_eq!(
        changed.content,
        ExplorerFileContent::Text("changed\n".to_owned())
    );
    assert!(changed.patch.contains("+changed"));

    fs::remove_file(repo.path().join("tracked.txt")).expect("delete file");
    let error = source
        .explorer_file(Path::new("tracked.txt"))
        .expect_err("removed file must not be readable in Explorer");
    assert!(error.to_string().contains("failed to inspect file"));
}

#[test]
fn marks_binary_contents_without_decoding_them() {
    let repo = test_repository();
    fs::write(repo.path().join("binary.dat"), [0, 159, 146, 150]).expect("write binary file");
    let source = GitRepositorySource::new(repo.path());

    let file = source
        .explorer_file(Path::new("binary.dat"))
        .expect("binary file");

    assert_eq!(file.content, ExplorerFileContent::Binary);
}

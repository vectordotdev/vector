use std::{fs, path::Path, process::Command};

use tempfile::{TempDir, tempdir};

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn write(repo: &Path, path: &str, contents: &str) {
    let path = repo.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn version(repo: &Path, version: &str) {
    write(
        repo,
        "Cargo.toml",
        &format!(
            "[package]\nname = \"vector\"\nversion = \"{version}\"\nedition = \"2021\"\n[workspace]\n"
        ),
    );
    write(
        repo,
        "Cargo.lock",
        &format!("version = 4\n[[package]]\nname = \"vector\"\nversion = \"{version}\"\n"),
    );
}

fn commit(repo: &Path) -> String {
    git(repo, &["add", "."]);
    git(repo, &["commit", "-m", "fixture"]);
    git(repo, &["rev-parse", "HEAD"])
}

fn preparation() -> (TempDir, String) {
    let temp = tempdir().unwrap();
    let repo = temp.path();
    git(repo, &["init", "-b", "master"]);
    git(repo, &["config", "core.hooksPath", "/dev/null"]);
    git(repo, &["config", "commit.gpgsign", "false"]);
    git(repo, &["config", "user.name", "Release test"]);
    git(repo, &["config", "user.email", "release@example.invalid"]);
    write(repo, "src/lib.rs", "");
    version(repo, "0.59.0-dev");
    let base = commit(repo);
    git(repo, &["switch", "-c", "release/prepare-v0.59.0"]);
    version(repo, "0.59.0");
    write(
        repo,
        "website/cue/reference/releases/0.59.0.cue",
        "version: \"0.59.0\"\n",
    );
    commit(repo);
    (temp, base)
}

fn check(repo: &Path, base: &str, success: bool) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_vdev"))
        .args([
            "release",
            "workflow",
            "pr-check",
            "--base-sha",
            base,
            "--head-ref",
            "release/prepare-v0.59.0",
        ])
        .current_dir(repo)
        .output()
        .unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.success(), success, "{stderr}");
    stderr
}

#[test]
fn release_preparation_requires_its_frozen_base() {
    let (temp, base) = preparation();
    let repo = temp.path();
    check(repo, &base, true);

    git(repo, &["switch", "master"]);
    write(repo, "README.md", "A concurrent change\n");
    let updated = commit(repo);
    git(repo, &["switch", "release/prepare-v0.59.0"]);
    git(repo, &["merge", "--no-edit", "master"]);

    let error = check(repo, &updated, false);
    assert!(error.contains("release PR must contain exactly one non-merge commit"));
}

#[test]
fn release_preparation_rejects_source_changes() {
    let (temp, base) = preparation();
    let repo = temp.path();
    write(repo, "src/lib.rs", "pub fn unreviewed() {}\n");
    git(repo, &["add", "."]);
    git(repo, &["commit", "--amend", "--no-edit"]);

    let error = check(repo, &base, false);
    assert!(error.contains("unexpected release preparation file: src/lib.rs"));
}

#[test]
fn release_preparation_rejects_a_merge_commit() {
    let (temp, base) = preparation();
    let repo = temp.path();
    git(repo, &["switch", "master"]);
    git(
        repo,
        &["merge", "--no-ff", "--no-edit", "release/prepare-v0.59.0"],
    );

    let error = check(repo, &base, false);
    assert!(error.contains("release PR must contain exactly one non-merge commit"));
}

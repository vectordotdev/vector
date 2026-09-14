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
    write(
        repo,
        ".github/release-state.json",
        r#"{"schema_version":1,"status":"development","version":"0.59.0-dev"}"#,
    );
    let base = commit(repo);
    git(repo, &["switch", "-c", "release/prepare-v0.59.0"]);
    version(repo, "0.59.0");
    write(
        repo,
        ".github/release-state.json",
        &format!(
            r#"{{"schema_version":1,"status":"prepared","version":"0.59.0","prepared_from":"{base}"}}"#
        ),
    );
    write(
        repo,
        "website/cue/reference/releases/0.59.0.cue",
        "version: \"0.59.0\"\n",
    );
    commit(repo);
    (temp, base)
}

fn check(repo: &Path, base: &str, branch: &str, success: bool) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_vdev"))
        .args([
            "release",
            "workflow",
            "pr-check",
            "--base-sha",
            base,
            "--head-ref",
            branch,
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
    check(repo, &base, "release/prepare-v0.59.0", true);
    git(repo, &["switch", "master"]);
    write(repo, "README.md", "A concurrent change\n");
    let updated = commit(repo);
    git(repo, &["switch", "release/prepare-v0.59.0"]);
    git(repo, &["merge", "--no-edit", "master"]);
    let error = check(repo, &updated, "release/prepare-v0.59.0", false);
    assert!(error.contains("release state prepared_from does not match"));
}

#[test]
fn release_transitions_accept_generated_dependencies_but_not_source_changes() {
    let (temp, base) = preparation();
    let repo = temp.path();
    write(repo, "docs/generated/vrl-functions.json", "{}\n");
    write(
        repo,
        "LICENSE-3rdparty.csv",
        "Component,Origin,License,Copyright\n",
    );
    let release = commit(repo);
    check(repo, &base, "release/prepare-v0.59.0", true);

    version(repo, "0.60.0-dev");
    write(
        repo,
        ".github/release-state.json",
        &format!(
            r#"{{"schema_version":1,"status":"development","version":"0.60.0-dev","last_release":{{"version":"0.59.0","tag":"v0.59.0","commit":"{release}"}}}}"#
        ),
    );
    write(
        repo,
        "docs/generated/vrl-functions.json",
        "{\"updated\":true}\n",
    );
    write(
        repo,
        "LICENSE-3rdparty.csv",
        "Component,Origin,License,Copyright\nvrl,git,MIT,VRL\n",
    );
    commit(repo);
    check(repo, &release, "release/housekeeping-v0.59.0", true);

    write(repo, "src/lib.rs", "pub fn unreviewed() {}\n");
    commit(repo);
    let error = check(repo, &release, "release/housekeeping-v0.59.0", false);
    assert!(error.contains("unexpected housekeeping files"));
}

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
    let vrl = if version.ends_with("-dev") {
        r#"git = "https://github.com/vectordotdev/vrl.git", branch = "main""#
    } else {
        r#"version = "0.28.0""#
    };
    write(
        repo,
        "Cargo.toml",
        &format!(
            "[package]\nname = \"vector\"\nversion = \"{version}\"\nedition = \"2021\"\n[dependencies]\nvrl = {{ workspace = true }}\n[workspace.dependencies]\nvrl = {{ {vrl} }}\n"
        ),
    );
    write(
        repo,
        "Cargo.lock",
        &format!(
            "version = 4\n[[package]]\nname = \"vector\"\nversion = \"{version}\"\ndependencies = [\"vrl\"]\n[[package]]\nname = \"vrl\"\nversion = \"0.28.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n"
        ),
    );
}

fn commit(repo: &Path) -> String {
    git(repo, &["add", "."]);
    git(repo, &["commit", "-m", "fixture"]);
    git(repo, &["rev-parse", "HEAD"])
}

fn preparation() -> (TempDir, String) {
    preparation_with_breaking_changes(false)
}

fn preparation_with_breaking_changes(breaking: bool) -> (TempDir, String) {
    let temp = tempdir().unwrap();
    let repo = temp.path();
    git(repo, &["init", "-b", "master"]);
    git(repo, &["config", "core.hooksPath", "/dev/null"]);
    git(repo, &["config", "commit.gpgsign", "false"]);
    git(repo, &["config", "user.name", "Release test"]);
    git(repo, &["config", "user.email", "release@example.invalid"]);
    write(repo, "src/lib.rs", "");
    // Resolve a tiny registry dependency offline while exercising real Cargo metadata.
    write(
        repo,
        ".cargo/config.toml",
        "[source.crates-io]\nreplace-with = \"fixture\"\n[source.fixture]\ndirectory = \".git/vendor\"\n",
    );
    write(
        repo,
        ".git/vendor/vrl/Cargo.toml",
        "[package]\nname = \"vrl\"\nversion = \"0.28.0\"\nedition = \"2021\"\n",
    );
    write(repo, ".git/vendor/vrl/src/lib.rs", "");
    write(
        repo,
        ".git/vendor/vrl/.cargo-checksum.json",
        r#"{"files":{},"package":null}"#,
    );
    write(
        repo,
        "website/cue/reference/administration/interfaces/kubectl.cue",
        "version: \"0.58.0\"\n",
    );
    if breaking {
        write(repo, "changelog.d/change.breaking.md", "Breaking change\n");
    }
    write(
        repo,
        "distribution/install.sh",
        "VECTOR_VERSION=\"${VECTOR_VERSION:-\"0.58.0\"}\"\n",
    );
    write(
        repo,
        "website/cue/reference/releases/0.58.0.cue",
        "version: \"0.58.0\"\n",
    );
    version(repo, "0.59.0-dev");
    // Legacy release without a corresponding releases/*.cue file.
    write(
        repo,
        "website/cue/reference/versions.cue",
        "versions: [\n\"0.1.0\",\n]\n",
    );
    let base = commit(repo);
    git(repo, &["switch", "-c", "prepare-v-0-59-0-website"]);
    version(repo, "0.59.0");
    write(
        repo,
        "website/cue/reference/administration/interfaces/kubectl.cue",
        "version: \"0.59.0\"\n",
    );
    if breaking {
        git(repo, &["rm", "changelog.d/change.breaking.md"]);
        write(
            repo,
            "website/content/en/highlights/2026-09-16-0-59-0-upgrade-guide.md",
            "Migration instructions\n",
        );
    }
    write(
        repo,
        "distribution/install.sh",
        "VECTOR_VERSION=\"${VECTOR_VERSION:-\"0.59.0\"}\"\n",
    );
    write(
        repo,
        "website/cue/reference/releases/0.59.0.cue",
        "version: \"0.59.0\"\n",
    );
    write(
        repo,
        "website/content/en/releases/0.59.0.md",
        "Release notes\n",
    );
    write(
        repo,
        "website/cue/reference/versions.cue",
        "package metadata\n\nversions: [string, ...string] & [\n\t\"0.59.0\",\n\t\"0.58.0\",\n\t\"0.1.0\",\n]\n",
    );
    commit(repo);
    (temp, base)
}

fn check(repo: &Path, base: &str, success: bool) -> String {
    check_with_args(repo, base, &[], success)
}

fn check_with_args(repo: &Path, base: &str, args: &[&str], success: bool) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_vdev"))
        .args([
            "release",
            "workflow",
            "pr-check",
            "--base-sha",
            base,
            "--head-ref",
            "prepare-v-0-59-0-website",
        ])
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.success(), success, "{stderr}");
    stderr
}

fn prepare_check(repo: &Path, success: bool) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_vdev"))
        .args([
            "release",
            "workflow",
            "prepare-check",
            "--version",
            "0.59.0",
            "--bot-app",
            "release-bot",
            "--repository",
            "vectordotdev/vector",
        ])
        .current_dir(repo)
        .output()
        .unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.success(), success, "{stderr}");
    stderr
}

#[test]
fn release_preparation_accepts_review_commits() {
    let (temp, base) = preparation();
    let repo = temp.path();
    write(
        repo,
        "website/cue/reference/releases/0.59.0.cue",
        "version: \"0.59.0\"\ndescription: \"Reviewed release notes\"\n",
    );
    commit(repo);
    write(
        repo,
        "website/content/en/releases/0.59.0.md",
        "Corrected release date\n",
    );
    commit(repo);
    check(repo, &base, true);

    // Extra commits do not bypass validation of the complete release diff.
    write(repo, "src/lib.rs", "pub fn unexpected() {}\n");
    commit(repo);
    assert!(check(repo, &base, false).contains("unexpected release preparation file: src/lib.rs"));
}

#[test]
fn release_preparation_retry_requires_the_requested_vrl_pin() {
    let (temp, base) = preparation();
    let repo = temp.path();
    check_with_args(repo, &base, &["--expected-vrl-version", "0.28.0"], true);
    let error = check_with_args(repo, &base, &["--expected-vrl-version", "0.28.1"], false);
    assert!(error.contains("existing preparation branch pins VRL to 0.28.0, but requested 0.28.1"));
}

#[test]
fn release_preparation_requires_a_guide_when_the_base_has_breaking_changes() {
    let (temp, base) = preparation_with_breaking_changes(true);
    let repo = temp.path();
    check(repo, &base, true);
    git(
        repo,
        &[
            "rm",
            "website/content/en/highlights/2026-09-16-0-59-0-upgrade-guide.md",
        ],
    );
    git(repo, &["commit", "--amend", "--no-edit"]);
    assert!(
        check(repo, &base, false).contains("breaking releases require a generated upgrade guide")
    );
}

#[test]
fn release_preparation_rejects_unrelated_kubectl_changes() {
    let (temp, base) = preparation();
    let repo = temp.path();
    write(
        repo,
        "website/cue/reference/administration/interfaces/kubectl.cue",
        "unrelated: \"change\"\n",
    );
    git(repo, &["add", "."]);
    git(repo, &["commit", "--amend", "--no-edit"]);
    assert!(
        check(repo, &base, false)
            .contains("kubectl.cue may only contain release version substitutions")
    );
}

#[test]
fn release_preparation_rejects_invalid_versions_index() {
    for contents in [
        "versions: [\"0.59.0\"]\n",
        "not valid CUE",
        "package metadata\n\nversions: [string, ...string] & [\n\t\"0.59.0\",\n\t\"0.58.0\",\n]\n",
        "package metadata\n\nversions: [string, ...string] & [\n\t\"0.60.0\",\n\t\"0.59.0\",\n\t\"0.58.0\",\n\t\"0.1.0\",\n]\n",
    ] {
        let (temp, base) = preparation();
        let repo = temp.path();
        write(repo, "website/cue/reference/versions.cue", contents);
        git(repo, &["add", "."]);
        git(repo, &["commit", "--amend", "--no-edit"]);
        assert!(check(repo, &base, false).contains("versions.cue must match the generated index"));
    }
}

#[test]
fn release_preparation_requires_a_stable_vrl_pin() {
    for requirement in ["*", "^0.28.0", "0.28", "0.28.0-rc.1", "0.28.0+build"] {
        let (temp, base) = preparation();
        let repo = temp.path();
        let manifest = fs::read_to_string(repo.join("Cargo.toml")).unwrap();
        write(repo, "Cargo.toml", &manifest.replace("0.28.0", requirement));
        git(repo, &["add", "."]);
        git(repo, &["commit", "--amend", "--no-edit"]);
        assert!(check(repo, &base, false).contains("VRL version"));
    }
}

#[test]
fn release_preparation_requires_matching_registry_vrl_in_lockfile() {
    for (old, new) in [
        ("0.28.0", "0.28.1"),
        (
            "registry+https://github.com/rust-lang/crates.io-index",
            "git+https://github.com/vectordotdev/vrl.git#abc",
        ),
    ] {
        let (temp, base) = preparation();
        let repo = temp.path();
        let lock = fs::read_to_string(repo.join("Cargo.lock")).unwrap();
        write(repo, "Cargo.lock", &lock.replace(old, new));
        git(repo, &["add", "."]);
        git(repo, &["commit", "--amend", "--no-edit"]);
        assert!(check(repo, &base, false).contains("Cargo.lock must resolve VRL"));
    }
}

#[test]
fn release_preparation_requires_its_frozen_base() {
    let (temp, base) = preparation();
    let repo = temp.path();
    check(repo, &base, true);

    git(repo, &["switch", "master"]);
    write(repo, "README.md", "A concurrent change\n");
    let updated = commit(repo);
    git(repo, &["switch", "prepare-v-0-59-0-website"]);
    assert!(check(repo, &updated, false).contains("release PR must descend from frozen base"));
    git(repo, &["merge", "--no-edit", "master"]);

    let error = check(repo, &updated, false);
    assert!(error.contains("release PR must contain only non-merge commits"));
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
fn release_preparation_rejects_source_files_renamed_into_allowed_paths() {
    let (temp, base) = preparation();
    let repo = temp.path();
    fs::create_dir_all(repo.join("docs/generated")).unwrap();
    git(repo, &["mv", "src/lib.rs", "docs/generated/lib.rs"]);
    git(repo, &["commit", "--amend", "--no-edit"]);

    let error = check(repo, &base, false);
    assert!(error.contains("unexpected release preparation file: src/lib.rs"));
}

#[test]
fn release_preparation_rejects_changes_to_historical_release_metadata() {
    let (temp, base) = preparation();
    let repo = temp.path();
    write(
        repo,
        "website/cue/reference/releases/0.58.0.cue",
        "version: \"corrupted\"\n",
    );
    git(repo, &["add", "."]);
    git(repo, &["commit", "--amend", "--no-edit"]);

    let error = check(repo, &base, false);
    assert!(error.contains(
        "unexpected release preparation file: website/cue/reference/releases/0.58.0.cue"
    ));
}

#[test]
fn release_preparation_rejects_deleting_required_files() {
    let (temp, base) = preparation();
    let repo = temp.path();
    git(repo, &["rm", "distribution/install.sh"]);
    git(repo, &["commit", "--amend", "--no-edit"]);

    let error = check(repo, &base, false);
    assert!(error.contains("release preparation cannot delete file: distribution/install.sh"));
}

#[test]
fn release_preparation_rejects_uncommitted_generation_output() {
    let (temp, base) = preparation();
    let repo = temp.path();
    git(
        repo,
        &[
            "rm",
            "--cached",
            "website/cue/reference/releases/0.59.0.cue",
        ],
    );
    git(repo, &["commit", "--amend", "--no-edit"]);

    let error = check(repo, &base, false);
    assert!(error.contains("working tree must be clean"));
}

#[test]
fn release_preparation_rejects_a_remote_only_release_tag() {
    let temp = tempdir().unwrap();
    let repo = temp.path();
    git(repo, &["init", "-b", "master"]);
    git(repo, &["config", "core.hooksPath", "/dev/null"]);
    git(repo, &["config", "commit.gpgsign", "false"]);
    git(repo, &["config", "user.name", "Release test"]);
    git(repo, &["config", "user.email", "release@example.invalid"]);
    version(repo, "0.59.0-dev");
    commit(repo);

    let remote = tempdir().unwrap();
    git(remote.path(), &["init", "--bare"]);
    git(
        repo,
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    );
    git(repo, &["tag", "v0.59.0"]);
    git(repo, &["push", "origin", "refs/tags/v0.59.0"]);
    git(repo, &["tag", "--delete", "v0.59.0"]);

    let error = prepare_check(repo, false);
    assert!(error.contains("tag v0.59.0 already exists"));
}

#[test]
fn release_preparation_rejects_a_merge_commit() {
    let (temp, base) = preparation();
    let repo = temp.path();
    git(repo, &["switch", "master"]);
    git(
        repo,
        &["merge", "--no-ff", "--no-edit", "prepare-v-0-59-0-website"],
    );

    let error = check(repo, &base, false);
    assert!(error.contains("release PR must contain only non-merge commits"));

    // Reject merges anywhere in the preparation history, not just at HEAD.
    write(
        repo,
        "website/content/en/releases/0.59.0.md",
        "Reviewed notes\n",
    );
    commit(repo);
    assert!(check(repo, &base, false).contains("release PR must contain only non-merge commits"));
}

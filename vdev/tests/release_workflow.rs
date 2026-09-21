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

#[cfg(unix)]
mod housekeeping {
    use super::*;
    use serde_json::json;
    use std::{env, os::unix::fs::PermissionsExt as _, process::Output};

    struct Fixture {
        repo: TempDir,
        _remote: TempDir,
        release: String,
    }

    impl Fixture {
        fn new() -> Self {
            let (repo, _) = preparation();
            let release = git(repo.path(), &["rev-parse", "HEAD"]);
            git(repo.path(), &["tag", "v0.59.0"]);
            let remote = tempdir().unwrap();
            git(remote.path(), &["init", "--bare"]);
            git(
                repo.path(),
                &["remote", "add", "origin", remote.path().to_str().unwrap()],
            );
            git(repo.path(), &["push", "origin", "HEAD:refs/heads/master"]);

            // Real Cargo git resolution, redirected to a tiny local VRL repository.
            let vrl = repo.path().join(".git/vrl-source");
            write(
                &vrl,
                "Cargo.toml",
                "[package]\nname = \"vrl\"\nversion = \"0.28.0\"\nedition = \"2021\"\n",
            );
            write(&vrl, "src/lib.rs", "");
            git(&vrl, &["init", "-b", "main"]);
            for (key, value) in [
                ("commit.gpgsign", "false"),
                ("core.hooksPath", "/dev/null"),
                ("user.name", "Release test"),
                ("user.email", "release@example.invalid"),
            ] {
                git(&vrl, &["config", key, value]);
            }
            commit(&vrl);
            git(
                repo.path(),
                &[
                    "config",
                    "--file",
                    ".git/test-gitconfig",
                    &format!("url.file://{}.insteadOf", vrl.display()),
                    "https://github.com/vectordotdev/vrl.git",
                ],
            );
            write(repo.path(), ".git/associated-prs.json", &json!([[{
                "merged_at": "2026-09-21T12:00:00Z",
                "merge_commit_sha": release,
                "user": {"login": "vectordotdev-bot[bot]"},
                "base": {"ref": "master", "repo": {"full_name": "vectordotdev/vector"}},
                "head": {"ref": "prepare-v-0-59-0-website", "repo": {"full_name": "vectordotdev/vector"}}
            }]]).to_string());
            write(repo.path(), ".git/pr-list.json", "[]");
            write(
                repo.path(),
                ".git/test-bin/gh",
                "#!/bin/sh\ncase \"$1\" in\napi) cat .git/associated-prs.json ;;\npr) cat .git/pr-list.json ;;\n*) exit 1 ;;\nesac\n",
            );
            fs::set_permissions(
                repo.path().join(".git/test-bin/gh"),
                fs::Permissions::from_mode(0o755),
            )
            .unwrap();
            Self {
                repo,
                _remote: remote,
                release,
            }
        }

        fn run(&self, args: &[&str], success: bool) -> Output {
            let repo = self.repo.path();
            let path = env::join_paths(
                std::iter::once(repo.join(".git/test-bin"))
                    .chain(env::split_paths(&env::var_os("PATH").unwrap())),
            )
            .unwrap();
            let output = Command::new(env!("CARGO_BIN_EXE_vdev"))
                .args(["release", "workflow"])
                .args(args)
                .env("PATH", path)
                .env("GIT_CONFIG_GLOBAL", repo.join(".git/test-gitconfig"))
                .env("CARGO_NET_GIT_FETCH_WITH_CLI", "true")
                // Do not cache the redirected VRL repository in the developer's Cargo home.
                .env("CARGO_HOME", repo.join(".git/cargo-home"))
                .env_remove("GITHUB_OUTPUT")
                .env_remove("GITHUB_STEP_SUMMARY")
                .current_dir(repo)
                .output()
                .unwrap();
            assert_eq!(
                output.status.success(),
                success,
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            output
        }

        fn state(&self, success: bool) -> String {
            let output = self.run(
                &[
                    "housekeeping-check",
                    "--tag",
                    "v0.59.0",
                    "--release-commit",
                    &self.release,
                    "--repository",
                    "vectordotdev/vector",
                ],
                success,
            );
            if success {
                String::from_utf8(output.stdout).unwrap()
            } else {
                String::from_utf8(output.stderr).unwrap()
            }
        }

        fn prepare(&self) {
            self.run(
                &[
                    "housekeeping-prepare",
                    "--version",
                    "0.59.0",
                    "--release-commit",
                    &self.release,
                ],
                true,
            );
        }

        fn validate(&self, success: bool) -> String {
            let output = self.run(
                &[
                    "pr-check",
                    "--base-sha",
                    &self.release,
                    "--head-ref",
                    "release/housekeeping-v0.59.0",
                ],
                success,
            );
            String::from_utf8(output.stderr).unwrap()
        }
    }

    #[test]
    fn generates_and_validates_the_next_development_version() {
        let fixture = Fixture::new();
        fixture.prepare();
        let repo = fixture.repo.path();
        let manifest = fs::read_to_string(repo.join("Cargo.toml")).unwrap();
        assert!(manifest.contains("version = \"0.60.0-dev\""));
        assert!(manifest.contains("git = \"https://github.com/vectordotdev/vrl.git\""));
        assert!(manifest.contains("branch = \"main\""));
        let lock = fs::read_to_string(repo.join("Cargo.lock")).unwrap();
        assert!(lock.contains("git+https://github.com/vectordotdev/vrl.git?branch=main#"));
        write(repo, "LICENSE-3rdparty.csv", "Refreshed licenses\n");
        write(repo, "docs/generated/vrl.json", "{}\n");
        commit(repo);
        fixture.validate(true);
        assert!(git(repo, &["status", "--porcelain"]).is_empty());
    }

    #[test]
    fn rejects_unrelated_changes_and_stale_lockfiles() {
        for (path, contents, error) in [
            (
                "src/lib.rs",
                "pub fn unrelated() {}\n",
                "unexpected housekeeping file",
            ),
            ("Cargo.lock", "version = 4\n", "lock file"),
            (
                "Cargo.toml",
                "[package]\nname = \"vector\"\nversion = \"0.60.0-dev\"\nedition = \"2024\"\n",
                "housekeeping may only bump",
            ),
        ] {
            let fixture = Fixture::new();
            fixture.prepare();
            write(fixture.repo.path(), path, contents);
            commit(fixture.repo.path());
            assert!(fixture.validate(false).contains(error), "{path}");
        }
    }

    #[test]
    fn retries_reuse_the_published_branch_and_skip_open_or_completed_prs() {
        let fixture = Fixture::new();
        let output = fixture.state(true);
        assert!(output.contains("branch=release/housekeeping-v0.59.0\n"));
        assert!(output.contains("resume=false\nskip=false\n"));
        let repo = fixture.repo.path();
        fixture.prepare();
        let housekeeping = commit(repo);
        git(
            repo,
            &[
                "push",
                "origin",
                "HEAD:refs/heads/release/housekeeping-v0.59.0",
            ],
        );
        git(repo, &["switch", "--detach", &fixture.release]);
        assert!(fixture.state(true).contains("resume=true\nskip=false\n"));
        write(
            repo,
            ".git/pr-list.json",
            r#"[{"isCrossRepository":false,"url":"https://example.invalid/pr/1"}]"#,
        );
        assert!(fixture.state(true).contains("skip=true\n"));
        git(repo, &["switch", "--detach", &housekeeping]);
        assert!(fixture.state(true).contains("skip=true\n"));
    }

    #[test]
    fn requires_the_published_tag_and_frozen_release_commit() {
        let fixture = Fixture::new();
        let repo = fixture.repo.path();
        write(repo, ".git/associated-prs.json", "[[]]");
        assert!(
            fixture
                .state(false)
                .contains("expected one merged bot preparation PR")
        );
        write(repo, "README.md", "A commit during the freeze\n");
        commit(repo);
        assert!(
            fixture
                .state(false)
                .contains("master must still match the published release commit")
        );
        git(repo, &["-c", "tag.gpgsign=false", "tag", "-f", "v0.59.0"]);
        assert!(
            fixture
                .state(false)
                .contains("release tag does not match the published commit")
        );
    }

    #[test]
    fn skips_non_minor_releases_and_rejects_generation_from_the_wrong_commit() {
        let fixture = Fixture::new();
        for tag in ["v0.59.1", "v0.60.0-rc.1", "v0.59.0+build"] {
            let output = fixture.run(
                &[
                    "housekeeping-check",
                    "--tag",
                    tag,
                    "--release-commit",
                    &fixture.release,
                    "--repository",
                    "vectordotdev/vector",
                ],
                true,
            );
            assert_eq!(String::from_utf8(output.stdout).unwrap(), "skip=true\n");
        }
        let before = git(fixture.repo.path(), &["rev-parse", "HEAD^"]);
        fixture.run(
            &[
                "housekeeping-prepare",
                "--version",
                "0.59.0",
                "--release-commit",
                &before,
            ],
            false,
        );
        assert!(git(fixture.repo.path(), &["status", "--porcelain"]).is_empty());
    }
}

#[cfg(unix)]
mod autotag {
    use super::*;
    use serde_json::{Value, json};
    use std::{env, os::unix::fs::PermissionsExt as _, process::Output};

    fn approved_pr(sha: &str) -> Value {
        json!({
            "merged_at": "2026-09-17T12:00:00Z",
            "merge_commit_sha": sha,
            "user": {"login": "vectordotdev-bot[bot]"},
            "base": {"ref": "master", "repo": {"full_name": "vectordotdev/vector"}},
            "head": {"ref": "prepare-v-0-59-0-website", "repo": {"full_name": "vectordotdev/vector"}}
        })
    }

    fn check_autotag(repo: &Path, base: &str, sha: &str, prs: Value) -> Output {
        write(repo, ".git/associated-prs.json", &prs.to_string());
        write(
            repo,
            ".git/test-bin/gh",
            "#!/bin/sh\ncat .git/associated-prs.json\n",
        );
        fs::set_permissions(
            repo.join(".git/test-bin/gh"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        let path = env::join_paths(
            std::iter::once(repo.join(".git/test-bin"))
                .chain(env::split_paths(&env::var_os("PATH").unwrap())),
        )
        .unwrap();
        Command::new(env!("CARGO_BIN_EXE_vdev"))
            .args([
                "release",
                "workflow",
                "autotag-check",
                "--before-sha",
                base,
                "--sha",
                sha,
                "--repository",
                "vectordotdev/vector",
            ])
            .env("PATH", path)
            .env_remove("GITHUB_OUTPUT")
            .current_dir(repo)
            .output()
            .unwrap()
    }

    #[test]
    fn accepts_only_the_approved_squash_commit() {
        let (temp, base) = preparation();
        let repo = temp.path();
        let sha = git(repo, &["rev-parse", "HEAD"]);
        let result = check_autotag(repo, &base, &sha, json!([[approved_pr(&sha)]]));
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let output = String::from_utf8(result.stdout).unwrap();
        assert!(output.contains("tag_required=true\ntag=v0.59.0\nrelease_branch=v0.59\n"));

        write(
            repo,
            "website/content/en/releases/0.59.0.md",
            "Another commit\n",
        );
        let sha = commit(repo);
        let result = check_autotag(repo, &base, &sha, json!([[approved_pr(&sha)]]));
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("single squash-merge commit"));
    }

    #[test]
    fn rejects_unapproved_or_unrelated_prs() {
        let (temp, base) = preparation();
        let repo = temp.path();
        let sha = git(repo, &["rev-parse", "HEAD"]);
        for (pointer, value) in [
            ("/merged_at", Value::Null),
            ("/merge_commit_sha", json!(base)),
            ("/user/login", json!("other-bot[bot]")),
            ("/base/ref", json!("website")),
            ("/head/ref", json!("other-branch")),
            ("/head/repo/full_name", json!("someone/vector")),
            ("/head/repo", Value::Null),
        ] {
            let mut pr = approved_pr(&sha);
            *pr.pointer_mut(pointer).unwrap() = value;
            let result = check_autotag(repo, &base, &sha, json!([[pr]]));
            assert!(!result.status.success(), "accepted invalid {pointer}");
            assert!(
                String::from_utf8_lossy(&result.stderr)
                    .contains("expected one merged bot preparation PR")
            );
        }
        for prs in [
            json!([[]]),
            json!([[approved_pr(&sha)], [approved_pr(&sha)]]),
        ] {
            let result = check_autotag(repo, &base, &sha, prs);
            assert!(!result.status.success());
        }
    }

    #[test]
    fn skips_development_unchanged_and_patch_versions() {
        for new_version in ["0.60.0-dev", "0.59.0", "0.59.1"] {
            let (temp, _) = preparation();
            let repo = temp.path();
            let before = git(repo, &["rev-parse", "HEAD"]);
            version(repo, new_version);
            write(repo, "README.md", "Ordinary change\n");
            let sha = commit(repo);
            let result = check_autotag(repo, &before, &sha, json!("API must not be needed"));
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert_eq!(
                String::from_utf8(result.stdout).unwrap(),
                "tag_required=false\n"
            );
        }
    }

    #[test]
    fn revalidates_the_merged_release_files() {
        let (temp, base) = preparation();
        let repo = temp.path();
        write(repo, "src/lib.rs", "pub fn unexpected() {}\n");
        git(repo, &["add", "."]);
        git(repo, &["commit", "--amend", "--no-edit"]);
        let sha = git(repo, &["rev-parse", "HEAD"]);
        let result = check_autotag(repo, &base, &sha, json!([[approved_pr(&sha)]]));
        assert!(!result.status.success());
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("unexpected release preparation file")
        );
    }

    #[test]
    fn rejects_a_mismatched_checkout() {
        let (temp, base) = preparation();
        let result = check_autotag(temp.path(), &base, &base, json!([[]]));
        assert!(!result.status.success());
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("checkout must match the release SHA")
        );
    }
}

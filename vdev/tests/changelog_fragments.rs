use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use git2::{IndexAddOption, Repository, Signature};
use tempfile::{TempDir, tempdir};

const NOTICE: &str =
    "---\nwhat: Legacy option\ndeprecated_since: \"0.62.0\"\n---\n\nUse the replacement option.\n";

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
            "[package]\nname = \"changelog-fixture\"\nversion = \"{version}\"\n\n[features]\n"
        ),
    );
}

// These commits are disposable fixture data; no repository signing configuration is changed.
fn commit(root: &Path) -> git2::Oid {
    let repo = Repository::open(root).unwrap();
    let mut index = repo.index().unwrap();
    index.add_all(["*"], IndexAddOption::DEFAULT, None).unwrap();
    index.update_all(["*"], None).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let parent = repo.head().ok().map(|head| head.peel_to_commit().unwrap());
    let signature = Signature::now("Changelog test", "changelog@example.invalid").unwrap();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "fixture",
        &tree,
        &parent.iter().collect::<Vec<_>>(),
    )
    .unwrap()
}

fn fixture() -> TempDir {
    let temp = tempdir().unwrap();
    let repo = Repository::init(temp.path()).unwrap();
    // Do not start a host-configured background monitor for disposable repositories.
    repo.config()
        .unwrap()
        .set_bool("core.fsmonitor", false)
        .unwrap();
    version(temp.path(), "0.62.0-dev");
    write(
        temp.path(),
        "changelog.d/README.md",
        "Changelog fragments.\n",
    );
    write(
        temp.path(),
        "deprecation.d/README.md",
        "Deprecation notices.\n",
    );
    write(
        temp.path(),
        "deprecation.d/existing.md",
        &NOTICE.replace("0.62.0", "0.61.0"),
    );
    write(
        temp.path(),
        "changelog.d/existing.fix.md",
        "An existing fix.\n\nauthors: pront\n",
    );
    let base = commit(temp.path());
    repo.reference("refs/remotes/origin/master", base, true, "fixture")
        .unwrap();
    temp
}

fn announce(repo: &Path) {
    write(
        repo,
        "deprecation.d/new-option.md",
        &NOTICE.replace("Legacy option", "Another legacy option"),
    );
}

fn check(repo: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vdev"))
        .args(["check", "changelog-fragments"])
        .current_dir(repo)
        .output()
        .unwrap()
}

fn fails(repo: &Path, message: &str) {
    let output = check(repo);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "unexpected success: {stderr}");
    assert!(stderr.contains(message), "{stderr}");
}

#[test]
fn new_deprecation_announcement_satisfies_the_changelog_requirement() {
    let temp = fixture();
    announce(temp.path());
    commit(temp.path());
    let output = check(temp.path());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn announcements_must_match_the_exact_target_release() {
    for (current, since, accepted) in [
        ("0.62.0-dev", "0.61.0", false),
        ("0.62.0-dev", "0.62.1", false),
        ("0.62.0-dev", "0.63.0", false),
        ("0.62.0-dev", "0.62", true),
        ("0.62.0-dev", "0.62.0", true),
        ("0.62.0-rc.1+build.2", "0.62.0", true),
        ("0.62.0", "0.62.0", true),
        ("0.62.1-dev", "0.62", false),
        ("0.62.1-dev", "0.62.0", false),
        ("0.62.1-dev", "0.62.1", true),
    ] {
        let temp = fixture();
        version(temp.path(), current);
        write(
            temp.path(),
            "deprecation.d/new-option.md",
            &NOTICE
                .replace("Legacy option", "Another legacy option")
                .replace("0.62.0", since),
        );
        commit(temp.path());
        let output = check(temp.path());
        assert_eq!(
            output.status.success(),
            accepted,
            "{current} with deprecated_since {since}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if !accepted {
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("No changelog fragments detected")
            );
        }
    }
}

#[test]
fn only_new_active_notices_satisfy_the_requirement() {
    for path in [
        "deprecation.d/existing.md",
        "deprecation.d/README.md",
        "deprecation.d/nested/notice.md",
        "deprecation.d/notice.txt",
        "website/content/en/deprecations/enacted/notice.md",
    ] {
        let temp = fixture();
        write(
            temp.path(),
            path,
            &NOTICE.replace("Legacy option", "An edited or unrelated notice"),
        );
        commit(temp.path());
        fails(temp.path(), "No changelog fragments detected");
    }

    let temp = fixture();
    fs::rename(
        temp.path().join("deprecation.d/existing.md"),
        temp.path().join("deprecation.d/renamed.md"),
    )
    .unwrap();
    commit(temp.path());
    fails(temp.path(), "No changelog fragments detected");
}

#[test]
fn malformed_announcements_are_rejected() {
    for (notice, message) in [
        (
            "No frontmatter.\n".to_owned(),
            "must begin with YAML frontmatter",
        ),
        (
            NOTICE.replace("what: Legacy option\n", ""),
            "missing field `what`",
        ),
        (NOTICE.replace("Legacy option", "''"), "must not be empty"),
        (
            NOTICE
                .replace("Legacy option", "Another legacy option")
                .replace(
                    "deprecated_since:",
                    "removed_in: \"0.63.0\"\ndeprecated_since:",
                ),
            "sets removed_in",
        ),
    ] {
        let temp = fixture();
        write(temp.path(), "deprecation.d/new-option.md", &notice);
        commit(temp.path());
        fails(temp.path(), message);
    }
}

#[test]
fn an_announcement_does_not_waive_the_changelog_for_removals() {
    for enact in [false, true] {
        let temp = fixture();
        announce(temp.path());
        fs::remove_file(temp.path().join("deprecation.d/existing.md")).unwrap();
        if enact {
            write(
                temp.path(),
                "website/content/en/deprecations/enacted/existing.md",
                &NOTICE.replace("0.62.0", "0.61.0").replace(
                    "deprecated_since:",
                    "removed_in: \"0.62.0\"\ndeprecated_since:",
                ),
            );
        }
        commit(temp.path());
        fails(temp.path(), "No changelog fragments detected");
    }
}

#[test]
fn an_announcement_does_not_skip_changelog_validation() {
    let temp = fixture();
    announce(temp.path());
    write(
        temp.path(),
        "changelog.d/existing.fix.md",
        "An edited fix with no authors line.\n",
    );
    commit(temp.path());
    fails(temp.path(), "last line must be 'authors:");
}

#[test]
fn a_changelog_fragment_still_satisfies_the_requirement() {
    let temp = fixture();
    write(
        temp.path(),
        "changelog.d/new.fix.md",
        "A new fix.\n\nauthors: pront\n",
    );
    commit(temp.path());
    let output = check(temp.path());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_does_not_count_as_a_new_notice() {
    let temp = fixture();
    std::os::unix::fs::symlink("existing.md", temp.path().join("deprecation.d/symlink.md"))
        .unwrap();
    commit(temp.path());
    fails(temp.path(), "No changelog fragments detected");
}

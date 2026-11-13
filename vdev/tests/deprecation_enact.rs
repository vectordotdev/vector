use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use tempfile::{TempDir, tempdir};

const NOTICE: &str = "---\nwhat: Legacy option\ndeprecated_since: \"0.62.0\"\n---\n\nReplace the legacy option with `new_option`.\n";
const ACTIVE: &str = "deprecation.d/legacy.md";
const ENACTED: &str = "website/content/en/deprecations/enacted/legacy.md";

fn fixture() -> TempDir {
    let temp = tempdir().unwrap();
    fs::create_dir(temp.path().join(".git")).unwrap();
    fs::create_dir(temp.path().join("deprecation.d")).unwrap();
    fs::write(temp.path().join(ACTIVE), NOTICE).unwrap();
    temp
}

fn enact(root: &Path, version: &str, allow_early_removal: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_vdev"));
    command
        .current_dir(root)
        .args(["deprecation", "enact", "legacy", "--version", version]);
    if allow_early_removal {
        command.arg("--allow-early-removal");
    }
    command.output().unwrap()
}

#[test]
fn early_removal_requires_an_explicit_override() {
    let temp = fixture();
    let root = temp.path();
    let output = enact(root, "0.63.0", false);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--allow-early-removal"));
    assert_eq!(fs::read_to_string(root.join(ACTIVE)).unwrap(), NOTICE);
    assert!(!root.join(ENACTED).exists());

    let output = enact(root, "0.63.0", true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!root.join(ACTIVE).exists());
    let notice = fs::read_to_string(root.join(ENACTED)).unwrap();
    assert!(notice.contains("removed_in: \"0.63.0\""));
    assert!(notice.ends_with("Replace the legacy option with `new_option`.\n"));

    let output = Command::new(env!("CARGO_BIN_EXE_vdev"))
        .current_dir(root)
        .args(["deprecation", "check"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn override_still_rejects_invalid_versions_without_changing_the_notice() {
    for (version, expected) in [
        ("0.61.0", "later minor release"),
        ("0.62.0", "later minor release"),
        ("0.62.1", "later minor release"),
        ("0.63.0-alpha", "prerelease or build metadata"),
        ("0.63.0+build", "prerelease or build metadata"),
        ("invalid", "invalid value"),
    ] {
        let temp = fixture();
        let root = temp.path();
        let output = enact(root, version, true);
        assert!(!output.status.success(), "{version}");
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains(expected), "{version}: {error}");
        assert_eq!(fs::read_to_string(root.join(ACTIVE)).unwrap(), NOTICE);
        assert!(!root.join(ENACTED).exists());
    }
}

#[test]
fn override_does_not_overwrite_an_existing_destination() {
    let temp = fixture();
    let root = temp.path();
    let destination = root.join(ENACTED);
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    let existing = "---\nwhat: Another option\ndeprecated_since: \"0.60.0\"\nremoved_in: \"0.61.0\"\n---\n\nKeep this notice.\n";
    fs::write(&destination, existing).unwrap();

    let output = enact(root, "0.63.0", true);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("already exists"));
    assert_eq!(fs::read_to_string(root.join(ACTIVE)).unwrap(), NOTICE);
    assert_eq!(fs::read_to_string(destination).unwrap(), existing);
}

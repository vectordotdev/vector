#![cfg(unix)]

use std::{
    env, fs,
    os::unix::fs::PermissionsExt as _,
    path::Path,
    process::{Command, Output},
};

use tempfile::TempDir;

const VARIANTS: [&str; 3] = [
    "vector-agent",
    "vector-aggregator",
    "vector-stateless-aggregator",
];

fn script(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn fixture() -> TempDir {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir(root.join(".git")).unwrap();
    fs::create_dir(root.join("nested")).unwrap();
    fs::create_dir(root.join("bin")).unwrap();
    for variant in VARIANTS {
        let destination = root.join("distribution/kubernetes").join(variant);
        fs::create_dir_all(&destination).unwrap();
        fs::write(
            destination.join("README.md"),
            "version 0.58.0 with the following `values.yaml`:\n",
        )
        .unwrap();
        fs::write(destination.join("old.yaml"), "old\n").unwrap();
    }
    script(
        &root.join("bin/helm"),
        r#"#!/bin/sh
set -eu
printf '<%s> ' "$@" >> "$MANIFEST_FIXTURE_ROOT/helm.log"
printf '\n' >> "$MANIFEST_FIXTURE_ROOT/helm.log"
case "$1" in
    repo) ;;
    search) printf '[{"version":"0.59.0"}]\n' ;;
    show) printf 'version: 0.59.0\n' ;;
    template)
        while [ "$1" != '--output-dir' ]; do shift; done
        output="$2"
        if [ -n "${MANIFEST_FAIL_VARIANT:-}" ]; then
            case "$output" in *"$MANIFEST_FAIL_VARIANT") exit 17;; esac
        fi
        mkdir -p "$output/vector/templates"
        printf '%s\n' '---' '# Source: vector/templates/service.yaml' 'kind: Service' > "$output/vector/templates/service.yaml"
        printf '%s\n' '# Source: vector/templates/empty.yaml' > "$output/vector/templates/empty.yaml"
        ;;
    *) exit 1 ;;
esac
"#,
    );
    script(
        &root.join("bin/yq"),
        r#"#!/bin/sh
set -eu
if [ "$2" = '. | length' ]; then
    case "$3" in */empty.yaml) printf '0\n';; *) printf '1\n';; esac
else
    if [ -n "${MANIFEST_FAIL_YQ:-}" ]; then exit 18; fi
    cat "$3"
fi
"#,
    );
    temp
}

fn command(root: &Path) -> Command {
    let mut paths = vec![root.join("bin")];
    paths.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
    let mut command = Command::new(env!("CARGO_BIN_EXE_vdev"));
    command
        .current_dir(root.join("nested"))
        .args(["build", "manifests"])
        .env("PATH", env::join_paths(paths).unwrap())
        .env("MANIFEST_FIXTURE_ROOT", root);
    command
}

fn succeeds(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn generates_all_variants_with_one_pinned_version() {
    for explicit in [false, true] {
        let temp = fixture();
        let root = temp.path();
        let mut command = command(root);
        if explicit {
            command.args(["--chart-version", "0.59.0"]);
        }
        succeeds(&command.output().unwrap());
        let log = fs::read_to_string(root.join("helm.log")).unwrap();
        assert_eq!(log.contains("<search>"), !explicit);
        assert_eq!(log.matches("<--version> <0.59.0>").count(), 4);
        for variant in VARIANTS {
            let destination = root.join("distribution/kubernetes").join(variant);
            assert!(!destination.join("old.yaml").exists());
            assert!(!destination.join("empty.yaml").exists());
            assert!(
                fs::read_to_string(destination.join("service.yaml"))
                    .unwrap()
                    .contains("Please re-run `make generate-kubernetes-manifests`")
            );
            assert!(
                fs::read_to_string(destination.join("kustomization.yaml"))
                    .unwrap()
                    .ends_with("resources:\n  - service.yaml\n")
            );
            assert!(
                fs::read_to_string(destination.join("README.md"))
                    .unwrap()
                    .contains("version 0.59.0 with the following")
            );
        }
        // Regeneration at the same version is allowed.
        succeeds(&command.output().unwrap());
    }
}

#[test]
fn rendering_failures_preserve_existing_manifests() {
    for (variable, value) in [
        ("MANIFEST_FAIL_VARIANT", "vector-stateless-aggregator"),
        ("MANIFEST_FAIL_YQ", "1"),
    ] {
        let temp = fixture();
        let output = command(temp.path()).env(variable, value).output().unwrap();
        assert!(!output.status.success());
        for variant in VARIANTS {
            let destination = temp.path().join("distribution/kubernetes").join(variant);
            assert_eq!(
                fs::read_to_string(destination.join("old.yaml")).unwrap(),
                "old\n"
            );
            assert!(
                fs::read_to_string(destination.join("README.md"))
                    .unwrap()
                    .contains("version 0.58.0")
            );
            assert!(!destination.join("service.yaml").exists());
        }
    }
}

#[test]
fn rejects_downgrades_before_rendering() {
    let temp = fixture();
    let output = command(temp.path())
        .args(["--chart-version", "0.57.0"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("OLDER"));
    let log = fs::read_to_string(temp.path().join("helm.log")).unwrap();
    assert!(!log.contains("<template>"));
    assert!(!log.contains("<show>"));
}

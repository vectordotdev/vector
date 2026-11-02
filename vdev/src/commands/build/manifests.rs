use std::{collections::BTreeMap, fs, path::Path, process::Command};

use anyhow::{Context, Result, ensure};
use semver::Version;
use serde::Deserialize;

use crate::{
    app::CommandExt as _,
    utils::{paths, semver as versions},
};

const VARIANTS: &[(&str, &str)] = &[
    ("vector-agent", "role: Agent\nservice:\n  enabled: false\n"),
    ("vector-aggregator", ""),
    (
        "vector-stateless-aggregator",
        "role: Stateless-Aggregator\n",
    ),
];

// Keep yq's formatting and comment preservation identical to the former script.
const CLEANUP: &str = "del(.metadata.labels.\"helm.sh/chart\") | del(.metadata.labels.\"app.kubernetes.io/managed-by\") | del(.metadata.annotations.checksum/*) | del(.spec.template.metadata.annotations.checksum/*)";

/// Build Kubernetes manifests from the latest or specified Helm chart.
#[derive(clap::Args, Debug)]
pub(super) struct Cli {
    /// Helm chart version to generate; defaults to the latest published chart.
    #[arg(long)]
    chart_version: Option<Version>,
}

#[derive(Deserialize)]
struct Chart {
    version: String,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let root = paths::find_repo_root()?;
        Command::new("helm")
            .args(["repo", "add", "vector", "https://helm.vector.dev"])
            .check_run()?;
        Command::new("helm").args(["repo", "update"]).check_run()?;

        let version = if let Some(version) = self.chart_version {
            version
        } else {
            let output = Command::new("helm")
                .args(["search", "repo", "vector/vector", "-o", "json"])
                .check_output()?;
            let charts: Vec<Chart> = serde_json::from_str(&output)?;
            let chart = charts
                .first()
                .context("No published Vector Helm chart found")?;
            versions::parse(&chart.version)?
        };
        let destination = root.join("distribution/kubernetes");
        let readme = fs::read_to_string(destination.join("vector-agent/README.md"))?;
        check_version(&version, &readme)?;
        let version = version.to_string();

        let chart = Command::new("helm")
            .args(["show", "chart", "vector/vector", "--version", &version])
            .check_output()?;
        let chart: Chart = serde_yaml::from_str(&chart)?;
        ensure!(
            versions::parse(&chart.version)? == versions::parse(&version)?,
            "Helm returned chart version {}, expected {version}",
            chart.version
        );
        info!("Generating manifests from chart version {version}");

        let temporary = tempfile::tempdir()?;
        let mut generated = Vec::new();
        for &(variant, values) in VARIANTS {
            let values_path = temporary.path().join(format!("values-{variant}.yaml"));
            fs::write(&values_path, values)?;
            let rendered = temporary.path().join(format!("generated_{variant}"));
            Command::new("helm")
                .args([
                    "template",
                    "--namespace",
                    "default",
                    "vector",
                    "vector/vector",
                    "--version",
                    &version,
                    "--values",
                ])
                .arg(&values_path)
                .arg("--output-dir")
                .arg(&rendered)
                .check_run()?;

            let mut manifests = read_manifests(&rendered.join("vector/templates"))?;
            manifests.insert("kustomization.yaml".to_owned(), kustomization(&manifests));
            manifests.insert("README.md".to_owned(), readme_text(&chart.version, values));
            generated.push((variant, manifests));
        }

        // Finish rendering every variant before replacing any checked-in files.
        for (variant, manifests) in generated {
            write_manifests(&destination.join(variant), &manifests)?;
        }
        Ok(())
    }
}

fn check_version(version: &Version, readme: &str) -> Result<()> {
    if let Some(current) = readme.lines().find_map(|line| {
        line.strip_prefix("version ")?
            .split_once(" with the following")
            .map(|(version, _)| version)
    }) {
        versions::ensure_newer_or_equal(version, &versions::parse(current)?, "chart")?;
    }
    Ok(())
}

fn read_manifests(templates: &Path) -> Result<BTreeMap<String, String>> {
    let mut manifests = BTreeMap::new();
    for entry in fs::read_dir(templates)? {
        let path = entry?.path();
        if path.extension().is_none_or(|ext| ext != "yaml") || !path.is_file() {
            continue;
        }
        let length = Command::new("yq")
            .args(["eval", ". | length"])
            .arg(&path)
            .check_output()?;
        if length.trim() == "0" {
            continue;
        }
        fs::write(&path, source_comments(&fs::read_to_string(&path)?))?;
        let cleaned = Command::new("yq")
            .args(["eval", CLEANUP])
            .arg(&path)
            .check_output()?;
        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .context("Manifest filename is not valid UTF-8")?;
        manifests.insert(filename.to_owned(), cleaned);
    }
    ensure!(
        !manifests.is_empty(),
        "Helm generated no Kubernetes manifests"
    );
    Ok(manifests)
}

fn source_comments(contents: &str) -> String {
    let mut output = String::new();
    for line in contents.lines() {
        if let Some(source) = line.strip_prefix("# Source: ") {
            output.push_str(
                "# This file has been generated by `helm template vector vector/vector` from ",
            );
            output.push_str(source);
            output.push_str(". Please re-run `make generate-kubernetes-manifests` rather than modifying this file manually.\n");
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }
    output
}

fn kustomization(manifests: &BTreeMap<String, String>) -> String {
    let mut output = "apiVersion: kustomize.config.k8s.io/v1beta1\nkind: Kustomization\n\nnamespace: vector\n\nresources:\n".to_owned();
    for filename in manifests.keys() {
        output.push_str("  - ");
        output.push_str(filename);
        output.push('\n');
    }
    output
}

fn readme_text(version: &str, values: &str) -> String {
    format!(
        "The kubernetes manifests found in this directory have been automatically generated\nfrom the [helm chart `vector/vector`](https://github.com/vectordotdev/helm-charts/tree/master/charts/vector)\nversion {version} with the following `values.yaml`:\n\n```yaml\n{}\n```\n",
        values.trim_end_matches('\n')
    )
}

fn write_manifests(destination: &Path, manifests: &BTreeMap<String, String>) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(destination)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            fs::remove_file(entry.path())?;
        }
    }
    for (filename, contents) in manifests {
        let path = destination.join(filename);
        if path.is_symlink() {
            fs::remove_file(&path)?;
        }
        fs::write(&path, contents)
            .with_context(|| format!("Failed to write {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser as _;

    use super::*;

    #[test]
    fn accepts_native_chart_version_option() {
        for args in [
            vec!["vdev", "build", "manifests"],
            vec!["vdev", "build", "manifests", "--chart-version", "0.58.0"],
        ] {
            crate::commands::Cli::try_parse_from(args).unwrap();
        }
        assert!(
            crate::commands::Cli::try_parse_from([
                "vdev",
                "build",
                "manifests",
                "--chart-version",
                "invalid",
            ])
            .is_err()
        );
    }

    #[test]
    fn permits_retries_and_upgrades_but_rejects_downgrades() {
        let readme = readme_text("0.58.0", "");
        for (version, accepted) in [
            ("0.58.0", true),
            ("0.59.0", true),
            ("0.57.0", false),
            ("0.58.0-rc.1", false),
        ] {
            assert_eq!(
                check_version(&versions::parse(version).unwrap(), &readme).is_ok(),
                accepted
            );
        }
        assert!(
            check_version(
                &versions::parse("0.58.0").unwrap(),
                "version invalid with the following"
            )
            .is_err()
        );
    }

    #[test]
    fn preserves_source_comments_for_multiple_documents() {
        let input = "---\n# Source: vector/templates/rbac.yaml\nkind: Role\n---\n# Source: vector/templates/rbac.yaml\nkind: RoleBinding\n";
        let output = source_comments(input);
        assert_eq!(
            output
                .matches("Please re-run `make generate-kubernetes-manifests`")
                .count(),
            2
        );
        assert!(output.contains("kind: Role\n---\n"));
        assert!(output.ends_with("kind: RoleBinding\n"));
    }

    #[test]
    fn replaces_stale_files_and_sorts_resources() {
        let temporary = tempfile::tempdir().unwrap();
        fs::write(temporary.path().join("old.yaml"), "old").unwrap();
        fs::create_dir(temporary.path().join("custom")).unwrap();
        let mut manifests = BTreeMap::from([
            ("z.yaml".to_owned(), "kind: Service\n".to_owned()),
            ("a.yaml".to_owned(), "kind: ConfigMap\n".to_owned()),
        ]);
        let expected = "apiVersion: kustomize.config.k8s.io/v1beta1\nkind: Kustomization\n\nnamespace: vector\n\nresources:\n  - a.yaml\n  - z.yaml\n";
        assert_eq!(kustomization(&manifests), expected);
        manifests.insert("kustomization.yaml".to_owned(), expected.to_owned());
        manifests.insert("README.md".to_owned(), readme_text("0.58.0", ""));
        write_manifests(temporary.path(), &manifests).unwrap();
        assert!(!temporary.path().join("old.yaml").exists());
        assert!(temporary.path().join("custom").is_dir());
        for (name, contents) in &manifests {
            assert_eq!(
                fs::read_to_string(temporary.path().join(name)).unwrap(),
                *contents
            );
        }
        write_manifests(temporary.path(), &manifests).unwrap();
        assert_eq!(
            fs::read_to_string(temporary.path().join("kustomization.yaml")).unwrap(),
            expected
        );
    }
}

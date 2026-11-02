//! The coverage workflow contract: plan the complete inventory, seal producer
//! artifacts, then validate every report before starting any Datadog upload.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write as _,
    path::{Component, Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{app, app::CommandExt as _, testing::config::ComposeTestConfig};

#[derive(clap::Args, Debug)]
pub struct Cli {
    #[command(subcommand)]
    action: Action,
}

#[derive(clap::Subcommand, Debug)]
enum Action {
    /// Record the expected reports for the checked-out commit and workflow matrix.
    Plan {
        #[arg(long)]
        workflow: PathBuf,
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        output: PathBuf,
    },
    /// Normalize and validate a producer's reports, then write its manifest.
    Seal {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        artifact: String,
        #[arg(long)]
        directory: PathBuf,
    },
    /// Validate the full collection and upload originals with suite flags.
    Publish {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        directory: PathBuf,
        #[arg(long)]
        summary: PathBuf,
        #[arg(long)]
        branch: String,
        #[arg(long)]
        validate_only: bool,
    },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    commit: String,
    run_id: String,
    artifacts: BTreeMap<String, Reports>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Reports {
    flag: String,
    files: BTreeSet<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    commit: String,
    run_id: String,
    artifact: String,
    checksums: BTreeMap<String, String>,
}

// Each executable source line is counted once; coverage is the union across
// reports/environments, not a sum of their percentages or execution counts.
type Lines = BTreeMap<String, BTreeMap<u64, bool>>;

#[derive(Default, Serialize)]
struct Summary {
    reports: usize,
    files: usize,
    executable_lines: usize,
    covered_lines: usize,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        match self.action {
            Action::Plan {
                workflow,
                run_id,
                output,
            } => {
                let workflow: serde_yaml::Value =
                    serde_yaml::from_str(&fs::read_to_string(workflow)?)?;
                let services =
                    workflow["jobs"]["integration-tests"]["strategy"]["matrix"]["service"]
                        .as_sequence()
                        .context("Expected a static integration service matrix")?;
                ensure!(!services.is_empty(), "Empty integration matrix");
                let mut artifacts = BTreeMap::from([(
                    "coverage-unit".into(),
                    Reports {
                        flag: "unit".into(),
                        files: BTreeSet::from(["lcov.info".into()]),
                    },
                )]);
                for service in services {
                    let service = service.as_str().context("Invalid integration service")?;
                    safe_name(service)?;
                    let (_, config) = ComposeTestConfig::load("integration", service)?;
                    let files = config
                        .environments()
                        .keys()
                        .map(|env| {
                            let name = crate::testing::runner::coverage_filename(Some(env));
                            safe_name(&name)?;
                            Ok(name)
                        })
                        .collect::<Result<BTreeSet<_>>>()?;
                    ensure!(!files.is_empty(), "No environments for {service}");
                    ensure!(
                        artifacts
                            .insert(
                                format!("coverage-int-{service}"),
                                Reports {
                                    flag: "integration".into(),
                                    files,
                                }
                            )
                            .is_none(),
                        "Duplicate service {service}"
                    );
                }
                write_json(
                    &output,
                    &Plan {
                        commit: checkout_commit()?,
                        run_id,
                        artifacts,
                    },
                )
            }
            Action::Seal {
                plan,
                artifact,
                directory,
            } => {
                let plan = read_plan(&plan)?;
                ensure_checkout(&plan)?;
                seal(&plan, &artifact, &directory, Path::new(app::path()))
            }
            Action::Publish {
                plan,
                directory,
                summary,
                branch,
                validate_only,
            } => {
                let plan = read_plan(&plan)?;
                ensure_checkout(&plan)?;
                let (groups, summaries) = validate(&plan, &directory)?;
                write_json(&summary, &summaries)?;
                if !validate_only {
                    // Both overrides are required: the CLI prefers PR head SHA
                    // over commit SHA, which may have advanced since the review.
                    for (flag, paths) in groups {
                        Command::new("datadog-ci")
                            .args(["coverage", "upload", "--flags", &flag])
                            .args(paths)
                            .env("DD_GIT_COMMIT_SHA", &plan.commit)
                            .env("DD_GIT_COMMIT_HEAD_SHA", &plan.commit)
                            .env("DD_GIT_BRANCH", &branch)
                            .env(
                                "DD_GIT_REPOSITORY_URL",
                                "https://github.com/vectordotdev/vector",
                            )
                            .check_run()?;
                    }
                }
                Ok(())
            }
        }
    }
}

fn checkout_commit() -> Result<String> {
    Ok(Command::new("git")
        .args(["rev-parse", "HEAD"])
        .check_output()?
        .trim()
        .to_owned())
}

fn ensure_checkout(plan: &Plan) -> Result<()> {
    ensure!(
        checkout_commit()? == plan.commit,
        "Checkout does not match coverage plan"
    );
    Ok(())
}

fn read_plan(path: &Path) -> Result<Plan> {
    let plan: Plan = serde_json::from_slice(&fs::read(path)?)?;
    ensure!(
        plan.commit.len() == 40 && plan.commit.bytes().all(|c| c.is_ascii_hexdigit()),
        "Invalid commit SHA"
    );
    ensure!(!plan.run_id.is_empty(), "Missing run ID");
    ensure!(
        plan.artifacts.contains_key("coverage-unit"),
        "Missing unit producer"
    );
    for (artifact, reports) in &plan.artifacts {
        safe_name(artifact)?;
        ensure!(
            !reports.files.is_empty(),
            "Empty report inventory for {artifact}"
        );
        ensure!(
            reports.flag == "unit" || reports.flag == "integration",
            "Unknown suite flag"
        );
        for file in &reports.files {
            safe_name(file)?;
        }
    }
    Ok(plan)
}

fn safe_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
            && name != "."
            && name != "..",
        "Unsafe artifact/report name: {name}"
    );
    Ok(())
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    fs::write(path, serde_json::to_vec_pretty(value)?)
        .with_context(|| format!("Writing {}", path.display()))
}

fn inventory(directory: &Path, directories: bool) -> Result<BTreeSet<String>> {
    fs::read_dir(directory)?
        .map(|entry| {
            let entry = entry?;
            let kind = entry.file_type()?;
            ensure!(
                if directories {
                    kind.is_dir()
                } else {
                    kind.is_file()
                },
                "Unexpected entry {}",
                entry.path().display()
            );
            entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("Non-UTF8 artifact name"))
        })
        .collect()
}

fn checksum(contents: &str) -> String {
    hex::encode(Sha256::digest(contents.as_bytes()))
}

fn seal(plan: &Plan, artifact: &str, directory: &Path, root: &Path) -> Result<()> {
    let reports = plan
        .artifacts
        .get(artifact)
        .context("Unexpected coverage producer")?;
    // Permit replacing the manifest on a retry, but never accept extra reports.
    let mut actual = inventory(directory, false)?;
    actual.remove("manifest.json");
    ensure!(
        actual == reports.files,
        "Report inventory mismatch for {artifact}: expected {:?}, found {actual:?}",
        reports.files
    );
    let prefix = format!("SF:{}/", root.display());
    let mut checksums = BTreeMap::new();
    for file in &reports.files {
        let path = directory.join(file);
        let contents = fs::read_to_string(&path)?
            .lines()
            .map(|line| {
                if let Some(relative) = line
                    .strip_prefix(&prefix)
                    .or_else(|| line.strip_prefix("SF:/home/vector/"))
                {
                    format!("SF:{relative}\n")
                } else {
                    format!("{line}\n")
                }
            })
            .collect::<String>();
        let lines =
            parse_lcov(&contents).with_context(|| format!("Invalid report {}", path.display()))?;
        check_unit_library(&reports.flag, &lines)?;
        // Containers can leave root-owned, read-only reports in the runner's
        // writable output directory. Replace the file atomically rather than
        // requiring permission to truncate the container-owned inode.
        let mut normalized = tempfile::NamedTempFile::new_in(directory)?;
        normalized.write_all(contents.as_bytes())?;
        normalized
            .persist(&path)
            .with_context(|| format!("Replacing normalized report {}", path.display()))?;
        checksums.insert(file.clone(), checksum(&contents));
    }
    write_json(
        &directory.join("manifest.json"),
        &Manifest {
            commit: plan.commit.clone(),
            run_id: plan.run_id.clone(),
            artifact: artifact.into(),
            checksums,
        },
    )
}

fn check_unit_library(flag: &str, lines: &Lines) -> Result<()> {
    ensure!(
        flag != "unit" || lines.keys().any(|path| path.starts_with("lib/")),
        "Unit report omits workspace libraries"
    );
    Ok(())
}

fn parse_lcov(contents: &str) -> Result<Lines> {
    let mut lines = Lines::new();
    let mut source = None;
    for line in contents.lines() {
        if let Some(path) = line.strip_prefix("SF:") {
            ensure!(source.is_none(), "Unterminated LCOV record");
            ensure!(
                !path.is_empty()
                    && !path.contains(['\\', ':'])
                    && Path::new(path)
                        .components()
                        .all(|c| matches!(c, Component::Normal(_))),
                "Source path must be repository-relative: {path}"
            );
            source = Some(path.to_owned());
        } else if let Some(data) = line.strip_prefix("DA:") {
            let source = source.as_ref().context("DA outside source record")?;
            let fields = data.split(',').collect::<Vec<_>>();
            ensure!((2..=3).contains(&fields.len()), "Invalid DA: {data}");
            let number = fields[0].parse::<u64>()?;
            let count = fields[1].parse::<u64>()?;
            ensure!(number > 0, "Invalid source line zero");
            *lines
                .entry(source.clone())
                .or_default()
                .entry(number)
                .or_default() |= count > 0;
        } else if line == "end_of_record" {
            ensure!(source.take().is_some(), "Unexpected end_of_record");
        }
    }
    ensure!(source.is_none(), "Unterminated LCOV record");
    ensure!(!lines.is_empty(), "Empty coverage report");
    Ok(lines)
}

type Uploads = BTreeMap<String, Vec<PathBuf>>;

fn validate(plan: &Plan, directory: &Path) -> Result<(Uploads, BTreeMap<String, Summary>)> {
    let expected = plan.artifacts.keys().cloned().collect();
    ensure!(
        inventory(directory, true)? == expected,
        "Coverage producer inventory mismatch"
    );
    let mut groups = Uploads::new();
    let mut union = BTreeMap::<String, Lines>::new();
    for (artifact, reports) in &plan.artifacts {
        let folder = directory.join(artifact);
        let manifest: Manifest = serde_json::from_slice(&fs::read(folder.join("manifest.json"))?)?;
        ensure!(
            manifest.commit == plan.commit
                && manifest.run_id == plan.run_id
                && manifest.artifact == *artifact,
            "Coverage provenance mismatch for {artifact}"
        );
        ensure!(
            manifest.checksums.keys().cloned().collect::<BTreeSet<_>>() == reports.files,
            "Manifest inventory mismatch for {artifact}"
        );
        let mut files = reports.files.clone();
        files.insert("manifest.json".into());
        ensure!(
            inventory(&folder, false)? == files,
            "Report inventory mismatch for {artifact}"
        );
        for file in &reports.files {
            let path = folder.join(file);
            let contents = fs::read_to_string(&path)?;
            ensure!(
                checksum(&contents) == manifest.checksums[file],
                "Checksum mismatch for {}",
                path.display()
            );
            let lines = parse_lcov(&contents)
                .with_context(|| format!("Invalid report {}", path.display()))?;
            check_unit_library(&reports.flag, &lines)?;
            for flag in [&reports.flag, "combined"] {
                let combined = union.entry(flag.to_owned()).or_default();
                for (file, lines) in &lines {
                    for (number, covered) in lines {
                        *combined
                            .entry(file.clone())
                            .or_default()
                            .entry(*number)
                            .or_default() |= covered;
                    }
                }
            }
            groups.entry(reports.flag.clone()).or_default().push(path);
        }
    }
    let summaries = union
        .into_iter()
        .map(|(flag, lines)| {
            let reports = if flag == "combined" {
                groups.values().map(Vec::len).sum()
            } else {
                groups[&flag].len()
            };
            let summary = Summary {
                reports,
                files: lines.len(),
                executable_lines: lines.values().map(BTreeMap::len).sum(),
                covered_lines: lines
                    .values()
                    .flat_map(BTreeMap::values)
                    .filter(|covered| **covered)
                    .count(),
            };
            (flag, summary)
        })
        .collect();
    Ok((groups, summaries))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, Plan) {
        let root = tempfile::tempdir().unwrap();
        let plan = Plan {
            commit: "a".repeat(40),
            run_id: "123".into(),
            artifacts: BTreeMap::from([
                (
                    "coverage-unit".into(),
                    Reports {
                        flag: "unit".into(),
                        files: BTreeSet::from(["lcov.info".into()]),
                    },
                ),
                (
                    "coverage-int-redis".into(),
                    Reports {
                        flag: "integration".into(),
                        files: BTreeSet::from(["lcov-6.info".into(), "lcov-7.info".into()]),
                    },
                ),
            ]),
        };
        for (artifact, reports) in &plan.artifacts {
            let dir = root.path().join(artifact);
            fs::create_dir(&dir).unwrap();
            for file in &reports.files {
                let data = if reports.flag == "unit" {
                    "SF:lib/example.rs\nDA:1,1\nend_of_record\nSF:src/main.rs\nDA:1,0\nDA:2,1\nend_of_record\n"
                } else {
                    "SF:/home/vector/src/main.rs\nDA:1,2\nDA:2,0\nend_of_record\n"
                };
                fs::write(dir.join(file), data).unwrap();
            }
            seal(&plan, artifact, &dir, root.path()).unwrap();
        }
        (root, plan)
    }

    #[test]
    fn retains_every_environment_and_unions_lines() {
        let (root, plan) = fixture();
        let (groups, summary) = validate(&plan, root.path()).unwrap();
        assert_eq!(groups["integration"].len(), 2);
        assert_eq!(summary["combined"].reports, 3);
        assert_eq!(summary["combined"].executable_lines, 3);
        assert_eq!(summary["combined"].covered_lines, 3);
        assert_eq!(summary["integration"].covered_lines, 1);
    }

    #[test]
    #[cfg(unix)]
    fn seals_read_only_container_reports() {
        use std::os::unix::fs::PermissionsExt as _;

        let (root, plan) = fixture();
        let directory = root.path().join("coverage-int-redis");
        let path = directory.join("lcov-6.info");
        fs::write(
            &path,
            "SF:/home/vector/src/main.rs\nDA:1,2\nend_of_record\n",
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
        seal(&plan, "coverage-int-redis", &directory, root.path()).unwrap();
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .starts_with("SF:src/main.rs\n")
        );
        validate(&plan, root.path()).unwrap();
    }

    #[test]
    fn missing_producer_or_environment_is_rejected() {
        let (root, plan) = fixture();
        fs::remove_file(root.path().join("coverage-int-redis/lcov-7.info")).unwrap();
        assert!(validate(&plan, root.path()).is_err());
        assert!(
            seal(
                &plan,
                "coverage-int-redis",
                &root.path().join("coverage-int-redis"),
                root.path()
            )
            .is_err()
        );
        fs::remove_dir_all(root.path().join("coverage-int-redis")).unwrap();
        assert!(validate(&plan, root.path()).is_err());
    }

    #[test]
    fn mismatched_commit_run_or_checksum_is_rejected() {
        let (root, mut plan) = fixture();
        plan.commit = "b".repeat(40);
        assert!(validate(&plan, root.path()).is_err());
        plan.commit = "a".repeat(40);
        plan.run_id = "other-run".into();
        assert!(validate(&plan, root.path()).is_err());
        plan.run_id = "123".into();
        fs::write(
            root.path().join("coverage-int-redis/lcov-7.info"),
            "changed",
        )
        .unwrap();
        assert!(validate(&plan, root.path()).is_err());
    }

    #[test]
    fn extra_reports_and_producers_are_rejected() {
        let (root, plan) = fixture();
        fs::write(root.path().join("coverage-unit/stale.info"), "stale").unwrap();
        assert!(validate(&plan, root.path()).is_err());
        fs::remove_file(root.path().join("coverage-unit/stale.info")).unwrap();
        fs::create_dir(root.path().join("coverage-e2e-runner")).unwrap();
        assert!(validate(&plan, root.path()).is_err());
    }

    #[test]
    fn malformed_empty_and_non_relative_reports_are_rejected() {
        for data in [
            "",
            "DA:1,1\n",
            "SF:src/a.rs\nDA:1,1\n",
            "SF:src/a.rs\nDA:0,1\nend_of_record\n",
            "SF:src/a.rs\nDA:1,-1\nend_of_record\n",
            "SF:../outside.rs\nDA:1,1\nend_of_record\n",
            "SF:/home/vector/src/a.rs\nDA:1,1\nend_of_record\n",
        ] {
            assert!(parse_lcov(data).is_err(), "Accepted {data:?}");
        }
    }

    #[test]
    fn root_only_unit_coverage_is_rejected() {
        let lines = parse_lcov("SF:src/main.rs\nDA:1,1\nend_of_record\n").unwrap();
        assert!(check_unit_library("unit", &lines).is_err());
        assert!(check_unit_library("integration", &lines).is_ok());
    }
}

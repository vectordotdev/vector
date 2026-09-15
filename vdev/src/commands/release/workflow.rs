#![allow(clippy::print_stdout)]

use crate::{
    app::CommandExt as _,
    utils::{git, paths},
};
use anyhow::{Context, Result, bail, ensure};
use semver::{Prerelease, Version};
use serde::Deserialize;
use std::{
    env, fs,
    fs::OpenOptions,
    io::Write as _,
    path::Path,
    process::{Command, Stdio},
};
use toml_edit::DocumentMut;

const MASTER_BRANCH: &str = "master";

/// Helpers used by the GitHub release preparation workflows.
#[derive(clap::Args, Debug)]
pub struct Cli {
    #[command(subcommand)]
    command: WorkflowCommand,
}

#[derive(clap::Subcommand, Debug)]
enum WorkflowCommand {
    /// Validate a request before generating a release preparation PR.
    PrepareCheck(PrepareCheck),
    /// Validate a generated release preparation PR.
    PrCheck(PrCheck),
}

#[derive(clap::Args, Debug)]
struct PrepareCheck {
    #[arg(long)]
    version: Version,
    #[arg(long)]
    bot_app: String,
    #[arg(long)]
    repository: String,
}

#[derive(clap::Args, Debug)]
struct PrCheck {
    #[arg(long)]
    base_sha: String,
    #[arg(long)]
    head_ref: String,
}

#[derive(Debug, Deserialize)]
struct ExistingPullRequest {
    #[serde(rename = "isCrossRepository")]
    is_cross_repository: bool,
    url: String,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        env::set_current_dir(paths::find_repo_root()?)?;
        match self.command {
            WorkflowCommand::PrepareCheck(args) => args.exec(),
            WorkflowCommand::PrCheck(args) => args.exec(),
        }
    }
}

impl PrepareCheck {
    fn exec(self) -> Result<()> {
        ensure_stable(&self.version, "release version")?;

        let expected_current = development_version(&self.version)?;
        let current = current_cargo_version()?;
        ensure!(
            current == expected_current,
            "expected Cargo.toml version {expected_current}, found {current}"
        );

        let tag = format!("v{}", self.version);
        ensure!(
            resolve_ref(&format!("refs/tags/{tag}^{{commit}}"))?.is_none(),
            "tag {tag} already exists"
        );

        let branch = format!("release/prepare-v{}", self.version);
        if let Some(url) = find_existing_pr(&self.repository, &branch, &self.bot_app)? {
            println!("Preparation PR already exists: {url}");
            set_output("skip", "true")?;
            append_step_summary(&format!("Existing preparation PR: {url}"))?;
        } else {
            ensure!(
                !remote_branch_exists(&branch)?,
                "branch {branch} exists without an open PR"
            );
            set_output("skip", "false")?;
        }
        Ok(())
    }
}

impl PrCheck {
    fn exec(self) -> Result<()> {
        ensure_sha(&self.base_sha, "base SHA")?;
        let version = self
            .head_ref
            .strip_prefix("release/prepare-v")
            .context("release preparation branch must start with `release/prepare-v`")?;
        let version = parse_stable_version(version, "preparation branch version")?;
        let expected_base = development_version(&version)?;
        let base_version = cargo_version_at(&self.base_sha)?;
        let head_version = current_cargo_version()?;

        ensure!(
            base_version == expected_base,
            "expected base version {expected_base}, found {base_version}"
        );
        ensure!(
            head_version == version,
            "expected head version {version}, found {head_version}"
        );
        validate_single_commit(&self.base_sha)?;

        validate_release_files(&changed_files(&self.base_sha, "HEAD")?)?;
        let release_file = format!("website/cue/reference/releases/{version}.cue");
        ensure!(
            Path::new(&release_file).is_file(),
            "{release_file} does not exist"
        );

        cargo_metadata()
    }
}

fn ensure_stable(version: &Version, label: &str) -> Result<()> {
    ensure!(
        version.pre.is_empty() && version.build.is_empty(),
        "{label} must be a stable semantic version"
    );
    Ok(())
}

fn parse_stable_version(value: &str, label: &str) -> Result<Version> {
    let version = Version::parse(value).with_context(|| format!("invalid {label}: {value}"))?;
    ensure_stable(&version, label)?;
    Ok(version)
}

fn development_version(version: &Version) -> Result<Version> {
    ensure_stable(version, "release version")?;
    let mut development = version.clone();
    development.pre = Prerelease::new("dev")?;
    Ok(development)
}

fn current_cargo_version() -> Result<Version> {
    parse_cargo_version(&fs::read_to_string("Cargo.toml").context("failed to read Cargo.toml")?)
}

fn cargo_version_at(revision: &str) -> Result<Version> {
    parse_cargo_version(&git::run_and_check_output(&[
        "show",
        &format!("{revision}:Cargo.toml"),
    ])?)
}

fn parse_cargo_version(contents: &str) -> Result<Version> {
    let doc = contents
        .parse::<DocumentMut>()
        .context("failed to parse Cargo.toml")?;
    let version = doc["package"]["version"]
        .as_str()
        .context("Cargo.toml package.version must be a string")?;
    Version::parse(version).context("Cargo.toml package.version is not a semantic version")
}

fn validate_release_files(files: &[String]) -> Result<()> {
    if let Some(file) = files.iter().find(|file| !release_file_allowed(file)) {
        bail!("unexpected release preparation file: {file}");
    }
    Ok(())
}

fn release_file_allowed(file: &str) -> bool {
    matches!(
        file,
        "Cargo.toml"
            | "Cargo.lock"
            | "LICENSE-3rdparty.csv"
            | "distribution/install.sh"
            | "website/cue/reference/administration/interfaces/kubectl.cue"
            | "website/cue/reference/versions.cue"
    ) || file.starts_with("docs/generated/")
        || prefixed_file(file, "changelog.d/", ".md")
        || prefixed_file(file, "website/content/en/highlights/", ".md")
        || prefixed_file(file, "website/content/en/releases/", ".md")
        || prefixed_file(file, "website/cue/reference/releases/", ".cue")
}

fn prefixed_file(file: &str, prefix: &str, suffix: &str) -> bool {
    file.strip_prefix(prefix)
        .is_some_and(|name| !name.is_empty() && name.ends_with(suffix))
}

fn changed_files(before: &str, after: &str) -> Result<Vec<String>> {
    Ok(
        git::run_and_check_output(&["diff", "--name-only", before, after])?
            .lines()
            .map(str::to_owned)
            .collect(),
    )
}

fn validate_single_commit(base: &str) -> Result<()> {
    let head = git::run_and_check_output(&["rev-list", "--parents", "-n", "1", "HEAD"])?;
    let mut revisions = head.split_whitespace();
    let _head = revisions.next();
    ensure!(
        revisions.next() == Some(base) && revisions.next().is_none(),
        "release PR must contain exactly one non-merge commit on base {base}"
    );
    Ok(())
}

fn resolve_ref(reference: &str) -> Result<Option<String>> {
    let output = Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", reference])
        .output()
        .context("failed to inspect git reference")?;
    if output.status.success() {
        Ok(Some(
            String::from_utf8(output.stdout)
                .context("git reference is not UTF-8")?
                .trim()
                .to_owned(),
        ))
    } else if output.status.code() == Some(1) {
        Ok(None)
    } else {
        bail!("failed to inspect git reference {reference}")
    }
}

fn remote_branch_exists(branch: &str) -> Result<bool> {
    let status = Command::new("git")
        .args(["ls-remote", "--exit-code", "--heads", "origin", branch])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to inspect remote branch")?;
    match status.code() {
        Some(0) => Ok(true),
        Some(2) => Ok(false),
        code => bail!("git ls-remote failed with status {code:?}"),
    }
}

fn find_existing_pr(repository: &str, branch: &str, bot_app: &str) -> Result<Option<String>> {
    let output = Command::new("gh")
        .args([
            "pr",
            "list",
            "--repo",
            repository,
            "--head",
            branch,
            "--base",
            MASTER_BRANCH,
            "--app",
            bot_app,
            "--state",
            "open",
            "--json",
            "isCrossRepository,url",
        ])
        .check_output()?;
    let prs: Vec<ExistingPullRequest> =
        serde_json::from_str(&output).context("invalid gh pr list response")?;
    Ok(prs
        .into_iter()
        .find(|pr| !pr.is_cross_repository)
        .map(|pr| pr.url))
}

fn ensure_sha(value: &str, label: &str) -> Result<()> {
    ensure!(
        value.len() == 40
            && value
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
        "{label} must be a 40-character lowercase hexadecimal SHA"
    );
    Ok(())
}

fn set_output(name: &str, value: &str) -> Result<()> {
    if let Some(path) = env::var_os("GITHUB_OUTPUT") {
        writeln!(
            OpenOptions::new().create(true).append(true).open(path)?,
            "{name}={value}"
        )?;
    } else {
        println!("{name}={value}");
    }
    Ok(())
}

fn append_step_summary(line: &str) -> Result<()> {
    if let Some(path) = env::var_os("GITHUB_STEP_SUMMARY") {
        writeln!(
            OpenOptions::new().create(true).append(true).open(path)?,
            "{line}"
        )?;
    }
    Ok(())
}

fn cargo_metadata() -> Result<()> {
    Command::new("cargo")
        .args(["metadata", "--locked", "--no-deps", "--format-version", "1"])
        .stdout(Stdio::null())
        .check_run()
}

#[cfg(test)]
mod tests {
    use super::{parse_cargo_version, release_file_allowed};
    use indoc::indoc;

    #[test]
    fn parses_package_version() {
        let cargo_toml = indoc! {r#"
            [package]
            name = "vector"
            version = "0.59.0-dev"
        "#};
        assert_eq!(
            parse_cargo_version(cargo_toml).expect("valid Cargo.toml"),
            "0.59.0-dev".parse().expect("valid version")
        );
    }

    #[test]
    fn release_allowlist_is_narrow() {
        assert!(release_file_allowed("Cargo.toml"));
        assert!(release_file_allowed("changelog.d/26289.feature.md"));
        assert!(release_file_allowed(
            "website/cue/reference/releases/0.59.0.cue"
        ));
        assert!(!release_file_allowed("src/main.rs"));
        assert!(!release_file_allowed("changelog.d/README.txt"));
    }
}

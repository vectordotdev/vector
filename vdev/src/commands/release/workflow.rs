#![allow(clippy::print_stdout)]

use crate::{
    app::CommandExt as _,
    commands::release::prepare::update_vector_package_version,
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

const STATE_PATH: &str = ".github/release-state.json";
const MASTER_BRANCH: &str = "master";
const VRL_GIT_URL: &str = "https://github.com/vectordotdev/vrl.git";

/// Helpers used by the GitHub release workflows.
#[derive(clap::Args, Debug)]
pub struct Cli {
    #[command(subcommand)]
    command: WorkflowCommand,
}

#[derive(clap::Subcommand, Debug)]
enum WorkflowCommand {
    /// Validate a request before generating a release preparation PR.
    PrepareCheck(PrepareCheck),
    /// Validate a release preparation or housekeeping PR.
    PrCheck(PrCheck),
    /// Validate a master transition before creating a release tag.
    AutotagCheck(AutotagCheck),
    /// Validate a release tag before publishing artifacts.
    PublicationCheck(PublicationCheck),
    /// Classify the current master state before post-release housekeeping.
    HousekeepingCheck(HousekeepingCheck),
    /// Generate the files for a post-release housekeeping PR.
    HousekeepingPrepare(HousekeepingPrepare),
}

#[derive(clap::Args, Debug)]
struct PrepareCheck {
    #[arg(long)]
    version: Version,
    #[arg(long)]
    vrl_version: Version,
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

#[derive(clap::Args, Debug)]
struct AutotagCheck {
    #[arg(long)]
    before_sha: String,
    #[arg(long)]
    sha: String,
    #[arg(long)]
    repository: String,
}

#[derive(clap::Args, Debug)]
struct PublicationCheck {
    #[arg(long)]
    tag: String,
    #[arg(long)]
    sha: String,
    #[arg(long)]
    repository: String,
}

#[derive(clap::Args, Debug)]
struct HousekeepingCheck {
    #[arg(long)]
    version: Version,
    #[arg(long)]
    release_commit: String,
    #[arg(long)]
    bot_app: String,
    #[arg(long)]
    repository: String,
}

#[derive(clap::Args, Debug)]
struct HousekeepingPrepare {
    #[arg(long)]
    version: Version,
    #[arg(long)]
    release_commit: String,
}

#[derive(Debug, Deserialize)]
struct ReleaseMetadata {
    schema_version: u64,
    status: String,
    version: String,
    #[serde(default)]
    prepared_from: Option<String>,
    #[serde(default)]
    last_release: Option<LastRelease>,
}

#[derive(Debug, Deserialize)]
struct LastRelease {
    version: String,
    tag: String,
    commit: String,
}

#[derive(Debug, Deserialize)]
struct ExistingPullRequest {
    #[serde(rename = "isCrossRepository")]
    is_cross_repository: bool,
    url: String,
}

#[derive(Debug, Deserialize)]
struct AssociatedPullRequest {
    merged_at: Option<String>,
    base: PullRequestRef,
    head: PullRequestHead,
    merge_commit_sha: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PullRequestRef {
    #[serde(rename = "ref")]
    name: String,
}

#[derive(Debug, Deserialize)]
struct PullRequestHead {
    #[serde(rename = "ref")]
    name: String,
    repo: Option<PullRequestRepo>,
}

#[derive(Debug, Deserialize)]
struct PullRequestRepo {
    full_name: String,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        env::set_current_dir(paths::find_repo_root()?)?;
        match self.command {
            WorkflowCommand::PrepareCheck(args) => args.exec(),
            WorkflowCommand::PrCheck(args) => args.exec(),
            WorkflowCommand::AutotagCheck(args) => args.exec(),
            WorkflowCommand::PublicationCheck(args) => args.exec(),
            WorkflowCommand::HousekeepingCheck(args) => args.exec(),
            WorkflowCommand::HousekeepingPrepare(args) => args.exec(),
        }
    }
}

impl PrepareCheck {
    fn exec(self) -> Result<()> {
        ensure_stable(&self.version, "release version")?;
        ensure_stable(&self.vrl_version, "VRL version")?;

        let expected_current = development_version(&self.version)?;
        let current = current_cargo_version()?;
        ensure!(
            current == expected_current,
            "expected Cargo.toml version {expected_current}, found {current}"
        );

        let metadata = read_metadata(Path::new(STATE_PATH))?;
        validate_development_metadata(&metadata, &expected_current)?;

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
        set_output(
            "next_version",
            &next_minor_development_version(&self.version)?.to_string(),
        )?;
        Ok(())
    }
}

impl PrCheck {
    fn exec(self) -> Result<()> {
        ensure_sha(&self.base_sha, "base SHA")?;
        let base_version = cargo_version_at(&self.base_sha)?;
        let head_version = current_cargo_version()?;
        let changed_files = changed_files(&self.base_sha, "HEAD")?;

        if let Some(version) = self.head_ref.strip_prefix("release/prepare-v") {
            let version = parse_stable_version(version, "preparation branch version")?;
            let expected_base = development_version(&version)?;
            ensure!(
                base_version == expected_base,
                "expected base version {expected_base}, found {base_version}"
            );
            ensure!(
                head_version == version,
                "expected head version {version}, found {head_version}"
            );
            validate_release_files(&changed_files, "release preparation")?;

            let base_metadata = metadata_at(&self.base_sha)?;
            validate_development_metadata(&base_metadata, &expected_base)?;
            let metadata = read_metadata(Path::new(STATE_PATH))?;
            validate_prepared_metadata(&metadata, &version, &self.base_sha)?;

            let release_file = format!("website/cue/reference/releases/{version}.cue");
            ensure!(
                Path::new(&release_file).is_file(),
                "{release_file} does not exist"
            );
        } else if let Some(version) = self.head_ref.strip_prefix("release/housekeeping-v") {
            let version = parse_stable_version(version, "housekeeping branch version")?;
            ensure!(
                base_version == version,
                "expected base version {version}, found {base_version}"
            );

            let base_metadata = metadata_at(&self.base_sha)?;
            validate_prepared_metadata(
                &base_metadata,
                &version,
                base_metadata.prepared_from.as_deref().unwrap_or_default(),
            )?;
            let next = next_minor_development_version(&version)?;
            ensure!(
                head_version == next,
                "expected head version {next}, found {head_version}"
            );
            validate_housekeeping_files(&changed_files)?;

            let metadata = read_metadata(Path::new(STATE_PATH))?;
            validate_housekeeping_metadata(&metadata, &next, &version, None)?;
        } else {
            bail!("unsupported release branch {}", self.head_ref);
        }

        cargo_metadata()
    }
}

impl AutotagCheck {
    fn exec(self) -> Result<()> {
        ensure_sha(&self.before_sha, "before SHA")?;
        ensure_sha(&self.sha, "release SHA")?;
        let current = current_cargo_version()?;
        let previous = cargo_version_at(&self.before_sha)?;

        if current.pre.as_str() == "dev" && current.build.is_empty() {
            println!("Development transition {previous} -> {current}; no release tag required.");
            set_output("tag_required", "false")?;
            return Ok(());
        }

        ensure_stable(&current, "current Cargo.toml version")?;
        let expected_previous = development_version(&current)?;
        ensure!(
            previous == expected_previous,
            "invalid release transition {previous} -> {current}"
        );

        validate_release_files(
            &changed_files(&self.before_sha, &self.sha)?,
            "release merge",
        )?;
        let metadata = read_metadata(Path::new(STATE_PATH))?;
        validate_prepared_metadata(&metadata, &current, &self.before_sha)?;
        validate_associated_preparation_pr(&self.repository, &self.sha, &current)?;

        let tag = format!("v{current}");
        if let Some(existing) = resolve_ref(&format!("refs/tags/{tag}^{{commit}}"))? {
            ensure!(
                existing == self.sha,
                "tag {tag} already points to {existing}"
            );
        }
        set_output("tag_required", "true")?;
        set_output("tag", &tag)
    }
}

impl PublicationCheck {
    fn exec(self) -> Result<()> {
        ensure_sha(&self.sha, "release SHA")?;
        let version = self
            .tag
            .strip_prefix('v')
            .context("release tag must start with `v`")?;
        let version = parse_stable_version(version, "release tag")?;
        let cargo_version = current_cargo_version()?;
        ensure!(
            cargo_version == version,
            "tag {0} contains Cargo.toml version {cargo_version}",
            self.tag
        );

        let metadata = read_metadata(Path::new(STATE_PATH))?;
        let prepared_from = metadata
            .prepared_from
            .as_deref()
            .context("prepared release state is missing prepared_from")?;
        validate_prepared_metadata(&metadata, &version, prepared_from)?;
        ensure!(
            is_ancestor(&self.sha, "origin/master")?,
            "tagged release commit {} is not on master",
            self.sha
        );
        validate_associated_preparation_pr(&self.repository, &self.sha, &version)?;

        set_output("version", &version.to_string())?;
        set_output(
            "next_version",
            &next_minor_development_version(&version)?.to_string(),
        )
    }
}

impl HousekeepingCheck {
    fn exec(self) -> Result<()> {
        ensure_stable(&self.version, "release version")?;
        ensure_sha(&self.release_commit, "release commit")?;
        let next = next_minor_development_version(&self.version)?;
        let current = current_cargo_version()?;
        let metadata = read_metadata(Path::new(STATE_PATH))?;

        if current == next {
            validate_housekeeping_metadata(
                &metadata,
                &next,
                &self.version,
                Some(&self.release_commit),
            )?;
            println!("Housekeeping for v{} is already merged.", self.version);
            set_output("skip", "true")?;
            return Ok(());
        }

        if current.pre.as_str() == "dev" && current.build.is_empty() {
            validate_development_metadata(&metadata, &current)?;
            let last_release = metadata
                .last_release
                .as_ref()
                .context("development release state is missing last_release")?;
            let last_version = parse_stable_version(&last_release.version, "last release version")?;
            if last_version > self.version {
                println!(
                    "Master has advanced past v{}; housekeeping is no longer needed.",
                    self.version
                );
                set_output("skip", "true")?;
                return Ok(());
            }
        } else if current > self.version {
            ensure_stable(&current, "current Cargo.toml version")?;
            let prepared_from = metadata
                .prepared_from
                .as_deref()
                .context("prepared release state is missing prepared_from")?;
            validate_prepared_metadata(&metadata, &current, prepared_from)?;
            println!(
                "Master is preparing a release after v{}; housekeeping is no longer needed.",
                self.version
            );
            set_output("skip", "true")?;
            return Ok(());
        }

        ensure!(
            current == self.version,
            "expected master version {}, found {current}",
            self.version
        );
        let prepared_from = metadata
            .prepared_from
            .as_deref()
            .context("prepared release state is missing prepared_from")?;
        validate_prepared_metadata(&metadata, &self.version, prepared_from)?;

        let branch = format!("release/housekeeping-v{}", self.version);
        set_output("skip", "false")?;
        if let Some(url) = find_existing_pr(&self.repository, &branch, &self.bot_app)? {
            println!("Housekeeping PR already exists: {url}");
            set_output("skip_change", "true")?;
            set_output("pr_url", &url)?;
            append_step_summary(&format!("Existing housekeeping PR: {url}"))?;
        } else {
            ensure!(
                !remote_branch_exists(&branch)?,
                "branch {branch} exists without an open PR"
            );
            set_output("skip_change", "false")?;
        }
        Ok(())
    }
}

impl HousekeepingPrepare {
    fn exec(self) -> Result<()> {
        ensure_stable(&self.version, "release version")?;
        ensure_sha(&self.release_commit, "release commit")?;
        let next = next_minor_development_version(&self.version)?;

        let metadata = read_metadata(Path::new(STATE_PATH))?;
        let prepared_from = metadata
            .prepared_from
            .as_deref()
            .context("prepared release state is missing prepared_from")?;
        validate_prepared_metadata(&metadata, &self.version, prepared_from)?;

        let cargo_toml = fs::read_to_string("Cargo.toml").context("failed to read Cargo.toml")?;
        let cargo_toml = update_vector_package_version(
            &cargo_toml,
            &self.version.to_string(),
            &next.to_string(),
        )?;
        let cargo_toml = update_vrl_to_main(&cargo_toml)?;
        fs::write("Cargo.toml", cargo_toml).context("failed to write Cargo.toml")?;

        Command::new("cargo")
            .args(["update", "-p", "vector"])
            .check_run()?;
        Command::new("cargo")
            .args(["update", "-p", "vrl"])
            .check_run()?;

        let metadata = serde_json::json!({
            "schema_version": 1,
            "status": "development",
            "version": next.to_string(),
            "last_release": {
                "version": self.version.to_string(),
                "tag": format!("v{}", self.version),
                "commit": self.release_commit,
            }
        });
        fs::write(
            STATE_PATH,
            format!("{}\n", serde_json::to_string_pretty(&metadata)?),
        )
        .context("failed to write release state")?;

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

fn next_minor_development_version(version: &Version) -> Result<Version> {
    ensure_stable(version, "release version")?;
    let minor = version
        .minor
        .checked_add(1)
        .context("minor version overflow")?;
    let mut next = Version::new(version.major, minor, 0);
    next.pre = Prerelease::new("dev")?;
    Ok(next)
}

fn current_cargo_version() -> Result<Version> {
    parse_cargo_version(&fs::read_to_string("Cargo.toml").context("failed to read Cargo.toml")?)
}

fn cargo_version_at(revision: &str) -> Result<Version> {
    parse_cargo_version(&file_at(revision, "Cargo.toml")?)
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

fn read_metadata(path: &Path) -> Result<ReleaseMetadata> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    parse_metadata(&contents)
}

fn metadata_at(revision: &str) -> Result<ReleaseMetadata> {
    parse_metadata(&file_at(revision, STATE_PATH)?)
}

fn parse_metadata(contents: &str) -> Result<ReleaseMetadata> {
    serde_json::from_str(contents).context("invalid release state")
}

fn validate_development_metadata(metadata: &ReleaseMetadata, version: &Version) -> Result<()> {
    ensure!(
        metadata.schema_version == 1,
        "unsupported release state schema {}",
        metadata.schema_version
    );
    ensure!(
        metadata.status == "development",
        "expected development release state, found {}",
        metadata.status
    );
    ensure!(
        metadata.version == version.to_string(),
        "release state version {} does not match {version}",
        metadata.version
    );
    Ok(())
}

fn validate_prepared_metadata(
    metadata: &ReleaseMetadata,
    version: &Version,
    prepared_from: &str,
) -> Result<()> {
    ensure!(
        metadata.schema_version == 1,
        "unsupported release state schema {}",
        metadata.schema_version
    );
    ensure!(
        metadata.status == "prepared",
        "expected prepared release state, found {}",
        metadata.status
    );
    ensure!(
        metadata.version == version.to_string(),
        "release state version {} does not match {version}",
        metadata.version
    );
    ensure_sha(prepared_from, "prepared_from")?;
    ensure!(
        metadata.prepared_from.as_deref() == Some(prepared_from),
        "release state prepared_from does not match {prepared_from}"
    );

    Ok(())
}

fn validate_housekeeping_metadata(
    metadata: &ReleaseMetadata,
    next: &Version,
    release: &Version,
    release_commit: Option<&str>,
) -> Result<()> {
    validate_development_metadata(metadata, next)?;
    let last = metadata
        .last_release
        .as_ref()
        .context("development release state is missing last_release")?;
    ensure!(
        last.version == release.to_string(),
        "last release version {} does not match {release}",
        last.version
    );
    ensure!(
        last.tag == format!("v{release}"),
        "last release tag {} does not match v{release}",
        last.tag
    );
    ensure_sha(&last.commit, "last release commit")?;
    if let Some(release_commit) = release_commit {
        ensure!(
            last.commit == release_commit,
            "last release commit {} does not match {release_commit}",
            last.commit
        );
    }
    Ok(())
}

fn validate_release_files(files: &[String], transition: &str) -> Result<()> {
    if let Some(file) = files.iter().find(|file| !release_file_allowed(file)) {
        bail!("unexpected {transition} file: {file}");
    }
    Ok(())
}

fn release_file_allowed(file: &str) -> bool {
    matches!(
        file,
        ".github/release-state.json"
            | "Cargo.lock"
            | "Cargo.toml"
            | "distribution/install.sh"
            | "website/cue/reference/administration/interfaces/kubectl.cue"
            | "website/cue/reference/versions.cue"
    ) || prefixed_file(file, "changelog.d/", ".md")
        || prefixed_file(file, "website/content/en/highlights/", ".md")
        || prefixed_file(file, "website/content/en/releases/", ".md")
        || prefixed_file(file, "website/cue/reference/releases/", ".cue")
}

fn prefixed_file(file: &str, prefix: &str, suffix: &str) -> bool {
    file.strip_prefix(prefix)
        .is_some_and(|name| !name.is_empty() && name.ends_with(suffix))
}

fn validate_housekeeping_files(files: &[String]) -> Result<()> {
    let mut actual = files.to_vec();
    actual.sort();
    let expected = vec![
        STATE_PATH.to_string(),
        "Cargo.lock".to_string(),
        "Cargo.toml".to_string(),
    ];
    ensure!(
        actual == expected,
        "unexpected housekeeping files: {}",
        actual.join(", ")
    );
    Ok(())
}

fn file_at(revision: &str, path: &str) -> Result<String> {
    git::run_and_check_output(&["show", &format!("{revision}:{path}")])
}

fn changed_files(before: &str, after: &str) -> Result<Vec<String>> {
    Ok(
        git::run_and_check_output(&["diff", "--name-only", before, after])?
            .lines()
            .map(str::to_owned)
            .collect(),
    )
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

fn is_ancestor(commit: &str, reference: &str) -> Result<bool> {
    let status = Command::new("git")
        .args(["merge-base", "--is-ancestor", commit, reference])
        .status()
        .context("failed to inspect git ancestry")?;
    match status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        code => bail!("git merge-base failed with status {code:?}"),
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

fn validate_associated_preparation_pr(
    repository: &str,
    sha: &str,
    version: &Version,
) -> Result<()> {
    let endpoint = format!("repos/{repository}/commits/{sha}/pulls");
    let output = Command::new("gh").args(["api", &endpoint]).check_output()?;
    let prs: Vec<AssociatedPullRequest> =
        serde_json::from_str(&output).context("invalid associated pull requests response")?;
    let expected_head = format!("release/prepare-v{version}");
    let matches = prs
        .iter()
        .filter(|pr| {
            pr.merged_at.is_some()
                && pr.base.name == MASTER_BRANCH
                && pr.head.name == expected_head
                && pr.head.repo.as_ref().map(|repo| repo.full_name.as_str()) == Some(repository)
                && pr.merge_commit_sha.as_deref() == Some(sha)
        })
        .count();
    ensure!(
        matches == 1,
        "expected one merged preparation PR for {sha}, found {matches}"
    );
    Ok(())
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

fn update_vrl_to_main(cargo_toml_contents: &str) -> Result<String> {
    let mut doc = cargo_toml_contents
        .parse::<DocumentMut>()
        .context("failed to parse Cargo.toml")?;
    let vrl = doc["workspace"]["dependencies"]["vrl"]
        .as_inline_table_mut()
        .context("vrl in workspace.dependencies must be an inline table")?;
    ensure!(
        vrl.remove("version").is_some(),
        "VRL dependency does not have a pinned version"
    );
    vrl.insert("git", VRL_GIT_URL.into());
    vrl.insert("branch", "main".into());
    Ok(doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        next_minor_development_version, parse_cargo_version, parse_metadata, release_file_allowed,
        update_vrl_to_main, validate_housekeeping_files, validate_prepared_metadata,
    };
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

    #[test]
    fn derives_next_minor_development_version() {
        let release = "0.59.3".parse().expect("valid version");
        assert_eq!(
            next_minor_development_version(&release).expect("next version"),
            "0.60.0-dev".parse().expect("valid version")
        );
    }

    #[test]
    fn prepared_state_requires_the_exact_base() {
        let release = "0.59.0".parse().expect("valid version");
        let prepared_from = "0123456789abcdef0123456789abcdef01234567";
        let valid = format!(
            r#"{{
                "schema_version": 1,
                "status": "prepared",
                "version": "0.59.0",
                "prepared_from": "{prepared_from}"
            }}"#
        );
        let metadata = parse_metadata(&valid).expect("valid metadata");
        assert!(validate_prepared_metadata(&metadata, &release, prepared_from).is_ok());

        let other_base = "1123456789abcdef0123456789abcdef01234567";
        assert!(validate_prepared_metadata(&metadata, &release, other_base).is_err());
    }

    #[test]
    fn housekeeping_requires_exact_file_set() {
        assert!(
            validate_housekeeping_files(&[
                "Cargo.toml".to_owned(),
                ".github/release-state.json".to_owned(),
                "Cargo.lock".to_owned(),
            ])
            .is_ok()
        );
        assert!(validate_housekeeping_files(&["Cargo.toml".to_owned()]).is_err());
    }

    #[test]
    fn restores_vrl_main_dependency() {
        let input = indoc! {r#"
            [workspace.dependencies]
            vrl = { version = "0.28.0", features = ["cli"] }
        "#};
        let output = update_vrl_to_main(input).expect("restore dependency");
        assert!(output.contains("git = \"https://github.com/vectordotdev/vrl.git\""));
        assert!(output.contains("branch = \"main\""));
        assert!(!output.contains("version = \"0.28.0\""));
    }
}

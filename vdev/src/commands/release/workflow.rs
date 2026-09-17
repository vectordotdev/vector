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

use super::{
    ensure_stable, preparation_branch,
    prepare::{replace_version_references, update_vector_package_version, update_vrl_to_version},
};

/// Helpers used by the GitHub release preparation workflows.
#[derive(clap::Args, Debug)]
pub struct Cli {
    #[command(subcommand)]
    command: WorkflowCommand,
}

#[derive(clap::Subcommand, Debug)]
#[allow(clippy::enum_variant_names)] // These command names identify read-only workflow checks.
enum WorkflowCommand {
    /// Validate a request before generating a release preparation PR.
    PrepareCheck(PrepareCheck),
    /// Validate a generated release preparation PR.
    PrCheck(PrCheck),
    /// Validate an approved minor-release squash merge before creating its refs.
    AutotagCheck(AutotagCheck),
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
    /// Require this VRL pin when reusing a preparation branch for a workflow retry.
    #[arg(long)]
    expected_vrl_version: Option<Version>,
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
    merge_commit_sha: Option<String>,
    user: PullRequestAuthor,
    base: PullRequestRef,
    head: PullRequestRef,
}

#[derive(Debug, Deserialize)]
struct PullRequestAuthor {
    login: String,
}

#[derive(Debug, Deserialize)]
struct PullRequestRef {
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
        }
    }
}

impl PrepareCheck {
    fn exec(self) -> Result<()> {
        ensure_stable(&self.version, "release version")?;
        ensure!(
            self.version.patch == 0,
            "automated preparation supports minor releases only"
        );

        let expected_current = development_version(&self.version)?;
        let current = current_cargo_version()?;
        ensure!(
            current == expected_current,
            "expected Cargo.toml version {expected_current}, found {current}"
        );

        let tag = format!("v{}", self.version);
        ensure!(
            !remote_ref_exists(&format!("refs/tags/{tag}"))?,
            "tag {tag} already exists"
        );

        let branch = preparation_branch(&self.version);
        set_github_output("branch", &branch)?;
        if let Some(url) = find_existing_pr(&self.repository, &branch, &self.bot_app)? {
            println!("Preparation PR already exists: {url}");
            set_github_output("skip", "true")?;
            append_github_step_summary(&format!("Existing preparation PR: {url}"))?;
        } else {
            let resume = remote_ref_exists(&format!("refs/heads/{branch}"))?;
            set_github_output("resume", if resume { "true" } else { "false" })?;
            set_github_output("skip", "false")?;
        }
        Ok(())
    }
}

impl AutotagCheck {
    fn exec(self) -> Result<()> {
        git::ensure_sha(&self.before_sha, "before SHA")?;
        git::ensure_sha(&self.sha, "release SHA")?;
        ensure!(
            git::run_and_check_output(&["rev-parse", "HEAD"])?.trim() == self.sha,
            "checkout must match the release SHA"
        );
        let version = current_cargo_version()?;
        let previous = cargo_version_at(&self.before_sha)?;
        // Ordinary changes, development bumps, and manual patch releases do not tag.
        if previous == version || version.pre.as_str() == "dev" || version.patch != 0 {
            set_github_output("tag_required", "false")?;
            return Ok(());
        }
        ensure_stable(&version, "release version")?;
        let parents = git::run_and_check_output(&["rev-list", "--parents", "-n", "1", "HEAD"])?;
        ensure!(
            parents.split_whitespace().collect::<Vec<_>>()
                == [self.sha.as_str(), self.before_sha.as_str()],
            "release must be a single squash-merge commit on the frozen base"
        );
        let head_ref = preparation_branch(&version);
        PrCheck {
            base_sha: self.before_sha,
            head_ref: head_ref.clone(),
            expected_vrl_version: None,
        }
        .exec()?;

        let endpoint = format!("repos/{}/commits/{}/pulls", self.repository, self.sha);
        let output = Command::new("gh")
            .args(["api", "--paginate", "--slurp", &endpoint])
            .check_output()?;
        let pages: Vec<Vec<AssociatedPullRequest>> =
            serde_json::from_str(&output).context("invalid associated pull requests response")?;
        let matches = pages
            .iter()
            .flatten()
            .filter(|pr| {
                pr.merged_at.is_some()
                    && pr.merge_commit_sha.as_deref() == Some(self.sha.as_str())
                    && pr.user.login == "vectordotdev-bot[bot]"
                    && pr.base.name == git::MASTER_BRANCH
                    && pr.head.name == head_ref
                    && pr.head.repo.as_ref().map(|repo| repo.full_name.as_str())
                        == Some(self.repository.as_str())
            })
            .count();
        ensure!(
            matches == 1,
            "expected one merged bot preparation PR for {}, found {matches}",
            self.sha
        );
        set_github_output("tag_required", "true")?;
        set_github_output("tag", &format!("v{version}"))?;
        set_github_output(
            "release_branch",
            &format!("v{}.{}", version.major, version.minor),
        )
    }
}

impl PrCheck {
    fn exec(self) -> Result<()> {
        git::ensure_sha(&self.base_sha, "base SHA")?;
        git::ensure_worktree_clean()?;
        let version = parse_preparation_branch(&self.head_ref)?;
        ensure!(
            version.patch == 0,
            "automated preparation supports minor releases only"
        );
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
        validate_linear_history(&self.base_sha)?;
        validate_release_manifest(&self.base_sha, &version, self.expected_vrl_version.as_ref())?;

        let files = changed_files(&self.base_sha, "HEAD")?;
        validate_release_files(&files, &version)?;
        validate_retired_fragments()?;
        validate_version_substitutions(&self.base_sha, &version)?;
        validate_upgrade_guide(&self.base_sha, &files, &version)?;
        for file in [
            format!("website/cue/reference/releases/{version}.cue"),
            format!("website/content/en/releases/{version}.md"),
            "website/cue/reference/versions.cue".to_owned(),
        ] {
            ensure!(
                files.iter().any(|change| change.path == file) && Path::new(&file).is_file(),
                "required release preparation file must be generated: {file}"
            );
        }

        validate_versions_index(&self.base_sha, &version)?;

        // Confirm the prepared manifest and lockfile agree without modifying the lockfile.
        Command::new("cargo")
            .args(["metadata", "--locked", "--format-version", "1"])
            .stdout(Stdio::null())
            .check_run()
    }
}

fn validate_versions_index(base: &str, version: &Version) -> Result<()> {
    const INDEX: &str = "website/cue/reference/versions.cue";
    let previous = git::run_and_check_output(&["show", &format!("{base}:{INDEX}")])?;
    let releases = git::run_and_check_output(&[
        "ls-tree",
        "-r",
        "--name-only",
        "-z",
        base,
        "--",
        "website/cue/reference/releases/",
    ])?;
    let versions = releases.split_terminator('\0').filter_map(|path| {
        path.strip_prefix("website/cue/reference/releases/")?
            .strip_suffix(".cue")?
            .parse::<Version>()
            .ok()
    });
    // Derive history from the frozen base, never from the candidate index.
    let expected = super::generate_cue::render_versions_cue(
        versions.chain(std::iter::once(version.clone())),
        &previous,
    );
    ensure!(
        fs::read_to_string(INDEX)? == expected,
        "versions.cue must match the generated index and preserve release history"
    );
    Ok(())
}

fn validate_version_substitutions(base: &str, version: &Version) -> Result<()> {
    let file = "distribution/install.sh";
    let before = git::run_and_check_output(&["show", &format!("{base}:{file}")])?;
    // Use the frozen base's default, not mutable local/remote release tags.
    let previous = before
        .lines()
        .find_map(|line| {
            line.strip_prefix("VECTOR_VERSION=\"${VECTOR_VERSION:-\"")
                .and_then(|value| value.strip_suffix("\"}\""))
        })
        .context("base installer is missing the default VECTOR_VERSION")?;
    let previous = parse_stable_version(previous, "base installer version")?;
    for file in [
        file,
        "website/cue/reference/administration/interfaces/kubectl.cue",
    ] {
        let before = git::run_and_check_output(&["show", &format!("{base}:{file}")])?;
        ensure!(
            fs::read_to_string(file)? == replace_version_references(&before, &previous, version),
            "{file} may only contain release version substitutions"
        );
    }
    Ok(())
}

fn validate_upgrade_guide(base: &str, files: &[ChangedFile], version: &Version) -> Result<()> {
    let base_fragments = git::run_and_check_output(&[
        "ls-tree",
        "-r",
        "--name-only",
        "-z",
        base,
        "--",
        "changelog.d/",
    ])?;
    if base_fragments
        .split_terminator('\0')
        .any(|file| file.ends_with(".breaking.md"))
    {
        ensure!(
            files.iter().any(|file| {
                file.kind == ChangeKind::Added
                    && release_highlight(&file.path, version)
                    && Path::new(&file.path).is_file()
            }),
            "breaking releases require a generated upgrade guide for {version}"
        );
    }
    Ok(())
}

fn validate_release_manifest(
    base: &str,
    version: &Version,
    expected_vrl_version: Option<&Version>,
) -> Result<()> {
    let actual: toml::Value = toml::from_str(&fs::read_to_string("Cargo.toml")?)?;
    let vrl = actual
        .get("workspace")
        .and_then(|value| value.get("dependencies"))
        .and_then(|value| value.get("vrl"))
        .and_then(|value| value.get("version"))
        .and_then(toml::Value::as_str)
        .context("Cargo.toml must pin VRL to a released registry version")?;
    let vrl_version = parse_stable_version(vrl, "VRL version")?;
    if let Some(expected) = expected_vrl_version {
        ensure!(
            vrl_version == *expected,
            "existing preparation branch pins VRL to {vrl_version}, but requested {expected}"
        );
    }
    let base_manifest = git::run_and_check_output(&["show", &format!("{base}:Cargo.toml")])?;
    let expected = update_vector_package_version(
        &base_manifest,
        &development_version(version)?.to_string(),
        &version.to_string(),
    )?;
    let expected = update_vrl_to_version(&expected, &vrl_version.to_string())?;
    // Compare TOML values so formatting and comments do not affect validation.
    let expected: toml::Value = toml::from_str(&expected)?;
    ensure!(
        actual == expected,
        "Cargo.toml may only change package.version to {version} and pin VRL to {vrl_version}"
    );

    let lock: toml::Value =
        toml::from_str(&fs::read_to_string("Cargo.lock")?).context("failed to parse lock file")?;
    let packages = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .context("Cargo.lock is missing packages")?;
    let mut vrl_packages = packages
        .iter()
        .filter(|package| package.get("name").and_then(toml::Value::as_str) == Some("vrl"));
    let pinned = vrl_packages.next().context("Cargo.lock is missing VRL")?;
    ensure!(
        vrl_packages.next().is_none()
            && pinned.get("version").and_then(toml::Value::as_str) == Some(vrl)
            && pinned.get("source").and_then(toml::Value::as_str)
                == Some("registry+https://github.com/rust-lang/crates.io-index"),
        "Cargo.lock must resolve VRL to the manifest's released registry version {vrl_version}"
    );
    Ok(())
}

fn parse_preparation_branch(branch: &str) -> Result<Version> {
    let version = branch
        .strip_prefix("prepare-v-")
        .and_then(|value| value.strip_suffix("-website"))
        .context("expected prepare-v-<major>-<minor>-<patch>-website")?;
    ensure!(
        version.split('-').count() == 3
            && version
                .split('-')
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())),
        "preparation branch version must contain three numeric components"
    );
    parse_stable_version(&version.replace('-', "."), "preparation branch version")
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

#[derive(Debug, Eq, PartialEq)]
enum ChangeKind {
    Added,
    Modified,
    Deleted,
}

#[derive(Debug, Eq, PartialEq)]
struct ChangedFile {
    kind: ChangeKind,
    path: String,
}

fn validate_release_files(files: &[ChangedFile], version: &Version) -> Result<()> {
    if let Some(file) = files
        .iter()
        .find(|file| !release_file_allowed(&file.path, version))
    {
        bail!("unexpected release preparation file: {}", file.path);
    }
    if let Some(file) = files
        .iter()
        .find(|file| file.kind == ChangeKind::Deleted && !release_file_deletion_allowed(&file.path))
    {
        bail!("release preparation cannot delete file: {}", file.path);
    }
    Ok(())
}

fn validate_retired_fragments() -> Result<()> {
    let tracked = git::run_and_check_output(&[
        "ls-tree",
        "-r",
        "--name-only",
        "-z",
        "HEAD",
        "--",
        "changelog.d/",
    ])?;
    if let Some(fragment) = tracked
        .split_terminator('\0')
        .find(|file| changelog_fragment(file))
    {
        bail!("release preparation must retire changelog fragment: {fragment}");
    }
    Ok(())
}

fn release_file_allowed(file: &str, version: &Version) -> bool {
    matches!(
        file,
        "Cargo.toml"
            | "Cargo.lock"
            | "LICENSE-3rdparty.csv"
            | "distribution/install.sh"
            | "website/cue/reference/administration/interfaces/kubectl.cue"
            | "website/cue/reference/versions.cue"
    ) || file.starts_with("docs/generated/")
        || changelog_fragment(file)
        || release_highlight(file, version)
        || file == format!("website/content/en/releases/{version}.md")
        || file == format!("website/cue/reference/releases/{version}.cue")
}

fn release_highlight(file: &str, version: &Version) -> bool {
    let suffix = format!(
        "-{}-{}-{}-upgrade-guide.md",
        version.major, version.minor, version.patch
    );
    file.strip_prefix("website/content/en/highlights/")
        .and_then(|name| name.strip_suffix(&suffix))
        .is_some_and(|date| {
            date.len() == 10 && chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok()
        })
}

fn release_file_deletion_allowed(file: &str) -> bool {
    file.starts_with("docs/generated/") || changelog_fragment(file)
}

fn changelog_fragment(file: &str) -> bool {
    file != "changelog.d/README.md" && prefixed_file(file, "changelog.d/", ".md")
}

fn prefixed_file(file: &str, prefix: &str, suffix: &str) -> bool {
    file.strip_prefix(prefix)
        .is_some_and(|name| !name.is_empty() && name.ends_with(suffix))
}

fn changed_files(before: &str, after: &str) -> Result<Vec<ChangedFile>> {
    git::run_and_check_output(&["diff", "--name-status", "--no-renames", before, after])?
        .lines()
        .map(|line| {
            let (status, path) = line
                .split_once('\t')
                .with_context(|| format!("invalid git diff entry: {line}"))?;
            let kind = match status {
                "A" => ChangeKind::Added,
                "M" => ChangeKind::Modified,
                "D" => ChangeKind::Deleted,
                _ => bail!("unsupported git diff status {status} for {path}"),
            };
            Ok(ChangedFile {
                kind,
                path: path.to_owned(),
            })
        })
        .collect()
}

fn validate_linear_history(base: &str) -> Result<()> {
    let merge_base = git::run_and_check_output(&["merge-base", base, "HEAD"])?;
    ensure!(
        merge_base.trim() == base,
        "release PR must descend from frozen base {base}"
    );
    let merges = git::run_and_check_output(&["rev-list", "--merges", &format!("{base}..HEAD")])?;
    ensure!(
        merges.trim().is_empty(),
        "release PR must contain only non-merge commits after base {base}"
    );
    Ok(())
}

fn remote_ref_exists(reference: &str) -> Result<bool> {
    let status = Command::new("git")
        .args(["ls-remote", "--exit-code", "origin", reference])
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
            git::MASTER_BRANCH,
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

fn set_github_output(name: &str, value: &str) -> Result<()> {
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

fn append_github_step_summary(line: &str) -> Result<()> {
    if let Some(path) = env::var_os("GITHUB_STEP_SUMMARY") {
        writeln!(
            OpenOptions::new().create(true).append(true).open(path)?,
            "{line}"
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        parse_cargo_version, parse_preparation_branch, preparation_branch, release_file_allowed,
    };
    use indoc::indoc;

    #[test]
    fn preparation_branch_matches_the_documented_format() {
        let version = "0.59.0".parse().unwrap();
        let branch = preparation_branch(&version);
        assert_eq!(branch, "prepare-v-0-59-0-website");
        assert_eq!(parse_preparation_branch(&branch).unwrap(), version);
    }

    #[test]
    fn preparation_branch_rejects_invalid_versions_and_formats() {
        for branch in [
            "release-website-prepare-v0.59.0",
            "release-website-prepare-v0.59.0-rc.1",
            "release-website-prepare-v0.59.0+build",
            "release-website-prepare-v0.59",
            "prepare-v-0-59-0-rc-1-website",
            "prepare-v-0-59-0+build-website",
            "prepare-v-0-59-website",
            "prepare-v-0-59-0-1-website",
            "prepare-v-0--0-website",
            "prepare-v-0-059-0-website",
            "prepare-v-0.59.0-website",
            "prepare-v-0-59-0",
            "unrelated-v0.59.0",
        ] {
            assert!(
                parse_preparation_branch(branch).is_err(),
                "accepted {branch}"
            );
        }
    }

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
        let version = "0.59.0".parse().unwrap();
        assert!(release_file_allowed("Cargo.toml", &version));
        assert!(release_file_allowed(
            "changelog.d/26289.feature.md",
            &version
        ));
        assert!(release_file_allowed(
            "website/cue/reference/releases/0.59.0.cue",
            &version
        ));
        assert!(!release_file_allowed(
            "website/cue/reference/releases/0.58.0.cue",
            &version
        ));
        assert!(!release_file_allowed("src/main.rs", &version));
        assert!(!release_file_allowed("changelog.d/README.txt", &version));
    }
}

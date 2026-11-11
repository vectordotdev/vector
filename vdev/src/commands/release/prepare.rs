#![allow(clippy::print_stdout)]
#![allow(clippy::print_stderr)]

use crate::commands::release::{
    ensure_stable, generate_cue, preparation_branch,
    versions::{VersionOptions, current_cargo_version, preparation_version},
    vrl_changelog,
};
use crate::utils::{command::run_command, git, paths};
use anyhow::{Context, Result, anyhow, bail, ensure};
use semver::Version;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};
use toml_edit::DocumentMut;

const KUBECLT_CUE_FILE: &str = "website/cue/reference/administration/interfaces/kubectl.cue";
const INSTALL_SCRIPT: &str = "distribution/install.sh";

/// Release preparations CLI options.
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    #[command(flatten)]
    versions: VersionOptions,
    /// Dry run. Enabling this will make it so no PRs will be created and no branches will be pushed upstream.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

struct Prepare {
    new_vector_version: Version,
    vrl_version: Version,
    repo_root: PathBuf,
    latest_vector_version: Version,
    release_branch: String,
    release_preparation_branch: String,
    dry_run: bool,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let repo_root = paths::find_repo_root()?;
        env::set_current_dir(&repo_root)?;

        if !self.dry_run {
            if self.versions.vector_version.is_none() {
                git::ensure_worktree_clean().context(
                    "release preparation without --version requires a clean master checkout; commit or stash your changes and retry",
                )?;
                let branch = git::current_branch()?;
                ensure!(
                    branch == git::MASTER_BRANCH,
                    "release preparation without --version must start on master (currently {branch}); switch to master and retry"
                );
                git::run_and_check_output(&[
                    "fetch",
                    "origin",
                    "--tags",
                    "+refs/heads/master:refs/remotes/origin/master",
                ])?;
                ensure!(
                    git::run_and_check_output(&["rev-parse", "HEAD"])?
                        == git::run_and_check_output(&["rev-parse", "origin/master"])?,
                    "release preparation without --version requires master to match origin/master; update your local master and retry"
                );
            } else {
                git::run_and_check_output(&["fetch"])?;
                git::checkout_main_branch()?;
            }
        }
        let version = match self.versions.vector_version {
            Some(ref version) => {
                ensure_stable(version, "release version")?;
                version.clone()
            }
            None => self.versions.resolve_vector()?,
        };
        let release_branch = format!("v{}.{}", version.major, version.minor);
        let release_preparation_branch = preparation_branch(&version);
        if !self.dry_run && self.versions.vector_version.is_none() {
            let master = git::run_and_check_output(&["rev-parse", "HEAD"])?;
            for branch in [&release_branch, &release_preparation_branch] {
                ensure!(
                    !git::branch_exists(branch)?
                        || git::run_and_check_output(&[
                            "rev-parse",
                            &format!("refs/heads/{branch}"),
                        ])? == master,
                    "release preparation without --version requires existing branch {branch} to match master; rename the existing branch and retry"
                );
            }
        }
        let vrl_version = self.versions.resolve_vrl(None)?;

        let prepare = Prepare {
            new_vector_version: version,
            vrl_version,
            repo_root,
            latest_vector_version: git::latest_release_version()?,
            release_branch,
            release_preparation_branch,
            dry_run: self.dry_run,
        };
        prepare.run()
    }
}

impl Prepare {
    pub fn run(&self) -> Result<()> {
        debug!("run");
        self.create_release_branches()?;
        self.prepare_vector_version()?;
        self.pin_vrl_version()?;

        self.generate_release_cue()?;

        self.update_vector_version(&self.repo_root.join(KUBECLT_CUE_FILE))?;
        self.update_vector_version(&self.repo_root.join(INSTALL_SCRIPT))?;

        self.publish_release_preparation()?;

        Ok(())
    }

    /// Steps 1 & 2
    fn create_release_branches(&self) -> Result<()> {
        debug!("create_release_branches");

        if self.dry_run {
            let head = git::run_and_check_output(&["rev-parse", "--abbrev-ref", "HEAD"])
                .unwrap_or_else(|_| "<unknown>".to_string());
            warn!(
                "dry-run: generating changes on HEAD ({}) without switching branches",
                head.trim()
            );
        } else {
            git::checkout_or_create_branch(self.release_branch.as_str())?;
        }

        // Explicit versions must match the selected release branch, not master.
        preparation_version(
            &current_cargo_version()?,
            Some(self.new_vector_version.clone()),
        )?;
        if self.dry_run {
            return Ok(());
        }

        git::push_and_set_upstream(self.release_branch.as_str())?;

        // Step 2: Create a new release preparation branch
        //         The branch website contains 'website' to generate vector.dev preview.
        git::checkout_or_create_branch(self.release_preparation_branch.as_str())?;
        git::push_and_set_upstream(self.release_preparation_branch.as_str())?;
        Ok(())
    }

    fn prepare_vector_version(&self) -> Result<()> {
        debug!("prepare_vector_version");

        let cargo_toml_path = self.repo_root.join("Cargo.toml");
        let contents = fs::read_to_string(&cargo_toml_path).context("Failed to read Cargo.toml")?;
        let expected_version = format!("{}-dev", self.new_vector_version);
        let release_version = self.new_vector_version.to_string();
        let updated_contents =
            update_vector_package_version(&contents, &expected_version, &release_version)?;
        fs::write(&cargo_toml_path, updated_contents).context("Failed to write Cargo.toml")?;

        run_command("cargo update -p vector");
        Ok(())
    }

    /// Step 3
    fn pin_vrl_version(&self) -> Result<()> {
        debug!("pin_vrl_version");
        let cargo_toml_path = &self.repo_root.join("Cargo.toml");
        let contents = fs::read_to_string(cargo_toml_path).context("Failed to read Cargo.toml")?;
        let vrl_version = self.vrl_version.to_string();
        let updated_contents = update_vrl_to_version(&contents, &vrl_version)?;

        fs::write(cargo_toml_path, updated_contents).context("Failed to write Cargo.toml")?;
        run_command("cargo update -p vrl");
        Ok(())
    }

    // Step 4
    fn generate_release_cue(&self) -> Result<()> {
        debug!("generate_release_cue");
        let vrl_notes = vrl_changelog::fetch(&self.vrl_version)?;
        generate_cue::run(
            &self.new_vector_version,
            generate_cue::PullRequestMetadata::Required,
            Some(&vrl_notes),
        )?;
        generate_cue::retire_all_fragments()?;

        debug!("Generated release CUE file");
        Ok(())
    }

    /// Steps 5 & 6: Replace old version with the new version.
    fn update_vector_version(&self, file_path: &Path) -> Result<()> {
        debug!("update_vector_version for {file_path:?}");
        let contents = fs::read_to_string(file_path)
            .map_err(|e| anyhow!("Failed to read {}: {}", file_path.display(), e))?;

        let latest_version = &self.latest_vector_version;
        let new_version = &self.new_vector_version;
        let old_version_str = format!("{}.{}", latest_version.major, latest_version.minor);

        if !contents.contains(&old_version_str) {
            return Err(anyhow!(
                "Could not find version {} to update in {}",
                latest_version,
                file_path.display()
            ));
        }

        let updated_contents = replace_version_references(&contents, latest_version, new_version);

        fs::write(file_path, updated_contents)
            .map_err(|e| anyhow!("Failed to write {}: {}", file_path.display(), e))?;

        Ok(())
    }

    fn publish_release_preparation(&self) -> Result<()> {
        if self.dry_run {
            return Ok(());
        }

        git::add_files_in_current_dir()?;
        git::commit(&format!(
            "chore(releasing): Prepare version {}",
            self.new_vector_version
        ))?;
        self.open_release_pr()
    }

    /// Final step. Create a release prep PR against the release branch.
    fn open_release_pr(&self) -> Result<()> {
        debug!("open_release_pr");
        git::push()?;

        let new_vector_version = &self.new_vector_version;
        let pr_title = format!("chore(releasing): prepare v{new_vector_version} release");
        let pr_body = format!("This PR prepares the release for Vector v{new_vector_version}");
        let gh_status = Command::new("gh")
            .arg("pr")
            .arg("create")
            .arg("--draft")
            .arg("--base")
            .arg(self.release_branch.as_str())
            .arg("--head")
            .arg(self.release_preparation_branch.as_str())
            .arg("--title")
            .arg(&pr_title)
            .arg("--body")
            .arg(&pr_body)
            .arg("--label")
            .arg("no-changelog")
            .current_dir(&self.repo_root)
            .status()?;
        if !gh_status.success() {
            return Err(anyhow!("Failed to create PR with gh CLI"));
        }
        info!("Successfully created PR against {}", self.release_branch);
        Ok(())
    }
}

// FREE FUNCTIONS AFTER THIS LINE

pub(super) fn replace_version_references(
    contents: &str,
    previous: &Version,
    next: &Version,
) -> String {
    contents
        .replace(&previous.to_string(), &next.to_string())
        .replace(
            &format!("{}.{}", previous.major, previous.minor),
            &format!("{}.{}", next.major, next.minor),
        )
}

/// Transforms a Cargo.toml string by replacing vrl's git dependency with a version dependency.
/// Updates the vrl entry in [workspace.dependencies] from git + branch to a version.
pub(super) fn update_vrl_to_version(
    cargo_toml_contents: &str,
    vrl_version: &str,
) -> Result<String> {
    let mut doc = cargo_toml_contents
        .parse::<DocumentMut>()
        .context("Failed to parse Cargo.toml")?;

    // Navigate to workspace.dependencies.vrl
    let vrl_table = doc["workspace"]["dependencies"]["vrl"]
        .as_inline_table_mut()
        .context("vrl in workspace.dependencies should be an inline table")?;

    // Remove git and branch, add version
    vrl_table.remove("git");
    vrl_table.remove("branch");
    vrl_table.insert("version", vrl_version.into());

    Ok(doc.to_string())
}

pub(super) fn update_vector_package_version(
    cargo_toml_contents: &str,
    expected_version: &str,
    release_version: &str,
) -> Result<String> {
    let mut doc = cargo_toml_contents
        .parse::<DocumentMut>()
        .context("Failed to parse Cargo.toml")?;
    let current_version = doc["package"]["version"]
        .as_str()
        .context("package.version should be a string")?;

    if current_version != expected_version {
        bail!("expected package version {expected_version}, found {current_version}");
    }

    doc["package"]["version"] = toml_edit::value(release_version);
    Ok(doc.to_string())
}

#[cfg(test)]
mod tests {
    use crate::commands::release::prepare::{update_vector_package_version, update_vrl_to_version};
    use indoc::indoc;

    #[test]
    fn test_update_vrl_to_version() {
        let input = indoc! {r#"
            [workspace.dependencies]
            some-other-dep = "1.0.0"
            vrl = { git = "https://github.com/vectordotdev/vrl.git", branch = "main", features = ["arbitrary", "cli", "test", "test_framework"] }
            another-dep = "2.0.0"
        "#};

        let result = update_vrl_to_version(input, "0.28.0").expect("should succeed");

        let expected = indoc! {r#"
            [workspace.dependencies]
            some-other-dep = "1.0.0"
            vrl = { features = ["arbitrary", "cli", "test", "test_framework"] , version = "0.28.0" }
            another-dep = "2.0.0"
        "#};

        assert_eq!(result, expected);
    }

    #[test]
    fn test_update_vector_package_version() {
        let input = indoc! {r#"
            [package]
            name = "vector"
            version = "0.59.0-dev"

            [workspace]
            resolver = "2"
        "#};

        let result = update_vector_package_version(input, "0.59.0-dev", "0.59.0")
            .expect("should update the expected development version");
        assert!(result.contains("version = \"0.59.0\""));

        let error = update_vector_package_version(input, "0.58.0-dev", "0.59.0")
            .expect_err("should reject an unexpected starting version");
        assert!(
            error
                .to_string()
                .contains("expected package version 0.58.0-dev")
        );
    }
}

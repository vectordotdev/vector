use anyhow::{Result, bail, Context as _};
use clap::{Args, ValueEnum};
use semver::Version;
use std::fs;
use std::path::PathBuf;

use crate::{git, util};

const PUBLISH_URL: &str = "https://github.com/vectordotdev/vector/actions/workflows/vdev_publish.yml";

/// Release a new version of vdev
///
/// This command automates the vdev release process:
/// 1. Validates the new version
/// 2. Updates vdev/Cargo.toml
/// 3. Commits the change
/// 4. Creates and pushes the release tag
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The release type: major, minor, or patch
    #[arg(value_enum)]
    release_type: ReleaseType,

    /// Skip confirmation prompts
    #[arg(long)]
    yes: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ReleaseType {
    Major,
    Minor,
    Patch,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        // Check git status
        if !git::check_git_repository_clean()? {
            bail!("Release cancelled. You have uncommitted changes in your repository.");
        }

        let repo_root = util::get_repo_root();
        let vdev_dir = repo_root.join("vdev");
        let vdev_cargo_toml = vdev_dir.join("Cargo.toml");
        if !vdev_cargo_toml.exists() {
            bail!("Could not find Cargo.toml at {}", vdev_cargo_toml.display());
        }
        let current_version = read_vdev_version(&vdev_cargo_toml)?;
        info!("Current vdev version: {current_version}");

        let new_version = match self.release_type {
            ReleaseType::Major => Version::new(current_version.major + 1, 0, 0),
            ReleaseType::Minor => Version::new(current_version.major, current_version.minor + 1, 0),
            ReleaseType::Patch => Version::new(current_version.major, current_version.minor, current_version.patch + 1),
        };

        let new_version_str = new_version.to_string();
        info!("Release version: {new_version_str} ({:?})", self.release_type);

        let tag_name = format!("vdev-v{new_version_str}");
        // Confirm before proceeding
        if !self.yes {
            let summary = format_release_plan(&new_version_str, &tag_name);
            info!("{summary}");
            if !util::confirm("Proceed with release?")? {
                bail!("Release cancelled");
            }
        }

        // Update Cargo.toml
        info!("Updating Cargo.toml to version {new_version_str}");
        update_vdev_version(&vdev_cargo_toml, &current_version, &new_version)?;

        let vdev_cargo_toml_relative = vdev_cargo_toml.strip_prefix(&repo_root)
            .unwrap_or(&vdev_cargo_toml);
        git::run_and_check_output(&["add", &vdev_cargo_toml_relative.display().to_string()])?;
        let commit_message = format!("chore(vdev): bump version to {new_version_str}");
        git::commit(&commit_message)?;
        debug!("Created commit: {commit_message}");

        info!("Creating tag {tag_name}");
        git::tag_version(&tag_name)?;

        let current_branch = git::current_branch()?;
        info!("Pushing to origin");
        git::push_branch(&current_branch)?;
        debug!("Pushed branch: {current_branch}");

        git::push_branch(&tag_name)?;
        debug!("Pushed tag: {tag_name}");

        info!("Monitor release workflow: {PUBLISH_URL}");
        Ok(())
    }
}

fn read_vdev_version(cargo_toml_path: &PathBuf) -> Result<Version> {
    let contents = fs::read_to_string(cargo_toml_path)
        .with_context(|| format!("Failed to read {}", cargo_toml_path.display()))?;

    let cargo_toml: util::CargoToml = toml::from_str(&contents)
        .with_context(|| format!("Failed to parse {}", cargo_toml_path.display()))?;

    Version::parse(&cargo_toml.package.version)
        .context("Failed to parse version number")
}

fn update_vdev_version(
    cargo_toml_path: &PathBuf,
    old_version: &Version,
    new_version: &Version,
) -> Result<()> {
    let contents = fs::read_to_string(cargo_toml_path)
        .with_context(|| format!("Failed to read {}", cargo_toml_path.display()))?;

    let old_line = format!("version = \"{old_version}\"");
    let new_line = format!("version = \"{new_version}\"");

    if !contents.contains(&old_line) {
        bail!("Could not find exact version line '{old_line}' in {}", cargo_toml_path.display());
    }

    let updated_contents = contents.replace(&old_line, &new_line);

    fs::write(cargo_toml_path, updated_contents)
        .with_context(|| format!("Failed to write {}", cargo_toml_path.display()))?;

    Ok(())
}

fn format_release_plan(version: &str, tag_name: &str) -> String {
    format!(
        "\nThis will:\n\
           1. Update Cargo.toml to version {version}\n\
           2. Commit the change\n\
           3. Create tag {tag_name}\n\
           4. Push the commit and tag to origin\n"
    )
}

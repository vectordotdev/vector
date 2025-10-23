use anyhow::{Result, bail, Context as _};
use clap::Args;
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
    /// The new vdev version (e.g., 0.2.0)
    version: Version,

    /// Skip confirmation prompts
    #[arg(long)]
    yes: bool,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let repo_root = util::get_repo_root();
        let vdev_dir = repo_root.join("vdev");
        let vdev_cargo_toml = vdev_dir.join("Cargo.toml");

        // Validate the file exists
        if !vdev_cargo_toml.exists() {
            bail!("Could not find Cargo.toml at {}", vdev_cargo_toml.display());
        }

        // Read current version
        let current_version = read_vdev_version(&vdev_cargo_toml)?;
        let version = self.version.to_string();
        let tag_name = format!("vdev-v{version}");

        info!("Current vdev version: {current_version}");
        info!("New vdev version: {version}");

        // Validate version bump
        if self.version <= current_version {
            bail!(
                "New version {} must be greater than current version {}",
                self.version,
                current_version
            );
        }

        // Check git status
        if !git::check_git_repository_clean()? {
            bail!("Release cancelled. You have uncommitted changes in your repository.");
        }

        // Confirm before proceeding
        if !self.yes {
            let summary = format_release_plan(&version, &tag_name);
            info!("{summary}");
            if !confirm("Proceed with release?")? {
                bail!("Release cancelled");
            }
        }

        // Update Cargo.toml
        info!("Updating Cargo.toml to version {version}");
        update_vdev_version(&vdev_cargo_toml, &current_version, &self.version)?;

        // Commit the change
        let vdev_cargo_toml_relative = vdev_cargo_toml.strip_prefix(&repo_root)
            .unwrap_or(&vdev_cargo_toml);
        git::run_and_check_output(&["add", &vdev_cargo_toml_relative.display().to_string()])?;
        let commit_message = format!("chore(vdev): bump version to {version}");
        git::commit(&commit_message)?;
        debug!("Created commit: {commit_message}");

        // Create the tag
        info!("Creating tag {tag_name}");
        git::tag_version(&tag_name)?;

        // Push to remote
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

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("version = ") {
            // Extract version string from: version = "0.1.0"
            let version_str = trimmed
                .strip_prefix("version = \"")
                .and_then(|s| s.strip_suffix('"'))
                .context("Failed to parse version line")?;

            return Version::parse(version_str)
                .context("Failed to parse version number");
        }
    }

    bail!("Could not find version field in {}", cargo_toml_path.display())
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
        bail!(
            "Could not find exact version line '{old_line}' in {}",
            cargo_toml_path.display()
        );
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

#[allow(clippy::print_stdout)]
fn confirm(prompt: &str) -> Result<bool> {
    use std::io::{self, Write};

    print!("{prompt} [y/N] ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    Ok(input.trim().eq_ignore_ascii_case("y") || input.trim().eq_ignore_ascii_case("yes"))
}

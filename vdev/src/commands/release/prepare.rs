#![allow(clippy::print_stdout)]
#![allow(clippy::print_stderr)]

use crate::commands::release::{ensure_stable, generate_cue, preparation_branch};
use crate::utils::{command::run_command, git, paths};
use anyhow::{Context, Result, anyhow, bail};
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
    /// The new Vector version.
    #[arg(long)]
    version: Version,
    /// The new VRL version.
    #[arg(long)]
    vrl_version: Version,
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
        ensure_stable(&self.version, "release version")?;
        ensure_stable(&self.vrl_version, "VRL version")?;
        let repo_root = paths::find_repo_root()?;
        env::set_current_dir(&repo_root)?;

        let prepare = Prepare {
            new_vector_version: self.version.clone(),
            vrl_version: self.vrl_version,
            repo_root,
            latest_vector_version: git::latest_release_version()?,
            release_branch: format!("v{}.{}", self.version.major, self.version.minor),
            release_preparation_branch: preparation_branch(&self.version),
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
            return Ok(());
        }

        // Step 1: Sync with remote and start from master.
        git::run_and_check_output(&["fetch"])?;
        git::checkout_main_branch()?;

        git::checkout_or_create_branch(self.release_branch.as_str())?;
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
        generate_cue::run(
            &self.new_vector_version,
            generate_cue::PullRequestMetadata::Required,
        )?;
        generate_cue::retire_all_fragments()?;

        self.append_vrl_changelog_to_release_cue()?;
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

    fn append_vrl_changelog_to_release_cue(&self) -> Result<()> {
        debug!("append_vrl_changelog_to_release_cue");

        let releases_path = self.repo_root.join("website/cue/reference/releases");
        let version = &self.new_vector_version;
        let cue_path = releases_path.join(format!("{version}.cue"));
        if !cue_path.is_file() {
            return Err(anyhow!("{} not found", cue_path.display()));
        }

        let vrl_changelog = get_vrl_changelog(&self.vrl_version)?;
        let vrl_changelog_block = format_vrl_changelog_block(&vrl_changelog);

        let original = fs::read_to_string(&cue_path)?;
        let updated = insert_block_after_changelog(&original, &vrl_changelog_block);

        let tmp_path = cue_path.with_extension("cue.tmp");
        fs::write(&tmp_path, &updated)?;
        fs::rename(&tmp_path, &cue_path)?;

        run_command(&format!("cue fmt {}", cue_path.display()));
        debug!("Successfully added VRL changelog to the release cue file.");
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

fn format_vrl_changelog_block(changelog: &str) -> String {
    let double_tab = "\t\t";
    let body = changelog
        .lines()
        .map(|line| {
            let line = line.trim();
            if line.starts_with('#') {
                format!("{double_tab}#{line}")
            } else {
                format!("{double_tab}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let opening = "\tvrl_changelog: #\"\"\"";
    let closing = format!("{double_tab}\"\"\"#");

    format!("{opening}\n{body}\n{closing}")
}

fn insert_block_after_changelog(original: &str, block: &str) -> String {
    let mut result = Vec::new();
    let mut inserted = false;
    let mut in_changelog = false;

    for line in original.lines() {
        result.push(line.to_string());

        if line.trim_start().starts_with("changelog:") {
            in_changelog = true;
        }

        // Insert after the closing `]` of the changelog array specifically.
        if !inserted && in_changelog && line.trim() == "]" {
            result.push(String::new()); // empty line before
            result.push(block.to_string());
            inserted = true;
        }
    }

    result.join("\n")
}

fn get_vrl_changelog(version: &Version) -> Result<String> {
    let tag = format!("v{version}");
    let changelog_output = Command::new("gh")
        .args([
            "api",
            &format!("repos/vectordotdev/vrl/contents/CHANGELOG.md?ref={tag}"),
            "-H",
            "Accept: application/vnd.github.raw+json",
        ])
        .output()
        .context("Failed to run `gh api` for VRL CHANGELOG.md")?;

    if !changelog_output.status.success() {
        let stderr = String::from_utf8_lossy(&changelog_output.stderr);
        bail!("gh api CHANGELOG.md failed: {stderr}");
    }

    let changelog =
        String::from_utf8(changelog_output.stdout).context("CHANGELOG.md is not valid UTF-8")?;

    // Extract the first release section (from the first ## to the next ##)
    let mut section = Vec::new();
    let mut found_first = false;
    for line in changelog.lines() {
        if line.starts_with("## ") {
            if found_first {
                break;
            }
            found_first = true;
        }
        if found_first {
            section.push(line);
        }
    }

    if !found_first {
        bail!("No ## headers found in VRL CHANGELOG.md");
    }

    Ok(section.join("\n"))
}

#[cfg(test)]
mod tests {
    use crate::commands::release::prepare::{
        format_vrl_changelog_block, insert_block_after_changelog, update_vector_package_version,
        update_vrl_to_version,
    };
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

    #[test]
    fn test_insert_block_after_changelog() {
        let vrl_changelog = "### [0.2.0]\n- Feature\n- Fix";
        let vrl_changelog_block = format_vrl_changelog_block(vrl_changelog);

        let expected = concat!(
            "\tvrl_changelog: #\"\"\"\n",
            "\t\t#### [0.2.0]\n",
            "\t\t- Feature\n",
            "\t\t- Fix\n",
            "\t\t\"\"\"#"
        );

        assert_eq!(vrl_changelog_block, expected);

        let original = indoc! {r#"
            version: "1.2.3"
            changelog: [
                {
                    type: "fix"
                    description: "Some fix"
                },
            ]
        "#};
        let updated = insert_block_after_changelog(original, &vrl_changelog_block);

        // Assert the last 5 lines match the VRL changelog block
        let expected_lines_len = 5;
        let updated_tail: Vec<&str> = updated
            .lines()
            .rev()
            .take(expected_lines_len)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let expected_lines: Vec<&str> = vrl_changelog_block.lines().collect();
        assert_eq!(updated_tail, expected_lines);
    }
}

#![allow(clippy::print_stdout)]
#![allow(clippy::print_stderr)]

use crate::commands::release::generate_cue;
use crate::utils::{command::run_command, git, paths};
use anyhow::{Context, Result, anyhow, bail};
use semver::Version;
use std::{
    env, fs,
    fs::File,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::Command,
};
use toml_edit::DocumentMut;

const ALPINE_PREFIX: &str = "FROM docker.io/alpine:";
const ALPINE_DOCKERFILE: &str = "distribution/docker/alpine/Dockerfile";
const DEBIAN_PREFIX: &str = "FROM docker.io/debian:";
const DEBIAN_DOCKERFILE: &str = "distribution/docker/debian/Dockerfile";
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
    /// Optional: The Alpine version to use in `distribution/docker/alpine/Dockerfile`.
    /// You can find the latest version here: <https://alpinelinux.org/releases/>.
    #[arg(long)]
    alpine_version: Option<String>,
    /// Optional: The Debian version to use in `distribution/docker/debian/Dockerfile`.
    /// You can find the latest version here: <https://www.debian.org/releases/>.
    #[arg(long)]
    debian_version: Option<String>,

    /// Dry run. Enabling this will make it so no PRs will be created and no branches will be pushed upstream.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

struct Prepare {
    new_vector_version: Version,
    vrl_version: Version,
    alpine_version: Option<String>,
    debian_version: Option<String>,
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

        let prepare = Prepare {
            new_vector_version: self.version.clone(),
            vrl_version: self.vrl_version,
            alpine_version: self.alpine_version,
            debian_version: self.debian_version,
            repo_root,
            latest_vector_version: git::latest_release_version()?,
            release_branch: format!("v{}.{}", self.version.major, self.version.minor),
            // Websites containing `website` will also generate website previews.
            // Caveat is these branches can only contain alphanumeric chars and dashes.
            release_preparation_branch: format!(
                "prepare-v-{}-{}-{}-website",
                self.version.major, self.version.minor, self.version.patch
            ),
            dry_run: self.dry_run,
        };
        prepare.run()
    }
}

impl Prepare {
    pub fn run(&self) -> Result<()> {
        debug!("run");
        self.create_release_branches()?;
        self.pin_vrl_version()?;

        self.update_dockerfile_base_version(
            &self.repo_root.join(ALPINE_DOCKERFILE),
            self.alpine_version.as_deref(),
            ALPINE_PREFIX,
        )?;

        self.update_dockerfile_base_version(
            &self.repo_root.join(DEBIAN_DOCKERFILE),
            self.debian_version.as_deref(),
            DEBIAN_PREFIX,
        )?;

        self.generate_release_cue()?;

        self.update_vector_version(&self.repo_root.join(KUBECLT_CUE_FILE))?;
        self.update_vector_version(&self.repo_root.join(INSTALL_SCRIPT))?;

        self.add_new_version_to_versions_cue()?;

        self.create_new_release_md()?;

        if !self.dry_run {
            self.open_release_pr()?;
        }

        Ok(())
    }

    /// Steps 1 & 2
    fn create_release_branches(&self) -> Result<()> {
        debug!("create_release_branches");

        if self.dry_run {
            // In dry-run mode the branches (and the commit range fed to
            // generate_cue) are based on whatever is currently checked out.
            // Surface that explicitly so a stale or feature branch doesn't
            // silently land unrelated commits in the generated release CUE.
            let head = git::run_and_check_output(&["rev-parse", "--abbrev-ref", "HEAD"])
                .unwrap_or_else(|_| "<unknown>".to_string());
            let last = &self.latest_vector_version;
            warn!(
                "dry-run: using HEAD ({}) as the release base; \
                 commits in the generated CUE will be `git log v{last}...HEAD`. \
                 Verify this matches what you'd expect from master.",
                head.trim()
            );
        } else {
            // Step 1: Sync with remote and start from master.
            git::run_and_check_output(&["fetch"])?;
            git::checkout_main_branch()?;
        }

        git::checkout_or_create_branch(self.release_branch.as_str())?;
        if !self.dry_run {
            git::push_and_set_upstream(self.release_branch.as_str())?;
        }

        // Step 2: Create a new release preparation branch
        //         The branch website contains 'website' to generate vector.dev preview.
        git::checkout_or_create_branch(self.release_preparation_branch.as_str())?;
        if !self.dry_run {
            git::push_and_set_upstream(self.release_preparation_branch.as_str())?;
        }
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
        git::commit(&format!(
            "chore(releasing): Pinned VRL version to {vrl_version}"
        ))?;
        Ok(())
    }

    /// Step 4 & 5: Update dockerfile versions.
    /// TODO: investigate if this can be automated.
    fn update_dockerfile_base_version(
        &self,
        dockerfile_path: &Path,
        new_version: Option<&str>,
        prefix: &str,
    ) -> Result<()> {
        debug!(
            "update_dockerfile_base_version for {}",
            dockerfile_path.display()
        );
        if let Some(version) = new_version {
            let contents = fs::read_to_string(dockerfile_path)?;

            if !contents.starts_with(prefix) {
                return Err(anyhow::anyhow!(
                    "Dockerfile at {} does not start with {prefix}",
                    dockerfile_path.display()
                ));
            }

            let mut lines = contents.lines();
            let first_line = lines.next().expect("File should have at least one line");
            let rest = lines.collect::<Vec<&str>>().join("\n");

            // Split into prefix, version, and suffix
            // E.g. "FROM docker.io/alpine:", "3.21", " AS builder"
            let after_prefix = first_line.strip_prefix(prefix).ok_or_else(|| {
                anyhow!("Failed to strip prefix in {}", dockerfile_path.display())
            })?;
            let parts: Vec<&str> = after_prefix.splitn(2, ' ').collect();
            let suffix = parts.get(1).unwrap_or(&"");

            // Rebuild with new version
            let updated_version_line = format!("{prefix}{version} {suffix}");
            let new_contents = format!("{updated_version_line}\n{rest}");

            fs::write(dockerfile_path, &new_contents)?;
            git::commit(&format!(
                "chore(releasing): Bump {} version to {version}",
                dockerfile_path
                    .strip_prefix(&self.repo_root)
                    .unwrap()
                    .display(),
            ))?;
        } else {
            debug!("No version specified for {dockerfile_path:?}; skipping update");
        }
        Ok(())
    }

    // Step 6
    fn generate_release_cue(&self) -> Result<()> {
        debug!("generate_release_cue");
        generate_cue::run(&self.new_vector_version)?;
        generate_cue::retire_all_fragments()?;

        self.append_vrl_changelog_to_release_cue()?;
        git::add_files_in_current_dir()?;
        git::commit("chore(releasing): Generated release CUE file")?;
        debug!("Generated release CUE file");
        Ok(())
    }

    /// Step 7 & 8: Replace old version with the new version.
    fn update_vector_version(&self, file_path: &Path) -> Result<()> {
        debug!("update_vector_version for {file_path:?}");
        let contents = fs::read_to_string(file_path)
            .map_err(|e| anyhow!("Failed to read {}: {}", file_path.display(), e))?;

        let latest_version = &self.latest_vector_version;
        let new_version = &self.new_vector_version;
        let old_version_str = format!("{}.{}", latest_version.major, latest_version.minor);
        let new_version_str = format!("{}.{}", new_version.major, new_version.minor);

        if !contents.contains(&old_version_str) {
            return Err(anyhow!(
                "Could not find version {} to update in {}",
                latest_version,
                file_path.display()
            ));
        }

        let updated_contents =
            contents.replace(&latest_version.to_string(), &new_version.to_string());
        let updated_contents = updated_contents.replace(&old_version_str, &new_version_str);

        fs::write(file_path, updated_contents)
            .map_err(|e| anyhow!("Failed to write {}: {}", file_path.display(), e))?;
        git::commit(&format!(
            "chore(releasing): Updated {} vector version to {new_version}",
            file_path.strip_prefix(&self.repo_root).unwrap().display(),
        ))?;

        Ok(())
    }

    /// Step 9: Add new version to `versions.cue`
    fn add_new_version_to_versions_cue(&self) -> Result<()> {
        debug!("add_new_version_to_versions_cue");
        let cure_reference_path = &self.repo_root.join("website").join("cue").join("reference");
        let versions_cue_path = cure_reference_path.join("versions.cue");
        if !versions_cue_path.is_file() {
            return Err(anyhow!("{} not found", versions_cue_path.display()));
        }

        let vector_version = &self.new_vector_version;
        let temp_file_path = cure_reference_path.join(format!("{vector_version}.cue.tmp"));
        let input_file = File::open(&versions_cue_path)?;
        let reader = BufReader::new(input_file);
        let mut output_file = File::create(&temp_file_path)?;

        for line in reader.lines() {
            let line = line?;
            writeln!(output_file, "{line}")?;
            if line.contains("versions:") {
                writeln!(output_file, "\t\"{vector_version}\",")?;
            }
        }

        fs::rename(&temp_file_path, &versions_cue_path)?;

        git::commit(&format!(
            "chore(releasing): Add {vector_version} to versions.cue"
        ))?;
        Ok(())
    }

    /// Step 10: Create a new release md file
    fn create_new_release_md(&self) -> Result<()> {
        debug!("create_new_release_md");
        let releases_dir = self
            .repo_root
            .join("website")
            .join("content")
            .join("en")
            .join("releases");

        let old_version = &self.latest_vector_version;
        let new_version = &self.new_vector_version;
        let old_file_path = releases_dir.join(format!("{old_version}.md"));
        if !old_file_path.exists() {
            return Err(anyhow!(
                "Source file not found: {}",
                old_file_path.display()
            ));
        }

        let content = fs::read_to_string(&old_file_path)?;
        let updated_content = content.replace(&old_version.to_string(), &new_version.to_string());
        let lines: Vec<&str> = updated_content.lines().collect();
        let mut updated_lines = Vec::new();
        let mut weight_updated = false;

        for line in lines {
            if line.trim().starts_with("weight: ") && !weight_updated {
                // Extract the current weight value
                let weight_str = line
                    .trim()
                    .strip_prefix("weight: ")
                    .ok_or_else(|| anyhow!("Invalid weight format"))?;
                let weight: i32 = weight_str
                    .parse()
                    .map_err(|e| anyhow!("Failed to parse weight: {e}"))?;
                // Increase by 1
                let new_weight = weight + 1;
                updated_lines.push(format!("weight: {new_weight}"));
                weight_updated = true;
            } else {
                updated_lines.push(line.to_string());
            }
        }

        if !weight_updated {
            error!("Couldn't update 'weight' line from {old_file_path:?}");
        }

        let new_file_path = releases_dir.join(format!("{new_version}.md"));
        updated_lines.push(String::new()); // File should end with a newline.
        let updated_content = updated_lines.join("\n");
        fs::write(&new_file_path, updated_content)?;
        git::add_files_in_current_dir()?;
        git::commit("chore(releasing): Created release md file")?;
        Ok(())
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

        let vrl_changelog = get_latest_vrl_tag_and_changelog()?;
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

/// Transforms a Cargo.toml string by replacing vrl's git dependency with a version dependency.
/// Updates the vrl entry in [workspace.dependencies] from git + branch to a version.
fn update_vrl_to_version(cargo_toml_contents: &str, vrl_version: &str) -> Result<String> {
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

fn get_latest_vrl_tag_and_changelog() -> Result<String> {
    // Step 1: get the latest tag
    let tag_output = Command::new("gh")
        .args(["api", "repos/vectordotdev/vrl/tags", "--jq", ".[0].name"])
        .output()
        .context("Failed to run `gh api` for VRL tags")?;

    if !tag_output.status.success() {
        let stderr = String::from_utf8_lossy(&tag_output.stderr);
        bail!("gh api tags failed: {stderr}");
    }

    let tag = String::from_utf8(tag_output.stdout).context("gh api output is not valid UTF-8")?;
    let tag = tag.trim().to_string();

    // Step 2: fetch CHANGELOG.md for that tag
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
        format_vrl_changelog_block, insert_block_after_changelog, update_vrl_to_version,
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

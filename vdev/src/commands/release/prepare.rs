use crate::git;
use crate::util::run_command;
use anyhow::{anyhow, Result};
use semver::Version;
use std::fs;
use std::path::{Path, PathBuf};
use toml::map::Map;
use toml::Value;

const ALPINE_PREFIX: &str = "FROM docker.io/alpine:";
const ALPINE_DOCKERFILE: &str = "distribution/docker/alpine/Dockerfile";
const DEBIAN_PREFIX: &str = "FROM docker.io/debian:";
const DEBIAN_DOCKERFILE: &str = "distribution/docker/debian/Dockerfile";
const RELEASE_CUE_SCRIPT: &str = "scripts/generate-release-cue.rb";
const KUBECLT_CUE_FILE: &str = "website/cue/reference/administration/interfaces/kubectl.cue";

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
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        create_release_branches(&self.version)?;
        pin_vrl_version(&self.vrl_version)?;

        update_dockerfile_base_version(
            &get_repo_root().join(ALPINE_DOCKERFILE),
            self.alpine_version.as_deref(),
            ALPINE_PREFIX,
        )?;

        update_dockerfile_base_version(
            &get_repo_root().join(DEBIAN_DOCKERFILE),
            self.debian_version.as_deref(),
            DEBIAN_PREFIX,
        )?;

        generate_release_cue(&self.version)?;

        let latest_version = get_latest_version()?;
        update_cue_vector_version(&latest_version, &self.version)?;

        // TODO automate more steps
        println!("Continue the release preparation process manually.");
        Ok(())
    }
}

fn get_repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn get_latest_version() -> Result<Version> {
    let tags = run_command("git tag --list --sort=-v:refname");
    let latest_tag = tags
        .lines().next()
        .ok_or_else(|| anyhow::anyhow!("No tags found starting with 'v'"))?;

    let version_str = latest_tag.trim_start_matches('v');
    Version::parse(version_str)
        .map_err(|e| anyhow::anyhow!("Failed to parse version from tag '{latest_tag}': {e}"))
}

/// Steps 1 & 2
fn create_release_branches(new_version: &Version) -> Result<()> {
    // Step 1: Create a new release branch
    git::run_and_check_output(&["fetch"])?;
    git::checkout_main_branch()?;
    let release_branch = format!("v{}.{}", new_version.major, new_version.minor);
    git::create_branch(release_branch.as_str())?;
    git::push_and_set_upstream(release_branch.as_str())?;

    // Step 2: Create a new release preparation branch
    //         The branch website contains 'website' to generate vector.dev preview.
    let release_preparation_branch = format!("website-prepare-v{new_version}");
    git::checkout_branch(release_preparation_branch.as_str())?;
    git::push_and_set_upstream(release_preparation_branch.as_str())?;
    Ok(())
}

/// Step 3
fn pin_vrl_version(new_version: &Version) -> Result<()> {
    let cargo_toml_path = get_repo_root().join("Cargo.toml");
    let contents = fs::read_to_string(&cargo_toml_path).expect("Failed to read Cargo.toml");

    // Needs this hybrid approach to preserve ordering.
    let mut lines: Vec<String> = contents.lines().map(String::from).collect();

    for line in &mut lines {
        if line.trim().starts_with("vrl = { git = ") {
            if let Ok(mut vrl_toml) = line.parse::<Value>() {
                let vrl_dependency: &mut Value = vrl_toml.get_mut("vrl").expect("line should start with 'vrl'");

                let mut new_dependency_value = Map::new();
                new_dependency_value.insert("version".to_string(), Value::String(new_version.to_string()));
                let features = vrl_dependency.get("features").expect("missing 'features' key");
                new_dependency_value.insert("features".to_string(), features.clone());

                *line = format!("vrl = {}", Value::from(new_dependency_value));
            }
            break;
        }
    }

    fs::write(&cargo_toml_path, lines.join("\n")).expect("Failed to write Cargo.toml");
    run_command("cargo update -p vrl");
    git::commit(&format!("chore(releasing): Pinned VRL version to {new_version}"))?;
    Ok(())
}

/// Step 4 & 5
fn update_dockerfile_base_version(
    dockerfile_path: &Path,
    new_version: Option<&str>,
    prefix: &str,
) -> Result<()> {
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
        let after_prefix = first_line
            .strip_prefix(prefix)
            .ok_or_else(|| anyhow!("Failed to strip prefix in {}", dockerfile_path.display()))?;
        let parts: Vec<&str> = after_prefix.splitn(2, ' ').collect();
        let suffix = parts.get(1).unwrap_or(&"");

        // Rebuild with new version
        let updated_version_line = format!("{prefix}{version} {suffix}");
        let new_contents = format!("{updated_version_line}\n{rest}");

        fs::write(dockerfile_path, &new_contents)?;
        git::commit(&format!(
            "chore(releasing): Bump {} version to {version}", dockerfile_path.strip_prefix(get_repo_root()).unwrap().display(),
        ))?;
    } else {
        println!(
            "No version specified for {dockerfile_path:?}; skipping update");
    }
    Ok(())
}

// Step 6
fn generate_release_cue(new_version: &Version) -> Result<()> {
    let script = get_repo_root().join(RELEASE_CUE_SCRIPT);
    if script.is_file() {
        run_command(&format!("{} --new-version {new_version} --no-interactive", script.to_string_lossy().as_ref()));
    } else {
        return Err(anyhow!("Script not found: {}", script.display()));
    }
    git::commit("chore(releasing): Generated release CUE file")?;
    println!("Generated release CUE file");
    Ok(())
}

/// Step 7: Find the `_vector_version` line and update the version.
fn update_cue_vector_version(latest_version: &Version, new_version: &Version) -> Result<()> {
    let cue_file_path = get_repo_root().join(KUBECLT_CUE_FILE);
    let contents = fs::read_to_string(&cue_file_path)
        .map_err(|e| anyhow!("Failed to read {}: {}", cue_file_path.display(), e))?;

    let old_version_str = format!("{}.{}", latest_version.major, latest_version.minor);
    let new_version_str = format!("{}.{}", new_version.major, new_version.minor);

    if !contents.contains(&old_version_str) {
        return Err(anyhow!("Could not find version {} to update in {}",
            latest_version, cue_file_path.display()));
    }

    let updated_contents = contents.replace(&old_version_str, &new_version_str) + "\n"; // Add newline at EOF

    fs::write(&cue_file_path, updated_contents)
        .map_err(|e| anyhow!("Failed to write {}: {}", cue_file_path.display(), e))?;
    git::commit(&format!(
        "chore(releasing): Updated {} vector version to {new_version}",
        cue_file_path.strip_prefix(get_repo_root()).unwrap().display(),
    ))?;

    Ok(())
}

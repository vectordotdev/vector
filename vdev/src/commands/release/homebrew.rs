use crate::git;
use anyhow::Result;
use hex;
use reqwest;
use sha2::Digest;
use std::path::Path;
use std::{env, fs};
use tempfile::TempDir;

/// Releases latest version to the vectordotdev homebrew tap
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// GitHub username for the repository.
    #[arg(long, default_value = "vectordotdev")]
    username: String,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        // Create temporary directory for cloning the homebrew-brew repository
        let td = TempDir::new()?;
        env::set_current_dir(td.path())?;

        debug!("Cloning the homebrew repository for username: {}", self.username);
        clone_and_setup_git(&self.username)?;

        let vector_version = env::var("VECTOR_VERSION")?;
        debug!("Updating the vector.rb formula for VECTOR_VERSION={vector_version}.");
        update_formula("Formula/vector.rb", &vector_version)?;

        debug!("Committing and pushing changes (if any).");
        commit_and_push_changes(&vector_version)?;

        td.close()?;
        Ok(())
    }
}


/// Clones the repository and sets up Git configuration
fn clone_and_setup_git(username: &str) -> Result<()> {
    let github_token = env::var("GITHUB_TOKEN")?;
    let homebrew_repo = format!(
        "https://{github_token}:x-oauth-basic@github.com/{username}/homebrew-brew"
    );

    git::clone(&homebrew_repo)?;
    env::set_current_dir("homebrew-brew")?;
    git::set_config_value("user.name", "vic")?;
    git::set_config_value("user.email", "vector@datadoghq.com")?;
    Ok(())
}

/// Updates the vector.rb formula with new URLs, SHA256s, and version
fn update_formula<P>(file_path: P, vector_version: &str) -> Result<()>
where
    P: AsRef<Path>,
{
    // URLs and SHA256s for both architectures
    let x86_package_url = format!(
        "https://packages.timber.io/vector/{vector_version}/vector-{vector_version}-x86_64-apple-darwin.tar.gz"
    );
    let x86_package_sha256 = hex::encode(sha2::Sha256::digest(
        reqwest::blocking::get(&x86_package_url)?.bytes()?,
    ));

    let arm_package_url = format!(
        "https://packages.timber.io/vector/{vector_version}/vector-{vector_version}-arm64-apple-darwin.tar.gz"
    );
    let arm_package_sha256 = hex::encode(sha2::Sha256::digest(
        reqwest::blocking::get(&arm_package_url)?.bytes()?,
    ));

    let content = fs::read_to_string(&file_path)?;

    // Replace the lines with updated URLs and SHA256s
    let updated_content = content
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("version \"") {
                format!("  version \"{vector_version}\"")
            } else if line.contains("# x86_64 url") {
                format!("      url \"{x86_package_url}\" # x86_64 url")
            } else if line.contains("# x86_64 sha256") {
                format!("      sha256 \"{x86_package_sha256}\" # x86_64 sha256")
            } else if line.contains("# arm64 url") {
                format!("      url \"{arm_package_url}\" # arm64 url")
            } else if line.contains("# arm64 sha256") {
                format!("      sha256 \"{arm_package_sha256}\" # arm64 sha256")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    fs::write(file_path, updated_content)?;
    Ok(())
}

/// Commits and pushes changes if any exist
fn commit_and_push_changes(vector_version: &str) -> Result<()> {
    let has_changes = !git::check_git_repository_clean()?;
    if has_changes {
        debug!("Modified lines {:?}", git::get_modified_files());
        let commit_message = format!("Release Vector {vector_version}");
        git::commit(&commit_message)?;
        git::push()?;
    } else {
        debug!("No changes to push.");
    }
    Ok(())
}

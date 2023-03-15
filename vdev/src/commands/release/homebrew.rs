use anyhow::{Result};
use std::{env, path::PathBuf};
use tempfile::TempDir;
use crate::git;
use hex;
use sha2::Digest;
use reqwest;
use std::path::Path;
use regex;

/// Releases latest version to the vectordotdev homebrew tap
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        // Create temporary directory for cloning the homebrew-brew repository
        let td = TempDir::new()?;
        env::set_current_dir(td.path())?;

        // Set git configurations
        let config_values = &[
            ("user.name", "vic"),
            ("user.email", "vector@datadoghq.com"),
        ];
        git::set_config_values(config_values)?;
        let github_token = env::var("GITHUB_TOKEN")?;

        // Clone the homebrew-brew repository
        let homebrew_dir = PathBuf::from("/homebrew-brew");
        let homebrew_repo = format!("https://{github_token}:x-oauth-basic@github.com/vectordotdev/homebrew-brew");
        git::clone(&homebrew_repo)?;
        env::set_current_dir(&homebrew_dir)?;

        // Get package details for updating Formula/vector.rb
        // TODO: use app::version() when it's checked in to master, currently in another PR here: https://github.com/vectordotdev/vector/pull/16724/files#diff-492220caf4fa036bb031d00a23eaa01aa4a0fd5636b2a789bd18f3ce184ede21
        let vector_version = env::var("VECTOR_VERSION")?;
        let package_url = format!("https://packages.timber.io/vector/{vector_version}/vector-{vector_version}-x86_64-apple-darwin.tar.gz");
        let package_sha256 = hex::encode(sha2::Sha256::digest(reqwest::blocking::get(&package_url)?.bytes()?));

        // Update content of Formula/vector.rb
        let file_path = homebrew_dir.join("Formula").join("vector.rb");
        update_content(file_path.as_path(), &package_url, &package_sha256, &vector_version)?;

        // Check if there is any change in git index
        let has_changes = !git::check_git_repository_clean()?;
        if has_changes {
            let commit_message = format!("Release Vector {vector_version}");
            git::commit(&commit_message)?;
        }
        git::push()?;

        // Remove temporary directory
        td.close()?;
        Ok(())
    }
}

// Open the vector.rb file and update the new content
fn update_content(file_path: &Path, package_url: &str, package_sha256: &str, vector_version: &str) -> Result<()> {
    let content = std::fs::read_to_string(file_path)?;
    let patterns = [
        (format!(r#"url "{package_url}""#), r#"url ".*""#),
        (format!(r#"sha256 "{package_sha256}""#), r#"sha256 ".*""#),
        (format!(r#"version "{vector_version}""#), r#"version ".*""#),
    ];
    let new_content = substitute(content, &patterns);
    std::fs::write(file_path, new_content)?;
    Ok(())
}

fn substitute(mut content: String, patterns: &[(String, &str)]) -> String {
    for (value, pattern) in patterns {
        let re = regex::Regex::new(pattern).unwrap();
        content = re.replace_all(&content, value.as_str()).to_string();
    }
    content
}

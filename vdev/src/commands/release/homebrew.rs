use crate::git;
use anyhow::Result;
use hex;
use regex;
use reqwest;
use sha2::Digest;
use std::env;
use std::path::Path;
use tempfile::TempDir;

/// Releases latest version to the vectordotdev homebrew tap
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        // Create temporary directory for cloning the homebrew-brew repository
        let td = TempDir::new()?;
        env::set_current_dir(td.path())?;

        let github_token = env::var("GITHUB_TOKEN")?;

        // Clone the homebrew-brew repository
        let homebrew_repo =
            format!("https://{github_token}:x-oauth-basic@github.com/vectordotdev/homebrew-brew");
        git::clone(&homebrew_repo)?;
        env::set_current_dir("homebrew-brew")?;

        // Set git configurations
        git::set_config_value("user.name", "vic")?;
        git::set_config_value("user.email", "vector@datadoghq.com")?;

        // Get package details for updating Formula/vector.rb
        let vector_version = env::var("VECTOR_VERSION")?;
        let package_url = format!("https://packages.timber.io/vector/{vector_version}/vector-{vector_version}-x86_64-apple-darwin.tar.gz");
        let package_sha256 = hex::encode(sha2::Sha256::digest(
            reqwest::blocking::get(&package_url)?.bytes()?,
        ));

        // Update content of Formula/vector.rb
        update_content(
            "Formula/vector.rb",
            &package_url,
            &package_sha256,
            &vector_version,
        )?;

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

/// Open the vector.rb file and update the new content
fn update_content<P>(
    file_path: P,
    package_url: &str,
    package_sha256: &str,
    vector_version: &str,
) -> Result<()>
where
    P: AsRef<Path>,
{
    let content = std::fs::read_to_string(&file_path)?;
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

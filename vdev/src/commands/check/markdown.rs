use anyhow::Result;

use crate::app;
use crate::git::git_ls_files;

/// Check that markdown is styled properly
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let files = git_ls_files(Some("*.md"))?;
        if files.is_empty() {
            return Ok(());
        }

        let args: Vec<&str> = vec![
            "--config",
            "scripts/.markdownlintrc",
            // We should fix these as well. Previously these files were not linted.
            "--ignore",
            ".github"
        ]
            .into_iter()
            .chain(files.iter().map(String::as_str))
            .collect();

        app::exec("markdownlint", &args, true)
    }
}

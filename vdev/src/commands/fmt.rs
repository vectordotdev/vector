use anyhow::Result;

use crate::{app, utils::git::git_ls_files};

const PRETTIER_EXTENSIONS: &[&str] = &["*.yml", "*.yaml", "*.js", "*.ts", "*.tsx", "*.json"];

/// Apply format changes across the repository
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        info!("Checking style (trailing spaces, line endings)...");
        app::exec("scripts/check-style.sh", ["--fix"], true)?;

        info!("Formatting Rust code...");
        app::exec("cargo", ["fmt", "--all"], true)?;

        info!("Formatting with prettier...");
        for ext in PRETTIER_EXTENSIONS {
            let files = git_ls_files(Some(ext))?;
            if files.is_empty() {
                continue;
            }
            let args: Vec<&str> = ["--ignore-path", ".prettierignore", "--write"]
                .into_iter()
                .chain(files.iter().map(String::as_str))
                .collect();
            app::exec("prettier", &args, true)?;
        }

        Ok(())
    }
}

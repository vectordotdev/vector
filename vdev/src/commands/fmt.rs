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

        let files: Vec<String> = PRETTIER_EXTENSIONS
            .iter()
            .filter_map(|ext| git_ls_files(Some(ext)).ok())
            .flatten()
            .collect();
        if !files.is_empty() {
            info!("Formatting with prettier...");
            let args: Vec<&str> = std::iter::once("--write")
                .chain(files.iter().map(String::as_str))
                .collect();
            app::exec("prettier", &args, true)?;
        }

        Ok(())
    }
}

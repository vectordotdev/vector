use anyhow::Result;

use crate::{app, commands::style, utils::git::git_ls_files};

pub(crate) const PRETTIER_EXTENSIONS: &[&str] =
    &["*.yml", "*.yaml", "*.js", "*.ts", "*.tsx", "*.json"];

/// Apply format changes across the repository
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        app::set_repo_dir()?;
        style::fix_changed()?;

        info!("Formatting Rust code...");
        app::exec("cargo", ["fmt", "--all"], true)?;

        for ext in PRETTIER_EXTENSIONS {
            let files = git_ls_files(Some(ext))?;
            if files.is_empty() {
                continue;
            }
            info!("Formatting {ext} files with prettier...");
            let args: Vec<&str> = ["--ignore-path", ".prettierignore", "--write"]
                .into_iter()
                .chain(files.iter().map(String::as_str))
                .collect();
            app::exec("prettier", &args, true)?;
        }

        Ok(())
    }
}

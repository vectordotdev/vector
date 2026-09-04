use anyhow::Result;

use crate::{
    app,
    commands::{fmt::PRETTIER_EXTENSIONS, style},
    utils::git::git_ls_files,
};

/// Check that all files are formatted properly
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        app::set_repo_dir()?;
        style::check_all()?;

        info!("Checking Rust formatting...");
        app::exec("cargo", ["fmt", "--", "--check"], true)?;

        for ext in PRETTIER_EXTENSIONS {
            let files = git_ls_files(Some(ext))?;
            if files.is_empty() {
                continue;
            }
            info!("Checking prettier formatting for {ext} files...");
            let args: Vec<&str> = ["--ignore-path", ".prettierignore", "--check"]
                .into_iter()
                .chain(files.iter().map(String::as_str))
                .collect();
            app::exec("prettier", &args, true)?;
        }

        Ok(())
    }
}

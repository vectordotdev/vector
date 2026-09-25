use anyhow::Result;

use crate::{
    app,
    commands::{fmt::PRETTIER_EXTENSIONS, style},
    utils::paths::{files_for_prettier, prettier},
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
            let files = files_for_prettier(ext)?;
            if files.is_empty() {
                continue;
            }
            info!("Checking prettier formatting for {ext} files...");
            let args = ["--check"]
                .into_iter()
                .chain(files.iter().map(String::as_str));
            prettier(args, true)?;
        }

        Ok(())
    }
}

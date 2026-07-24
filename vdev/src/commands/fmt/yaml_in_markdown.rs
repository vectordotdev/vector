use anyhow::Result;

use crate::commands::check::yaml_in_markdown as core;
use crate::utils::git::git_ls_files;

/// Auto-fix YAML code blocks inside Markdown files
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// Print OK status for unchanged blocks
    #[arg(long)]
    show_ok: bool,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let files = git_ls_files(Some("*.md"))?;
        if files.is_empty() {
            return Ok(());
        }

        let any_skipped = core::fix_files(&files, self.show_ok)?;
        if any_skipped {
            std::process::exit(1);
        }

        Ok(())
    }
}

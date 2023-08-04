use anyhow::Result;

use crate::app;

/// Compute the release version of Vector.
#[derive(clap::Args, Debug)]
pub(super) struct Cli {}

impl Cli {
    pub(super) fn exec(self) -> Result<()> {
        app::set_repo_dir()?;
        let version = app::version()?;
        println!("{version}");
        Ok(())
    }
}

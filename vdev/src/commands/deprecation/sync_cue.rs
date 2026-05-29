#![allow(clippy::print_stdout)]

use anyhow::Result;

use crate::utils::{deprecation, paths};

/// Regenerate website/cue/reference/generated/deprecations.json from deprecation.d/
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let repo_root = paths::find_repo_root()?;
        deprecation::sync_deprecations_cue(&repo_root)?;
        println!(
            "Wrote {}",
            repo_root.join(deprecation::DEPRECATIONS_JSON).display()
        );
        Ok(())
    }
}

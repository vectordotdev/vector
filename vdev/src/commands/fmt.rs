use anyhow::Result;

use crate::app;

/// Apply format changes across the repository
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        app::exec("scripts/check-style.sh", ["--fix"], true)?;
        app::exec("cargo", ["fmt", "--all"], true)?;

        // Format vdev (standalone crate)
        info!("Formatting vdev...");
        app::exec("cargo", ["fmt", "--manifest-path=vdev/Cargo.toml", "--all"], true)
    }
}

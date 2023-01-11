use std::path::Path;

use anyhow::Result;

use crate::app;

/// Apply format changes across the repository
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let script = Path::new(app::path()).join("scripts/check-style.sh");
        app::exec(script, ["--fix"])?;
        app::exec_app_path("cargo", ["fmt"])
    }
}

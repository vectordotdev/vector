mod check_newer;

use anyhow::Result;
use clap::Subcommand;

use crate::app;

/// Compute and compare release versions of Vector.
///
/// With no subcommand, prints the current release version (the historical
/// behavior of `vdev version`).
#[derive(clap::Args, Debug)]
pub(super) struct Cli {
    #[command(subcommand)]
    command: Option<Subcommands>,
}

#[derive(Subcommand, Debug)]
enum Subcommands {
    /// Verify one semver version is strictly newer than another
    CheckNewer(check_newer::Cli),
}

impl Cli {
    pub(super) fn exec(self) -> Result<()> {
        match self.command {
            None => {
                app::set_repo_dir()?;
                let version = app::version()?;
                println!("{version}");
                Ok(())
            }
            Some(Subcommands::CheckNewer(cli)) => cli.exec(),
        }
    }
}

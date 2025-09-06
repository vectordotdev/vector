use crate::app::VDevCommand;
use crate::util;
use anyhow::{Result, anyhow};
use glob::glob;

/// Uploads target/artifacts to GitHub releases
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let artifacts = glob("target/artifacts/*")
            .expect("failed to read glob pattern")
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow!("failed to read path: {}", e))?
            .into_iter()
            .map(|p| p.into_os_string().into_string())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow!("failed to turn path into string: {:?}", e))?;

        let version = util::get_version()?;
        VDevCommand::new("gh")
            .in_repo()
            .args(
                [
                    "release",
                    "--repo",
                    "vectordotdev/vector",
                    "create",
                    &format!("v{version}"),
                    "--title",
                    &format!("v{version}"),
                    "--notes",
                    &format!("[View release notes](https://vector.dev/releases/{version})"),
                ]
                .map(String::from)
                .into_iter()
                .chain(artifacts),
            )
            .check_run()
    }
}

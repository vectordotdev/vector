use anyhow::Result;

/// Check that markdown is styled properly
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        anyhow::bail!("PR pre-release test (vdev 0.3.4-pr.25456): intentional failure to verify CI uses new binary")
    }
}

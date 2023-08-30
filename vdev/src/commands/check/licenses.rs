use anyhow::Result;

use crate::app;

/// Check that the 3rd-party license file is up to date
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        app::exec("dd-rust-license-tool", ["check"], true).map_err(|err| {
            info!("Run `cargo vdev build licenses` to regenerate the file");
            err
        })
    }
}

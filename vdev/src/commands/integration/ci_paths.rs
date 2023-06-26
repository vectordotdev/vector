use anyhow::Result;
use clap::Args;

use crate::testing::config::IntegrationTestConfig;

/// Output paths in the repository that are associated with an integration.
/// If any changes are made to these paths, that integration should be tested.
#[derive(Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(&self) -> Result<()> {
        // changes to vector-core should test all integrations
        println!("all-int:");
        println!("- \"lib/vector-core/**\"");

        // paths for each integration are defined in their respective config files.
        for (integration, config) in IntegrationTestConfig::collect_all()? {
            if let Some(paths) = config.paths {
                println!("{integration}:");
                for path in paths {
                    println!("- \"{path}\"");
                }
            }
        }

        Ok(())
    }
}

use anyhow::Result;
use clap::Args;

use crate::testing::{integration::IntegrationTest, state::EnvsDir};

/// Stop an integration test environment
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The integration name to stop
    integration: String,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        if let Some(active) = EnvsDir::new(&self.integration).active()? {
            IntegrationTest::new(self.integration, active)?.stop()
        } else {
            println!("No environment for {:?} is active.", self.integration);
            Ok(())
        }
    }
}

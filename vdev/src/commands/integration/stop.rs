use anyhow::Result;
use clap::Args;

use crate::testing::integration::{self, IntegrationTest, OldIntegrationTest};
use crate::testing::state::EnvsDir;

/// Stop an integration test environment
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The integration name to stop
    integration: String,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        // Temporary hack to run old-style integration tests
        if integration::old_exists(&self.integration)? {
            let integration = OldIntegrationTest::new(&self.integration);
            return integration.stop();
        }

        if let Some(active) = EnvsDir::new(&self.integration).active()? {
            IntegrationTest::new(self.integration, active)?.stop()
        } else {
            println!("No environment for {:?} is active.", self.integration);
            Ok(())
        }
    }
}

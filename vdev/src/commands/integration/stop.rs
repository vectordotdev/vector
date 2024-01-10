use anyhow::Result;
use clap::Args;

use crate::testing::{integration::IntegrationTest, state::EnvsDir};

/// Stop an integration test environment
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The integration name to stop
    integration: String,

    /// If true, remove the runner container compiled with all integration test features
    #[arg(short = 'a', long)]
    all_features: bool,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        if let Some(active) = EnvsDir::new(&self.integration).active()? {
            IntegrationTest::new(self.integration, active, self.all_features, 0)?.stop()
        } else {
            println!("No environment for {:?} is active.", self.integration);
            Ok(())
        }
    }
}

use anyhow::Result;

use crate::testing::{integration::IntegrationTest, state::EnvsDir};

pub(crate) fn exec(integration: &str, path: &str, all_features: bool) -> Result<()> {
    if let Some(active) = EnvsDir::new(integration).active()? {
        IntegrationTest::new(integration, path, active, all_features, 0)?.stop()
    } else {
        println!("No environment for {integration} is active.");
        Ok(())
    }
}

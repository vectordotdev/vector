use anyhow::Result;

use crate::testing::{
    integration::{ComposeTest, ComposeTestLocalConfig},
    state::EnvsDir,
};

pub(crate) fn exec(
    local_config: ComposeTestLocalConfig,
    test_name: &str,
    all_features: bool,
) -> Result<()> {
    if let Some(active) = EnvsDir::new(test_name).active()? {
        ComposeTest::generate(local_config, test_name, active, all_features, 0)?.stop()
    } else {
        println!("No environment for {test_name} is active.");
        Ok(())
    }
}

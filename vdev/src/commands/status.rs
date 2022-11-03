use anyhow::Result;
use clap::Args;

use crate::app::Application;
use crate::testing::config::IntegrationTestConfig;

/// Show information about the current environment
#[derive(Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(&self, app: &Application) -> Result<()> {
        app.display(format!("Branch: {}", app.repo.git.current_branch()?));

        let configs = IntegrationTestConfig::collect_all(&app.repo.path)?;
        let mut changed = vec![];
        for (integration, config) in configs.iter() {
            if config.triggered(app.repo.git.changed_files()?)? {
                changed.push(integration.to_string());
            }
        }
        if !changed.is_empty() {
            app.display("Changed:");
            for integration in changed.iter() {
                app.display(format!("  {}", integration));
            }
        }

        Ok(())
    }
}

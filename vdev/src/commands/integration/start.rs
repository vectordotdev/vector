use anyhow::Result;
use clap::Args;

/// Start an environment
///
/// Note: Integration tests build the test runner image lazily when tests run,
/// not during start. Use `vdev int build` to pre-build the image with all features.
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The integration name
    integration: String,

    /// The desired environment name to start. If omitted, the first environment name is used.
    environment: Option<String>,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        crate::commands::compose_tests::start::exec_integration(
            &self.integration,
            self.environment.as_ref(),
        )
    }
}

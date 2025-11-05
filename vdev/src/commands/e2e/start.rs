use anyhow::Result;
use clap::Args;

/// Start an environment
///
/// E2E tests build the test runner image during start because Vector runs
/// as a service in the compose environment. The image is built with just
/// this test's features for faster builds.
///
/// To pre-build a shared image with all E2E features, use `vdev e2e build` first,
/// then use `--no-build` to skip the build step.
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The e2e test name
    test: String,

    /// The desired environment name to start. If omitted, the first environment name is used.
    environment: Option<String>,

    /// Skip building the test runner image (use pre-built image)
    #[arg(long)]
    no_build: bool,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        crate::commands::compose_tests::start::exec_e2e(
            &self.test,
            self.environment.as_ref(),
            self.no_build,
        )
    }
}

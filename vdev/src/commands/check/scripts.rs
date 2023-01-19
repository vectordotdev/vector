use anyhow::Result;

use crate::{app, git};

/// Check that shell scripts do not have common mistakes
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        app::set_repo_dir()?;

        #[allow(clippy::case_sensitive_file_extension_comparisons)]
        app::exec(
            "shellcheck",
            ["--external-sources", "--shell", "bash"].into_iter().chain(
                git::list_files()?
                    .iter()
                    .filter_map(|name| name.ends_with(".sh").then_some(name.as_str())),
            ),
            true,
        )
    }
}

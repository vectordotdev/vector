use anyhow::Result;

use crate::app::{self, Command, CommandExt as _};

/// Check that shell scripts do not have common mistakes
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        app::set_repo_dir()?;

        #[allow(clippy::case_sensitive_file_extension_comparisons)]
        let scripts: Vec<_> = Command::new("git")
            .arg("ls-files")
            .capture_output()?
            .lines()
            .filter_map(|entry| entry.ends_with(".sh").then(|| entry.to_owned()))
            .collect();

        app::exec(
            "shellcheck",
            ["--external-sources", "--shell", "bash"]
                .into_iter()
                .chain(scripts.iter().map(String::as_str)),
            true,
        )
    }
}

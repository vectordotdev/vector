use anyhow::{Result, Ok};
use std::process::Command;
use crate::app::CommandExt as _;


/// Check that component documentation is up-to-date
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    #[allow(clippy::dbg_macro)]
    pub fn exec(self) -> Result<()> {
        let args = vec!["ls-files", "--full-name", "--modified", "--others", "--exclude-standard"];
        let mut command = Command::new("git");
        command.in_repo();
        let files: Vec<String> = command.args(args).capture_output()?.lines().map(str::to_owned).collect();
        let dirty_component_files: Vec<String> = files
            .iter()
            .filter(|file| file.contains("website/cue/reference/components"))
            .map(|file| format!(" - {file}"))
            .collect();

        // If it is not empty, there are out-of-sync component Cue files in the current branch.
        if !dirty_component_files.is_empty() {
            println!("Found out-of-sync component Cue files in this branch:");
            println!("{}", dirty_component_files.join("\n"));
            println!("Run `make generate-component-docs` locally to update your branch and commit/push the changes.");
            std::process::exit(1);
        }

        Ok(())
    }
}

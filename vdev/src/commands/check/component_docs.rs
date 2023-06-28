use crate::git;
use anyhow::{Ok, Result};

/// Check that component documentation is up-to-date
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let files: Vec<String> = git::get_modified_files()?;
        let dirty_component_files: Vec<String> = files
            .into_iter()
            .filter(|file| file.starts_with("website/cue/reference/components"))
            .collect();

        // If it is not empty, there are out-of-sync component Cue files in the current branch.
        if !dirty_component_files.is_empty() {
            println!("Found out-of-sync component Cue files in this branch:");
            for file in dirty_component_files {
                println!(" - {file}");
            }
            println!("Run `make generate-component-docs` locally to update your branch and commit/push the changes.");
            std::process::exit(1);
        }

        Ok(())
    }
}

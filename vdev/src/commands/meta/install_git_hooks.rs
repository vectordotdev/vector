use anyhow::{Result, Ok};
use std::fs::File;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use crate::git;
use crate::app::CommandExt as _;

const SIGNOFF_HOOK: &str = r#"#!/bin/bash
set -euo pipefail

# Automatically sign off your commits.
#
# Installation:
#
#    cp scripts/signoff-git-hook.sh .git/hooks/commit-msg
#
# It's also possible to symlink the script, however that's a security hazard and
# is not recommended.

NAME="$(git config user.name)"
EMAIL="$(git config user.email)"

if [ -z "$NAME" ]; then
  echo "empty git config user.name"
  exit 1
fi

if [ -z "$EMAIL" ]; then
  echo "empty git config user.email"
  exit 1
fi

git interpret-trailers --if-exists doNothing --trailer \
  "Signed-off-by: $NAME <$EMAIL>" \
  --in-place "$1"

"#;

/// Install a Git commit message hook that verifies
/// that all commits have been signed off.
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let git_dir = git::get_git_dir()?;

        // Create a new directory named hooks in the .git directory if it
        // doesn't already exist.
        let mut command = Command::new("mkdir");
        command.in_repo();
        command.args(["-p", &format!("{git_dir}/hooks")]);

        command.check_run()?;

        let hook_path = Path::new(&git_dir).join("hooks").join("commit-msg");
        let mut file = File::create(hook_path)?;
        file.write_all(SIGNOFF_HOOK.as_bytes())?;
        file.metadata()?.permissions().set_mode(0o755);
        println!("Created signoff script in {git_dir}/hooks/commit-msg");

        Ok(())
    }
}

use anyhow::{Ok, Result};
use std::fs::File;
use std::io::Write;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::app;
use std::path::Path;

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
        let hook_dir = Path::new(app::path()).join(".git").join("hooks");

        // Create a new directory named hooks in the .git directory if it
        // doesn't already exist.
        // Use create_dir_all to avoid Error: File exists (os error 17)"
        std::fs::create_dir_all(&hook_dir)?;

        let file_path = hook_dir.join("commit-msg");
        let mut file = File::create(&file_path)?;
        file.write_all(SIGNOFF_HOOK.as_bytes())?;
        #[cfg(unix)]
        {
            file.metadata()?.permissions().set_mode(0o755);
        }
        println!("Created signoff script in {}", file_path.display());

        Ok(())
    }
}

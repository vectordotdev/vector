use std::{path::PathBuf, process::Command};

use anyhow::{Context, Result, anyhow};
use glob::glob;
use semver::Version;

use super::release_notes;
use crate::{
    app::CommandExt as _,
    utils::{cargo, paths},
};

/// Uploads target/artifacts to GitHub releases
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// Print the release description instead of creating the GitHub release
    #[arg(long)]
    dry_run: bool,

    /// Path to the release notes Handlebars template. Relative paths resolve against the
    /// repository root. Defaults to `vdev/src/commands/release/release_notes.hbs`.
    #[arg(long, env = "VDEV_RELEASE_NOTES_TEMPLATE")]
    template: Option<PathBuf>,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let repo_root = paths::find_repo_root()?;
        let version = cargo::get_version()?;
        let parsed_version = Version::parse(&version)
            .with_context(|| format!("Invalid release version {version:?}"))?;
        let notes = release_notes::generate(
            &repo_root,
            &parsed_version,
            self.template.as_deref(),
            release_notes::Target::Github,
        )?;

        if self.dry_run {
            println!("{notes}");
            return Ok(());
        }

        let artifacts = glob("target/artifacts/*")
            .expect("failed to read glob pattern")
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow!("failed to read path: {e}"))?
            .into_iter()
            .map(|p| p.into_os_string().into_string())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow!("failed to turn path into string: {}", e.to_string_lossy()))?;

        let mut command = Command::new("gh");
        command.in_repo();
        command.args(
            [
                "release",
                "--repo",
                "vectordotdev/vector",
                "create",
                &format!("v{version}"),
                "--title",
                &format!("v{version}"),
                "--notes",
                &notes,
            ]
            .map(String::from)
            .into_iter()
            .chain(artifacts),
        );
        command.check_run()?;
        Ok(())
    }
}

mod channel;
mod generate_cue;
mod github;
mod homebrew;
mod prepare;
mod workflow;

use anyhow::{Result, ensure};
use semver::Version;

use crate::utils::command::ScriptArgs;

fn ensure_stable(version: &Version, label: &str) -> Result<()> {
    ensure!(
        version.pre.is_empty() && version.build.is_empty(),
        "{label} must be a stable semantic version"
    );
    Ok(())
}

fn preparation_branch(version: &Version) -> String {
    // Keep the documented, website-preview-compatible branch format.
    format!(
        "prepare-v-{}-{}-{}-website",
        version.major, version.minor, version.patch
    )
}

/// Manage the release process...
#[derive(clap::Args, Debug)]
pub(super) struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand, Debug)]
enum Commands {
    Channel(channel::Cli),
    /// Build the Vector docker images and optionally push it to the registry
    Docker(ScriptArgs),
    GenerateCue(generate_cue::Cli),
    Github(github::Cli),
    Homebrew(homebrew::Cli),
    Prepare(prepare::Cli),
    Workflow(workflow::Cli),
    /// Uploads archives and packages to AWS S3
    S3(ScriptArgs),
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        match self.command {
            Commands::Channel(cli) => cli.exec(),
            Commands::Docker(args) => args.exec("build-docker.sh"),
            Commands::GenerateCue(cli) => cli.exec(),
            Commands::Github(cli) => cli.exec(),
            Commands::Homebrew(cli) => cli.exec(),
            Commands::Prepare(cli) => cli.exec(),
            Commands::Workflow(cli) => cli.exec(),
            Commands::S3(args) => args.exec("release-s3.sh"),
        }
    }
}

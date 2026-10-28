mod publisher;

use std::path::PathBuf;

use anyhow::{Context as _, Result};

use crate::app;

/// Sign, publish, and recover the apt/yum repositories using native Linux tools.
#[derive(clap::Args, Debug)]
pub(super) struct Cli {
    /// Publisher configuration; use --config until the production integration is installed.
    #[arg(
        long,
        default_value = "distribution/repositories/production.json",
        global = true
    )]
    config: PathBuf,
    /// Persistent workspace exchanged between input, signing, and publishing jobs.
    #[arg(long, default_value = "target/package-repositories", global = true)]
    work: PathBuf,
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand, Debug)]
enum Commands {
    /// Validate a completed Release Suite run and freeze its release assets.
    Fetch {
        #[arg(long)]
        run: u64,
        #[arg(long)]
        tag: String,
        #[arg(long, value_enum, default_value = "stable")]
        channel: Channel,
    },
    /// Read current metadata or restore the previously signed retry bundle.
    Base,
    /// Validate inputs, import the signing subkey, and generate signed metadata.
    Prepare,
    /// Save the signed retry bundle and publish with conditional S3 writes.
    Publish,
    /// Verify the configured public HTTPS endpoints against the signed snapshot.
    Verify,
    /// Print the publication state and its S3 version for operator inspection.
    Status,
    /// Restore an archived package selection with fresh signed metadata.
    Rollback {
        #[command(subcommand)]
        phase: Rollback,
    },
    /// Release an abandoned lock after the fixed credential safety window.
    RecoverLock {
        #[arg(long)]
        expected_version: String,
        /// Confirm the previous process and its in-flight writes have stopped.
        #[arg(long)]
        confirm_owner_stopped: bool,
    },
}

#[derive(clap::Subcommand, Debug)]
enum Rollback {
    /// Download the selected archive and the current/pending roots, or a saved retry.
    Base { snapshot: String },
    /// Sign fresh apt roots for the selected indexes; package bytes stay unchanged.
    Prepare,
    /// Publish the prepared rollback only if its expected state still matches.
    Publish,
    /// Verify public HTTPS objects against the prepared rollback.
    Verify,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum Channel {
    Stable,
    Beta,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        app::set_repo_dir()?;
        std::fs::create_dir_all(&self.work)?;
        let config = self.config.canonicalize().context(
            "Publisher configuration is missing; pass --config with a repository configuration",
        )?;
        publisher::exec(&config, &self.work, self.command)
    }
}

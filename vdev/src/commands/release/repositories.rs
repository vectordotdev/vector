use std::{path::PathBuf, process::Command};

use anyhow::{Context as _, Result, ensure};

use crate::app::{self, CommandExt as _};

/// Build, sign, publish, and recover the apt/yum repositories.
#[derive(clap::Args, Debug)]
pub(super) struct Cli {
    /// Container built with `build-image` from the reviewed publisher source.
    #[arg(long, default_value = "vector-package-publisher:local", global = true)]
    image: String,
    /// Configuration and public key directory; never contains a private key.
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
    /// Build the tools image locally; no registry publication is required.
    BuildImage,
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
    /// Restore archived metadata; this does not downgrade installed packages.
    Rollback { snapshot: String },
    /// Release an abandoned lock after the fixed credential safety window.
    RecoverLock {
        #[arg(long)]
        expected_version: String,
        /// Confirm the previous process and its in-flight writes have stopped.
        #[arg(long)]
        confirm_owner_stopped: bool,
    },
    /// Run the publisher's offline protocol and metadata regressions.
    Test {
        /// Prepare two releases in the workspace with an ephemeral test key.
        #[arg(long, conflicts_with = "s3")]
        packages: bool,
        /// Exercise prepared fixtures against the isolated private S3 sandbox.
        #[arg(long)]
        s3: bool,
    },
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum Channel {
    Stable,
    Beta,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        app::set_repo_dir()?;
        if matches!(self.command, Commands::BuildImage) {
            return Command::new("docker")
                .args(["build", "--tag", &self.image, "distribution/repositories"])
                .check_run();
        }
        std::fs::create_dir_all(&self.work)?;
        let work = self.work.canonicalize()?;
        let config = self
            .config
            .canonicalize()
            .context("Publisher configuration is missing")?;
        let directory = config.parent().context("Configuration has no parent")?;
        ensure!(config.is_file(), "Configuration must be a regular file");
        let mut command = Command::new("docker");
        command.args(["run", "--rm", "--init"]);
        let key_mount = {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt as _;

                let owner = work.metadata()?;
                command.args(["--user", &format!("{}:{}", owner.uid(), owner.gid())]);
                format!(
                    "/run/keys:rw,noexec,nosuid,size=16m,mode=0700,uid={},gid={}",
                    owner.uid(),
                    owner.gid()
                )
            }
            #[cfg(not(unix))]
            {
                "/run/keys:rw,noexec,nosuid,size=16m,mode=0700".to_owned()
            }
        };
        command.args([
            "--cap-drop=ALL",
            "--security-opt=no-new-privileges",
            "--tmpfs",
            &key_mount,
            "--env",
            "HOME=/run/keys",
            "--mount",
            &format!("type=bind,source={},target=/work", work.display()),
            "--mount",
            &format!(
                "type=bind,source={},target=/config,readonly",
                directory.display()
            ),
        ]);
        // Forward names, never values: command logging must not expose credentials.
        for name in self.command.credentials() {
            command.args(["--env", name]);
        }
        command.arg(&self.image).args([
            "--config",
            &format!(
                "/config/{}",
                config
                    .file_name()
                    .context("Missing configuration filename")?
                    .to_string_lossy()
            ),
        ]);
        self.command.add_arguments(&mut command);
        command.check_run()
    }
}

impl Commands {
    fn credentials(&self) -> &'static [&'static str] {
        match self {
            Self::Fetch { .. } => &["GH_TOKEN"],
            Self::Prepare => &["PACKAGE_SIGNING_SUBKEY", "PACKAGE_SIGNING_PASSPHRASE"],
            Self::Base
            | Self::Publish
            | Self::Status
            | Self::Rollback { .. }
            | Self::RecoverLock { .. }
            | Self::Test { s3: true, .. } => &[
                "AWS_ACCESS_KEY_ID",
                "AWS_SECRET_ACCESS_KEY",
                "AWS_SESSION_TOKEN",
                "PACKAGE_REPOSITORIES_ROLE_ARN",
            ],
            _ => &[],
        }
    }

    fn add_arguments(self, command: &mut Command) {
        match self {
            Commands::Fetch { run, tag, channel } => {
                command.args([
                    "fetch",
                    "--run",
                    &run.to_string(),
                    "--tag",
                    &tag,
                    "--channel",
                    match channel {
                        Channel::Stable => "stable",
                        Channel::Beta => "beta",
                    },
                ]);
            }
            Commands::Base => {
                command.arg("base");
            }
            Commands::Prepare => {
                command.arg("prepare");
            }
            Commands::Publish => {
                command.arg("publish");
            }
            Commands::Verify => {
                command.arg("verify");
            }
            Commands::Status => {
                command.arg("status");
            }
            Commands::Rollback { snapshot } => {
                command.args(["rollback", &snapshot]);
            }
            Commands::RecoverLock {
                expected_version,
                confirm_owner_stopped,
            } => {
                command.args(["recover-lock", "--expected-version", &expected_version]);
                if confirm_owner_stopped {
                    command.arg("--confirm-owner-stopped");
                }
            }
            Commands::Test { packages, s3 } => {
                command.arg("test");
                if packages {
                    command.arg("--packages");
                }
                if s3 {
                    command.arg("--s3");
                }
            }
            Commands::BuildImage => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Commands;

    #[test]
    fn signing_and_publishing_receive_disjoint_credentials() {
        let signing = Commands::Prepare.credentials();
        let publishing = Commands::Publish.credentials();
        assert_eq!(
            signing,
            ["PACKAGE_SIGNING_SUBKEY", "PACKAGE_SIGNING_PASSPHRASE"]
        );
        assert!(publishing.contains(&"AWS_SESSION_TOKEN"));
        assert!(signing.iter().all(|name| !publishing.contains(name)));
        assert!(Commands::Verify.credentials().is_empty());
        assert!(
            Commands::Test {
                packages: true,
                s3: false
            }
            .credentials()
            .is_empty()
        );
    }
}

use anyhow::{Context, Result};
use clap::Args;

use crate::utils::{git, semver};

/// Verify that one semver version is strictly newer than another.
///
/// Accepts full semver, including prerelease versions (`0.59.0-rc.1`), which
/// semver orders before their own stable release. Exits nonzero when the new
/// version is not strictly newer, so workflows can use it as a gate. Pass
/// `--allow-equal` when generating from the same version again is legitimate,
/// so only true downgrades are rejected.
///
/// Examples:
///   vdev version check-newer --new 0.59.0 --current 0.58.0
///   vdev version check-newer 0.59.0 0.58.0
///   vdev version check-newer 0.59.0   # current defaults to the latest release tag
#[derive(Args, Debug)]
#[command(visible_alias = "semver")]
pub(super) struct Cli {
    /// Version expected to be strictly newer
    #[arg(long, conflicts_with = "new_positional")]
    new: Option<String>,

    /// Version to compare against
    #[arg(long, requires = "new", conflicts_with = "current_positional")]
    current: Option<String>,

    /// What the versions describe, used in error messages (e.g. `chart`)
    #[arg(long, default_value = "target")]
    what: String,

    /// Permit the new version to equal the current one; reject only downgrades
    #[arg(long)]
    allow_equal: bool,

    /// Shorthand: `vdev version check-newer <NEW> [<CURRENT>]`
    #[arg(value_name = "NEW", conflicts_with_all = ["new", "current"])]
    new_positional: Option<String>,

    /// Defaults to the latest semver release tag when omitted
    #[arg(value_name = "CURRENT", requires = "new_positional")]
    current_positional: Option<String>,
}

impl Cli {
    pub(super) fn exec(self) -> Result<()> {
        let Cli {
            new,
            current,
            what,
            allow_equal,
            new_positional,
            current_positional,
        } = self;

        let (new, current) = match (new, current, new_positional, current_positional) {
            (Some(new), Some(current), _, _) | (None, None, Some(new), Some(current)) => {
                (new, current)
            }
            (None, None, Some(new), None) => {
                let current = git::latest_release_version()
                    .context("could not determine the current version; pass it explicitly")?;
                (new, current.to_string())
            }
            _ => anyhow::bail!("pass --new and --current, or NEW [CURRENT] positionally"),
        };

        let new = semver::parse(&new)?;
        let current = semver::parse(&current)?;
        if allow_equal {
            semver::ensure_newer_or_equal(&new, &current, &what)?;
            println!("{new} is not older than {current}");
        } else {
            semver::ensure_newer(&new, &current, &what)?;
            println!("{new} is newer than {current}");
        }
        Ok(())
    }
}

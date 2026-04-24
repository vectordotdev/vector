// Post-release verification probes.
//
// Each submodule is a single probe that asserts a release artifact is present and reachable
// in a publishing target (APT repo, Docker registries, Homebrew tap, etc.). Probes expose:
//
//   * `pub struct Cli` — clap args; the user can run a probe directly with
//     `vdev release verify <probe> [VERSION]`.
//   * `pub fn verify(version: &str) -> VerifyOutcome` — called by `run_all`; prints detail
//     inline and returns a short summary string on success.
//
// `vdev release verify` with no subcommand runs every probe and prints a summary.

mod apt;
mod docker;
mod github;
mod homebrew;
mod rpm;
mod timber_io;
mod website;

use anyhow::{Context as _, Result, bail};

use crate::utils::git;

/// Verify a Vector release is fully published across every target.
///
/// Runs every probe when invoked without a subcommand.
#[derive(clap::Args, Debug)]
#[command()]
pub(super) struct Cli {
    /// Version to verify (e.g. `0.55.0`). Only used when no subcommand is specified.
    /// Defaults to the most recent `v*` git tag.
    version: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(clap::Subcommand, Debug)]
enum Commands {
    /// Datadog APT repo (apt.vector.dev).
    Apt(apt::Cli),
    /// Docker Hub and GitHub Container Registry images.
    Docker(docker::Cli),
    /// GitHub release assets and SHA256SUMS.
    Github(github::Cli),
    /// Homebrew formula (vectordotdev/homebrew-brew).
    Homebrew(homebrew::Cli),
    /// Datadog YUM repo (yum.vector.dev).
    Rpm(rpm::Cli),
    /// packages.timber.io artifact bucket.
    #[command(name = "timber-io")]
    TimberIo(timber_io::Cli),
    /// vector.dev release page.
    Website(website::Cli),
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        match self.command {
            Some(Commands::Apt(cli)) => cli.exec(),
            Some(Commands::Docker(cli)) => cli.exec(),
            Some(Commands::Github(cli)) => cli.exec(),
            Some(Commands::Homebrew(cli)) => cli.exec(),
            Some(Commands::Rpm(cli)) => cli.exec(),
            Some(Commands::TimberIo(cli)) => cli.exec(),
            Some(Commands::Website(cli)) => cli.exec(),
            None => run_all(&resolve_version(self.version)?),
        }
    }
}

pub enum VerifyOutcome {
    Ok(String),
    Failed(anyhow::Error),
}

type ProbeFn = fn(&str) -> VerifyOutcome;

fn run_all(version: &str) -> Result<()> {
    println!("Verifying Vector {version}");

    let probes: &[(&str, ProbeFn)] = &[
        ("apt", apt::verify),
        ("docker", docker::verify),
        ("github", github::verify),
        ("homebrew", homebrew::verify),
        ("rpm", rpm::verify),
        ("timber-io", timber_io::verify),
        ("website", website::verify),
    ];

    let mut ok = 0usize;
    let mut failed = 0usize;

    for (name, probe) in probes {
        println!();
        println!("== {name} ==");
        match probe(version) {
            VerifyOutcome::Ok(summary) => {
                ok += 1;
                println!("  -> OK: {summary}");
            }
            VerifyOutcome::Failed(e) => {
                failed += 1;
                println!("  -> FAIL: {e:#}");
            }
        }
    }

    println!();
    println!("Summary: {ok} OK, {failed} FAIL");

    if failed > 0 {
        bail!("{failed} probe(s) failed");
    }
    Ok(())
}

/// Resolve a version argument, falling back to the most recent `v[0-9]*` git tag.
///
/// We can't use `Cargo.toml`'s version because it's bumped to the next development
/// version immediately after a release.
pub fn resolve_version(version: Option<String>) -> Result<String> {
    match version {
        Some(v) => Ok(v),
        None => latest_release_tag(),
    }
}

// We use `for-each-ref` sorted by creation date rather than `git describe`, because Vector
// tags releases on release branches (not master), so HEAD-reachability is the wrong signal.
// The glob `v[0-9]*` matches tags like `v0.55.0` while excluding `vdev-v*` (second char is
// `d`, not a digit).
fn latest_release_tag() -> Result<String> {
    let tag = git::run_and_check_output(&[
        "for-each-ref",
        "--sort=-creatordate",
        "--count=1",
        "--format=%(refname:short)",
        "refs/tags/v[0-9]*",
    ])
    .context("finding latest Vector release tag via `git for-each-ref`")?;
    let tag = tag.trim();
    if tag.is_empty() {
        bail!("no matching release tags found (pattern: `v[0-9]*`)");
    }
    let version = tag
        .strip_prefix('v')
        .ok_or_else(|| anyhow::anyhow!("expected tag {tag:?} to start with 'v'"))?;
    Ok(version.to_string())
}

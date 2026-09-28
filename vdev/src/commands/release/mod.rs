mod channel;
mod generate_cue;
mod github;
mod homebrew;
mod prepare;
mod workflow;

use anyhow::{Result, ensure};
use semver::Version;

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

crate::cli_subcommands! {
    "Manage the release process..."
    channel,
    docker,
    generate_cue,
    github,
    homebrew,
    prepare,
    workflow,
    s3,
}

crate::script_wrapper! {
    docker = "Build the Vector docker images and optionally push it to the registry"
        => "build-docker.sh"
}
crate::script_wrapper! {
    s3 = "Uploads archives and packages to AWS S3"
        => "release-s3.sh"
}

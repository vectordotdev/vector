mod channel;
mod generate_cue;
mod github;
mod homebrew;
mod prepare;
mod workflow;

crate::cli_subcommands! {
    "Manage the release process..."
    channel,
    docker,
    generate_cue,
    github,
    homebrew,
    prepare,
    s3,
    workflow,
}

crate::script_wrapper! {
    docker = "Build the Vector docker images and optionally push it to the registry"
        => "build-docker.sh"
}
crate::script_wrapper! {
    s3 = "Uploads archives and packages to AWS S3"
        => "release-s3.sh"
}

crate::cli_subcommands! {
    "Manage the release process..."
    generate_cue,
    mod channel,
    commit,
    docker,
    mod github,
    mod homebrew,
    mod prepare,
    mod push,
    s3,
}

crate::script_wrapper! {
    generate_cue = "Generate the release documentation files"
        => "generate-release-cue.rb"
}
crate::script_wrapper! {
    commit = "Commits and tags the pending release"
        => "release-commit.rb"
}
crate::script_wrapper! {
    docker = "Build the Vector docker images and optionally push it to the registry"
        => "build-docker.sh"
}
crate::script_wrapper! {
    s3 = "Uploads archives and packages to AWS S3"
        => "release-s3.sh"
}

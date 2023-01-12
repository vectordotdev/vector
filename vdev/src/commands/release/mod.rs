crate::cli_subcommands! {
    "Manage the release process"
    generate_cue,
    mod channel,
    commit,
    docker,
    github,
    homebrew,
    mod prepare,
    push,
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
    github = "Determine the appropriate release channel (nightly or latest) based on Git HEAD"
        => "release-github.sh"
}
crate::script_wrapper! {
    homebrew = "Releases latest version to the vectordotdev homebrew tap"
        => "release-homebrew.sh"
}
crate::script_wrapper! {
    push = "Pushes new versions produced by `make release` to the repository"
        => "release-push.sh"
}
crate::script_wrapper! {
    s3 = "Uploads archives and packages to AWS S3"
        => "release-s3.sh"
}

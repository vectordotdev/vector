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
    generate_cue = "generate-release-cue.rb"
        "Generate the release documentation files"
}
crate::script_wrapper! {
    commit = "release-commit.rb"
        "Commits and tags the pending release"
}
crate::script_wrapper! {
    docker = "build-docker.sh"
        "Build the Vector docker images and optionally push it to the registry"
}
crate::script_wrapper! {
    github = "release-github.sh"
        "Determine the appropriate release channel (nightly or latest) based on Git HEAD"
}
crate::script_wrapper! {
    homebrew = "release-homebrew.sh"
        "Releases latest version to the vectordotdev homebrew tap"
}
crate::script_wrapper! {
    push = "release-push.sh"
        "Pushes new versions produced by `make release` to the repository"
}
crate::script_wrapper! {
    s3 = "release-s3.sh"
        "Uploads archives and packages to AWS S3"
}

crate::cli_subcommands! {
    "Generate or Regenerate derived files"
    component_docs,
    manifests,
    release_cue,
}

crate::script_wrapper! {
    component_docs = "scripts/generate-component-docs.rb"
        "Generate component documentation"
}
crate::script_wrapper! {
    manifests = "scripts/generate-manifests.sh"
        "Generate Kubernetes manifests from latest Helm chart"
}
crate::script_wrapper! {
    release_cue = "scripts/generate-release-cue.rb"
        "Generate the release documentation files"
}

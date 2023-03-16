crate::cli_subcommands! {
    "Generate or Regenerate derived files..."
    component_docs,
    manifests,
    mod publish_metadata,
    release_cue,
    mod vrl_wasm,
}

crate::script_wrapper! {
    component_docs = "Generate component documentation"
        => "generate-component-docs.rb"
}
crate::script_wrapper! {
    manifests = "Generate Kubernetes manifests from latest Helm chart"
        => "generate-manifests.sh"
}
crate::script_wrapper! {
    release_cue = "Generate the release documentation files"
        => "generate-release-cue.rb"
}

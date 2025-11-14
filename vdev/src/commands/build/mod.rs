mod licenses;
mod publish_metadata;
mod vector;
mod vrl_wasm;

crate::cli_subcommands! {
    "Build, generate or regenerate components..."
    component_docs,
    licenses,
    manifests,
    publish_metadata,
    release_cue,
    vector,
    vrl_wasm,
}

crate::script_wrapper! {
    component_docs = "Build component documentation"
        => "generate-component-docs.rb"
}
crate::script_wrapper! {
    manifests = "Build Kubernetes manifests from latest Helm chart"
        => "generate-manifests.sh"
}
crate::script_wrapper! {
    release_cue = "Build the release documentation files"
        => "generate-release-cue.rb"
}

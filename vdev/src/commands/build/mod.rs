mod component_docs;
mod licenses;
mod publish_metadata;
mod vector;
mod vrl_docs;
mod vrl_wasm;

crate::cli_subcommands! {
    "Build, generate or regenerate components..."
    component_docs,
    licenses,
    manifests,
    publish_metadata,
    vector,
    vrl_docs,
    vrl_wasm,
}

crate::script_wrapper! {
    manifests = "Build Kubernetes manifests from latest Helm chart"
        => "generate-manifests.sh"
}

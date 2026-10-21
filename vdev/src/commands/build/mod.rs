mod component_docs;
pub(crate) mod component_examples;
mod debian_maintainer_scripts;
pub(crate) mod docs_json;
mod licenses;
mod manifests;
mod publish_metadata;
mod vector;
mod vrl_docs;
mod vrl_wasm;

crate::cli_subcommands! {
    "Build, generate or regenerate components..."
    component_docs,
    component_examples,
    debian_maintainer_scripts,
    docs_json,
    licenses,
    manifests,
    publish_metadata,
    vector,
    vrl_docs,
    vrl_wasm,
}

mod changelog_fragments;
mod component_features;
mod deny;
mod events;
mod examples;
mod fmt;
mod generated_docs;
mod licenses;
mod markdown;
mod rust;
mod scripts;

crate::cli_subcommands! {
    "Check parts of the Vector code base..."
    changelog_fragments,
    generated_docs,
    component_features,
    deny,
    docs,
    events,
    examples,
    fmt,
    licenses,
    markdown,
    rust,
    scripts,
}

// These should eventually be migrated to Rust code

crate::script_wrapper! {
    docs = "Check that all /docs files are valid"
        => "check-docs.sh"
}

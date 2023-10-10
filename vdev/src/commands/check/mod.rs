crate::cli_subcommands! {
    "Check parts of the Vector code base..."
    mod component_docs,
    mod component_features,
    mod deny,
    docs,
    events,
    mod examples,
    mod fmt,
    mod licenses,
    mod markdown,
    mod rust,
    mod scripts,
    version,
}

// These should eventually be migrated to Rust code

crate::script_wrapper! {
    docs = "Check that all /docs files are valid"
        => "check-docs.sh"
}

crate::script_wrapper! {
    events = "Check that events satisfy patterns set in <https://github.com/vectordotdev/vector/blob/master/docs/specs/instrumentation.md>"
        => "check-events"
}

crate::script_wrapper! {
    version = "Check that Vector's version is correct, accounting for recent changes"
        => "check-version.rb"
}

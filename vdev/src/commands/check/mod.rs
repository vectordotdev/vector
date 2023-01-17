crate::cli_subcommands! {
    "Check parts of the Vector code base"
    component_docs,
    component_features,
    mod deny,
    docs,
    events,
    examples,
    mod fmt,
    mod markdown,
    scripts,
    version,
}

// These should eventually be migrated to Rust code

crate::script_wrapper! {
    component_docs = "Check component documentation is up-to-date"
        => "check-component-docs.sh"
}

crate::script_wrapper! {
    component_features = "Check that all component features are set up properly"
        => "check-component-features"
}

crate::script_wrapper! {
    docs = "Check that all /docs files are valid"
        => "check-docs.sh"
}

crate::script_wrapper! {
    events = "Check that events satisfy patterns set in https://github.com/vectordotdev/vector/blob/master/rfcs/2020-03-17-2064-event-driven-observability.md"
        => "check-events"
}

crate::script_wrapper! {
    examples = "Check that the config/example files are valid"
        => "check-examples.sh"
}

crate::script_wrapper! {
    scripts = "Check that scripts do not have common mistakes"
        => "check-scripts.sh"
}

crate::script_wrapper! {
    version = "Check that Vector's version is correct, accounting for recent changes"
        => "check-version.rb"
}

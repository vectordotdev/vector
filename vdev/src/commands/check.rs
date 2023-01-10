crate::cli_subcommands! {
    "Check parts of the Vector code base"
    component_docs,
    component_features,
    deny,
    docs,
    events,
    examples,
    fmt,
    markdown,
    scripts,
    style,
    version,
}

// These should eventually be migrated to Rust code

crate::script_wrapper! {
    component_docs = "scripts/check-component-docs.sh"
        "Check component documentation is up-to-date"
}

crate::script_wrapper! {
    component_features = "scripts/check-component-features"
        "Check that all component features are set up properly"
}

crate::script_wrapper! {
    deny = "scripts/check-deny.sh"
        "Check advisories, licenses, and sources for crate dependencies "
}

crate::script_wrapper! {
    docs = "scripts/check-docs.sh"
        "Check that all /docs files are valid"
}

crate::script_wrapper! {
    events = "scripts/check-events"
        "Check that events satisfy patterns set in https://github.com/vectordotdev/vector/blob/master/rfcs/2020-03-17-2064-event-driven-observability.md"
}

crate::script_wrapper! {
    examples = "scripts/check-examples.sh"
        "Check that the config/example files are valid"
}

crate::script_wrapper! {
    fmt = "scripts/check-fmt.sh"
        "Check that all files are formatted properly"
}

crate::script_wrapper! {
    markdown = "scripts/check-markdown.sh"
        "Check that markdown is styled properly"
}

crate::script_wrapper! {
    scripts = "scripts/check-scripts.sh"
        "Check that scripts do not have common mistakes"
}

crate::script_wrapper! {
    style = "scripts/check-style.sh"
        "Check that all files are styled properly"
}

crate::script_wrapper! {
    version = "scripts/check-version.rb"
        "Check that Vector's version is correct, accounting for recent changes"
}

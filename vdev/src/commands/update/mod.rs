mod datadog_proto;
mod opentelemetry_proto;
mod released_tree;

crate::cli_subcommands! {
    "Refresh vendored protocol definitions from an upstream release"
    opentelemetry_proto,
    datadog_proto,
}

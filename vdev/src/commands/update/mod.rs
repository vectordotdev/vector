mod datadog_metrics_proto;
mod datadog_trace_proto;
mod opentelemetry_proto;
mod released_tree;

crate::cli_subcommands! {
    "Refresh vendored protocol definitions from an upstream release"
    datadog_metrics_proto,
    datadog_trace_proto,
    opentelemetry_proto,
}

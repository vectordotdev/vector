#![allow(clippy::print_stderr)]
#[cfg(feature = "e2e-tests-datadog")]
mod datadog;
#[cfg(feature = "e2e-tests-opentelemetry")]
mod opentelemetry;

#[cfg(all(
    unix,
    feature = "sources-ifile",
    feature = "sources-internal_metrics",
    feature = "transforms-filter",
    feature = "sinks-console"
))]
mod ifile;

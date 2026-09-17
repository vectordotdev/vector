#![allow(clippy::print_stderr)]
#[cfg(feature = "e2e-tests-datadog")]
mod datadog;
#[cfg(feature = "e2e-tests-opentelemetry")]
mod opentelemetry;

//! The AppSignal sink
//!
//! This sink provides downstream support for `AppSignal` to collect logs and a subset of Vector
//! metric types. These events are sent to the `appsignal-endpoint.net` domain, which is part of
//! the `appsignal.com` infrastructure.
//!
//! Logs and metrics are stored on an per app basis and require an app-level Push API key.

mod config;
mod encoder;
mod normalizer;
mod request_builder;
mod service;
mod sink;

#[cfg(all(test, feature = "appsignal-integration-tests"))]
mod integration_tests;
#[cfg(test)]
mod tests;

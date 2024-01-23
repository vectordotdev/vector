//! GCP Cloud Monitoring (formerly Stackdriver Metrics) sink.
//! Sends metrics to [GPC Cloud Monitoring][cloud monitoring].
//!
//! [cloud monitoring]: https://cloud.google.com/monitoring/docs/monitoring-overview
mod config;
mod request_builder;
mod sink;
#[cfg(test)]
mod tests;

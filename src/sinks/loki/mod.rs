//! Loki sink
//!
//! This sink provides downstream support for `Loki` via
//! the (configurable) `/loki/api/v1/push` endpoint.
//!
//! <https://grafana.com/docs/loki/v2.6.x/api/>
//!
//! This sink uses `PartitionBatching` to partition events
//! by streams. There must be at least one valid set of labels.
//!
//! If an event produces no labels, this can happen if the template
//! does not match, we will add a default label `{agent="vector"}`.

use crate::sinks::util::HttpEndpoint;

/// Appends a Loki path while retaining the boundary normalization used by
/// `UriSerde::append_path`: all trailing slashes on the endpoint and all
/// leading slashes on the path collapse to one separator.
fn append_loki_path(endpoint: &HttpEndpoint, path: &str) -> crate::Result<HttpEndpoint> {
    let mut parts = endpoint.as_uri().clone().into_parts();
    let base_path = parts
        .path_and_query
        .as_ref()
        .map(http::uri::PathAndQuery::path)
        .unwrap_or_default();
    let joined = format!(
        "{}/{}",
        base_path.trim_end_matches('/'),
        path.trim_start_matches('/')
    );
    parts.path_and_query = Some(joined.parse()?);

    Ok(HttpEndpoint::new(http::Uri::from_parts(parts)?)?)
}

mod config;
mod event;
mod healthcheck;
#[cfg(feature = "loki-integration-tests")]
#[cfg(test)]
mod integration_tests;
mod service;
mod sink;
#[cfg(test)]
mod tests;

#[cfg(feature = "loki-benches")]
pub use self::config::valid_label_name;
pub use self::config::{LokiConfig, OutOfOrderAction};

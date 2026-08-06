#[cfg(test)]
use vector_lib::event::Metric;
use vector_lib::sensitive_string::SensitiveString;

mod collector;
pub mod exporter;
pub mod remote_write;

use vector_lib::configurable::configurable_component;

/// Authentication strategies.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "strategy")]
#[configurable(metadata(docs::enum_tag_description = "The authentication strategy to use."))]
pub enum PrometheusRemoteWriteAuth {
    /// HTTP Basic Authentication.
    Basic {
        /// Basic authentication username.
        user: String,

        /// Basic authentication password.
        password: String,
    },

    /// Bearer authentication.
    ///
    /// A bearer token (OAuth2, JWT, etc) is passed as-is.
    Bearer {
        /// The bearer token to send.
        token: SensitiveString,
    },

    #[cfg(feature = "aws-core")]
    /// Amazon Prometheus Service-specific authentication.
    Aws(crate::aws::AwsAuthentication),
}

fn default_histogram_buckets() -> Vec<f64> {
    vec![
        0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
    ]
}

fn default_summary_quantiles() -> Vec<f64> {
    vec![0.5, 0.75, 0.9, 0.95, 0.99]
}

#[cfg(test)]
fn distribution_to_agg_histogram(metric: Metric, buckets: &[f64]) -> Option<Metric> {
    // If the metric isn;'t already a distribution, this ends up returning `None`.
    let new_value = metric
        .value()
        .clone()
        .distribution_to_agg_histogram(buckets);
    new_value.map(move |value| metric.with_value(value))
}

#[cfg(test)]
fn distribution_to_ddsketch(metric: Metric) -> Option<Metric> {
    // If the metric isn;'t already a distribution, this ends up returning `None`.
    let new_value = metric.value().clone().distribution_to_sketch();
    new_value.map(move |value| metric.with_value(value))
}

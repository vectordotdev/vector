pub(crate) mod parser;

#[cfg(feature = "kubernetes")]
pub(crate) mod kubernetes_sd;
#[cfg(feature = "sources-prometheus-pushgateway")]
mod pushgateway;
#[cfg(feature = "sources-prometheus-remote-write")]
mod remote_write;
#[cfg(feature = "sources-prometheus-scrape")]
mod scrape;

#[cfg(feature = "sources-prometheus-pushgateway")]
pub use pushgateway::PrometheusPushgatewayConfig;
#[cfg(feature = "sources-prometheus-remote-write")]
pub use remote_write::PrometheusRemoteWriteConfig;
#[cfg(feature = "sources-prometheus-scrape")]
pub use scrape::PrometheusScrapeConfig;

/// Merge an enrichment tag onto a metric, mirroring Prometheus' `honor_labels` semantics.
///
/// - If the metric already has the tag and `honor_label` is true, keep the metric's value as-is.
/// - If the metric already has the tag and `honor_label` is false, move the existing value to
///   `exported_<tag>` and overwrite with the new value.
/// - If the metric does not have the tag, set it to the new value.
///
/// Shared by `prometheus_scrape` and `prometheus_kubernetes_sd` to guarantee identical behavior.
#[cfg(any(feature = "sources-prometheus-scrape", feature = "kubernetes"))]
pub(crate) fn merge_honor_label_tag(
    metric: &mut vector_lib::event::Metric,
    tag: &str,
    value: &str,
    honor_label: bool,
) {
    match (honor_label, metric.tag_value(tag)) {
        (false, Some(old_value)) => {
            metric.replace_tag(format!("exported_{tag}"), old_value);
            metric.replace_tag(tag.to_string(), value.to_string());
        }
        (true, Some(_)) => {}
        (_, None) => {
            metric.replace_tag(tag.to_string(), value.to_string());
        }
    }
}

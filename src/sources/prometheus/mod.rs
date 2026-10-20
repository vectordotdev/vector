pub(crate) mod parser;

#[cfg(feature = "sources-prometheus_pushgateway")]
mod pushgateway;
#[cfg(feature = "sources-prometheus_remote_write")]
mod remote_write;
#[cfg(feature = "sources-prometheus_scrape")]
mod scrape;

#[cfg(feature = "sources-prometheus_pushgateway")]
pub use pushgateway::PrometheusPushgatewayConfig;
#[cfg(feature = "sources-prometheus_remote_write")]
pub use remote_write::PrometheusRemoteWriteConfig;
#[cfg(feature = "sources-prometheus_scrape")]
pub use scrape::PrometheusScrapeConfig;

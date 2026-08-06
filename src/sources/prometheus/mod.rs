pub(crate) mod parser;

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

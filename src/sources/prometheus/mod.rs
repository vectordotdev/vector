pub(crate) mod parser;
mod pushgateway;
#[cfg(feature = "sources-prometheus-remote-write")]
mod remote_write;
#[cfg(feature = "sources-prometheus-scrape")]
mod scrape;

pub use pushgateway::PrometheusPushgatewayConfig;
#[cfg(feature = "sources-prometheus-remote-write")]
pub use remote_write::PrometheusRemoteWriteConfig;
#[cfg(feature = "sources-prometheus-scrape")]
pub use scrape::PrometheusScrapeConfig;

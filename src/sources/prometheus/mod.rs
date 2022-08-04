pub(crate) mod parser;
mod remote_write;
mod scrape;

pub use remote_write::PrometheusRemoteWriteConfig;
pub use scrape::PrometheusScrapeConfig;

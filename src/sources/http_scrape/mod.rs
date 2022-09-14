#[cfg(feature = "sources-http_scrape")]
pub mod scrape;

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "http-scrape-integration-tests"))]
mod integration_tests;

pub use scrape::HttpScrapeConfig;

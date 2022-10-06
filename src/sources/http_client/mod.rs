#[cfg(feature = "sources-http_client")]
pub mod scrape;

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "http-client-integration-tests"))]
mod integration_tests;

pub use scrape::HttpClientConfig;

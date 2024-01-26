#[cfg(feature = "sources-http_client")]
pub mod client;

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "http-client-integration-tests"))]
mod integration_tests;

pub use client::HttpClientConfig;

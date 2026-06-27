mod config;
mod request_builder;
mod service;
mod sink;

#[cfg(all(test, feature = "azure-blob-integration-tests"))]
mod integration_tests;
#[cfg(test)]
mod test;

pub use self::config::AzureBlobSinkConfig;

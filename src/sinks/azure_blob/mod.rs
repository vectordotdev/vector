mod config;
mod request_builder;

#[cfg(feature = "azure-blob-integration-tests")]
#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod test;

use config::AzureBlobSinkConfig;

use crate::config::SinkDescription;

inventory::submit! {
    SinkDescription::new::<AzureBlobSinkConfig>("azure_blob")
}

pub mod config;
#[cfg(all(test, feature = "azure-event-hubs-integration-tests"))]
mod integration_tests;
pub mod service;
pub mod sink;

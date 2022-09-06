mod config;
mod request_builder;
mod retry;
mod service;
mod sink;

#[cfg(all(test, feature = "aws-sqs-integration-tests"))]
mod integration_tests;

pub use self::config::SqsSinkConfig;

mod config;
mod sink;

mod integration_tests;

#[cfg(feature = "codecs-parquet")]
pub use config::S3BatchEncoding;
pub use config::S3SinkConfig;

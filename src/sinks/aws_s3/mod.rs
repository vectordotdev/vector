mod config;
#[cfg(feature = "codecs-parquet")]
mod parquet_sink;
mod sink;

mod integration_tests;

pub use config::S3SinkConfig;
#[cfg(feature = "codecs-parquet")]
pub use config::{EncodedBatchConfig, S3BatchEncoding};

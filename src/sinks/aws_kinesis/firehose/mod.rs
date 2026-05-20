mod config;
#[cfg(feature = "aws-kinesis-firehose-integration-tests")]
#[cfg(test)]
mod integration_tests;
mod record;
mod tests;

use aws_sdk_firehose::{
    Client, operation::put_record_batch::PutRecordBatchError, types::Record as FRecord,
};

pub use self::config::KinesisFirehoseSinkConfig;
pub use super::{
    config::{KinesisSinkBaseConfig, build_sink},
    record::{Record, SendRecord},
    request_builder,
    service::{KinesisResponse, KinesisService},
    sink,
};

pub type KinesisError = PutRecordBatchError;
pub type KinesisRecord = FRecord;
pub type KinesisClient = Client;

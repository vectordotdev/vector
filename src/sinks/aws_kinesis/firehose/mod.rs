mod config;
mod integration_tests;
mod record;
mod tests;

use aws_sdk_firehose::{error::PutRecordBatchError, model::Record as FRecord, Client};

pub use super::{
    config::{build_sink, KinesisSinkBaseConfig},
    record::{Record, SendRecord},
    request_builder,
    service::{KinesisResponse, KinesisService},
    sink,
};

pub use self::config::KinesisFirehoseSinkConfig;

pub type KinesisError = PutRecordBatchError;
pub type KinesisRecord = FRecord;
pub type KinesisClient = Client;

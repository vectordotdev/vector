mod config;
mod integration_tests;
mod record;

use aws_sdk_kinesis::{
    operation::put_records::PutRecordsError, types::PutRecordsRequestEntry, Client,
};

pub use super::{
    config::{build_sink, KinesisSinkBaseConfig},
    record::{Record, SendRecord},
    request_builder,
    service::{KinesisResponse, KinesisService},
    sink,
};

pub use self::config::KinesisStreamsSinkConfig;

pub type KinesisError = PutRecordsError;
pub type KinesisRecord = PutRecordsRequestEntry;
pub type KinesisClient = Client;

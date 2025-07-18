#![cfg(test)]

use vector_lib::codecs::JsonSerializerConfig;

use super::*;
use crate::{
    aws::RegionOrEndpoint,
    config::{SinkConfig, SinkContext},
    sinks::{
        aws_kinesis::firehose::config::{
            KinesisFirehoseDefaultBatchSettings, MAX_PAYLOAD_EVENTS, MAX_PAYLOAD_SIZE,
        },
        util::{batch::BatchError, BatchConfig, Compression},
    },
};

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<KinesisFirehoseSinkConfig>();
}

#[tokio::test]
async fn check_batch_size() {
    // Sink builder should limit the batch size to the upper bound.
    let mut batch = BatchConfig::<KinesisFirehoseDefaultBatchSettings>::default();
    batch.max_bytes = Some(MAX_PAYLOAD_SIZE + 1);

    let base = KinesisSinkBaseConfig {
        stream_name: String::from("test"),
        region: RegionOrEndpoint::with_both("us-east-1", "http://localhost:4566"),
        encoding: JsonSerializerConfig::default().into(),
        compression: Compression::None,
        request: Default::default(),
        tls: None,
        auth: Default::default(),
        request_retry_partial: false,
        acknowledgements: Default::default(),
        partition_key_field: None,
    };

    let config = KinesisFirehoseSinkConfig { batch, base };

    let cx = SinkContext::default();
    let res = config.build(cx).await;

    assert_eq!(
        res.err().and_then(|e| e.downcast::<BatchError>().ok()),
        Some(Box::new(BatchError::MaxBytesExceeded {
            limit: MAX_PAYLOAD_SIZE
        }))
    );
}

#[tokio::test]
async fn check_batch_events() {
    let mut batch = BatchConfig::<KinesisFirehoseDefaultBatchSettings>::default();
    batch.max_events = Some(MAX_PAYLOAD_EVENTS + 1);

    let base = KinesisSinkBaseConfig {
        stream_name: String::from("test"),
        region: RegionOrEndpoint::with_both("us-east-1", "http://localhost:4566"),
        encoding: JsonSerializerConfig::default().into(),
        compression: Compression::None,
        request: Default::default(),
        tls: None,
        auth: Default::default(),
        request_retry_partial: false,
        acknowledgements: Default::default(),
        partition_key_field: None,
    };

    let config = KinesisFirehoseSinkConfig { batch, base };

    let cx = SinkContext::default();
    let res = config.build(cx).await;

    assert_eq!(
        res.err().and_then(|e| e.downcast::<BatchError>().ok()),
        Some(Box::new(BatchError::MaxEventsExceeded {
            limit: MAX_PAYLOAD_EVENTS
        }))
    );
}

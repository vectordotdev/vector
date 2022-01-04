#![cfg(test)]

use super::*;
use crate::{
    aws::RegionOrEndpoint,
    config::{SinkConfig, SinkContext},
    sinks::{
        aws_kinesis_firehose::config::{
            KinesisFirehoseDefaultBatchSettings, MAX_PAYLOAD_EVENTS, MAX_PAYLOAD_SIZE,
        },
        util::{
            batch::BatchError,
            encoding::{EncodingConfig, StandardEncodings},
            BatchConfig, Compression,
        },
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

    let config = KinesisFirehoseSinkConfig {
        stream_name: String::from("test"),
        region: RegionOrEndpoint::with_endpoint("http://localhost:4566"),
        encoding: EncodingConfig::from(StandardEncodings::Json),
        compression: Compression::None,
        batch,
        request: Default::default(),
        assume_role: None,
        auth: Default::default(),
    };

    let cx = SinkContext::new_test();
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

    let config = KinesisFirehoseSinkConfig {
        stream_name: String::from("test"),
        region: RegionOrEndpoint::with_endpoint("http://localhost:4566"),
        encoding: EncodingConfig::from(StandardEncodings::Json),
        compression: Compression::None,
        batch,
        request: Default::default(),
        assume_role: None,
        auth: Default::default(),
    };

    let cx = SinkContext::new_test();
    let res = config.build(cx).await;

    assert_eq!(
        res.err().and_then(|e| e.downcast::<BatchError>().ok()),
        Some(Box::new(BatchError::MaxEventsExceeded {
            limit: MAX_PAYLOAD_EVENTS
        }))
    );
}

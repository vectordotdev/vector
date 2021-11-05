#![cfg(test)]

use super::*;
use crate::config::{SinkConfig, SinkContext};
use crate::rusoto::RegionOrEndpoint;
use crate::sinks::aws_kinesis_firehose::config::{
    BuildError, MAX_PAYLOAD_EVENTS, MAX_PAYLOAD_SIZE,
};
use crate::sinks::util::encoding::EncodingConfig;
use crate::sinks::util::encoding::StandardEncodings;
use crate::sinks::util::{BatchConfig, Compression};

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<KinesisFirehoseSinkConfig>();
}

#[tokio::test]
async fn check_batch_size() {
    let config = KinesisFirehoseSinkConfig {
        stream_name: String::from("test"),
        region: RegionOrEndpoint::with_endpoint("http://localhost:4566".into()),
        encoding: EncodingConfig::from(StandardEncodings::Json),
        compression: Compression::None,
        batch: BatchConfig {
            max_bytes: Some(MAX_PAYLOAD_SIZE + 1),
            ..Default::default()
        },
        request: Default::default(),
        assume_role: None,
        auth: Default::default(),
    };

    let cx = SinkContext::new_test();
    let res = config.build(cx).await;

    assert_eq!(
        res.err().and_then(|e| e.downcast::<BuildError>().ok()),
        Some(Box::new(BuildError::BatchMaxSize))
    );
}

#[tokio::test]
async fn check_batch_events() {
    let config = KinesisFirehoseSinkConfig {
        stream_name: String::from("test"),
        region: RegionOrEndpoint::with_endpoint("http://localhost:4566".into()),
        encoding: EncodingConfig::from(StandardEncodings::Json),
        compression: Compression::None,
        batch: BatchConfig {
            max_events: Some(MAX_PAYLOAD_EVENTS + 1),
            ..Default::default()
        },
        request: Default::default(),
        assume_role: None,
        auth: Default::default(),
    };

    let cx = SinkContext::new_test();
    let res = config.build(cx).await;

    assert_eq!(
        res.err().and_then(|e| e.downcast::<BuildError>().ok()),
        Some(Box::new(BuildError::BatchMaxEvents))
    );
}

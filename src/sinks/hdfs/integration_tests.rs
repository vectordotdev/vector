use super::HdfsConfig;
use crate::sinks::util::Compression;
use crate::{
    config::{SinkConfig, SinkContext},
    test_util::{random_lines_with_stream, random_string},
};
use codecs::{JsonSerializerConfig, NewlineDelimitedEncoderConfig};
use futures::Stream;
use tokio_stream::StreamExt;
use vector_common::finalization::{BatchNotifier, BatchStatusReceiver};
use vector_core::event::EventArray;

#[tokio::test]
async fn hdfs_healthchecks_invalid_node_node() {
    // Point to an invalid endpoint
    let config = config("http://127.0.0.1:1");
    let (_, health_check) = config
        .build(SinkContext::new_test())
        .await
        .expect("config build must with success");
    let result = health_check.await;

    assert!(result.is_err())
}

#[tokio::test]
async fn hdfs_healthchecks_valid_node_node() {
    let config = config(&hdfs_name_node());
    let (_, health_check) = config
        .build(SinkContext::new_test())
        .await
        .expect("config build must with success");
    let result = health_check.await;

    assert!(result.is_ok())
}

#[allow(dead_code)]
fn hdfs_name_node() -> String {
    std::env::var("HDFS_NAME_NODE").unwrap_or_else(|_| "default".into())
}

fn config(name_node: &str) -> HdfsConfig {
    HdfsConfig {
        prefix: random_string(10) + "/date=%F",
        encoding: (
            Some(NewlineDelimitedEncoderConfig::new()),
            JsonSerializerConfig::default(),
        )
            .into(),
        name_node: name_node.to_string(),
        compression: Compression::gzip_default(),
        batch: Default::default(),
        acknowledgements: Default::default(),
    }
}

#[allow(dead_code)]
fn make_events_batch(
    len: usize,
    count: usize,
) -> (
    Vec<String>,
    impl Stream<Item = EventArray>,
    BatchStatusReceiver,
) {
    let (batch, receiver) = BatchNotifier::new_with_receiver();
    let (lines, events) = random_lines_with_stream(len, count, Some(batch));

    (lines, events.map(Into::into), receiver)
}

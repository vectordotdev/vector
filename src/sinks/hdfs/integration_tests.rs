use std::{
    io::{BufRead, BufReader, Cursor},
    time::Duration,
};

use codecs::{encoding::FramingConfig, TextSerializerConfig};
use futures::{stream, Stream, StreamExt, TryStreamExt};
use similar_asserts::assert_eq;
use vector_core::event::{BatchNotifier, BatchStatusReceiver, Event, EventArray, LogEvent};

use super::HdfsConfig;
use crate::{
    config::{SinkConfig, SinkContext},
    sinks::util::{BatchConfig, Compression},
    test_util::{
        components::{run_and_assert_sink_compliance, SINK_TAGS},
        random_lines_with_stream, random_string,
    },
};

#[tokio::test]
async fn hdfs_healthchecks_invalid_node_node() {
    // Point to an invalid endpoint
    let config = config("http://127.0.0.1:1", 10);
    let (_, health_check) = config
        .build(SinkContext::new_test())
        .await
        .expect("config build must with success");
    let result = health_check.await;

    assert!(result.is_err())
}

#[tokio::test]
async fn hdfs_healthchecks_valid_node_node() {
    let config = config(&hdfs_name_node(), 10);
    let (_, health_check) = config
        .build(SinkContext::new_test())
        .await
        .expect("config build must with success");
    let result = health_check.await;

    assert!(result.is_ok())
}

#[tokio::test]
async fn hdfs_rotate_files_after_the_buffer_size_is_reached() {
    let config = config(&hdfs_name_node(), 10);
    let prefix = config.prefix.clone();
    let op = config.build_operator().unwrap();
    let sink = config.build_processor(op.clone()).unwrap();

    let (lines, _events) = random_lines_with_stream(100, 30, None);

    let events = lines.clone().into_iter().enumerate().map(|(i, line)| {
        let mut e = LogEvent::from(line);
        let i = if i < 10 {
            1
        } else if i < 20 {
            2
        } else {
            3
        };
        e.insert("i", i.to_string());
        Event::from(e)
    });

    run_and_assert_sink_compliance(sink, stream::iter(events), &SINK_TAGS).await;

    // Hard-coded sleeps are bad, but we're waiting on localstack's state to converge.
    tokio::time::sleep(Duration::from_secs(1)).await;

    let objects: Vec<_> = op
        .object(&prefix)
        .scan()
        .await
        .unwrap()
        .try_collect()
        .await
        .unwrap();
    assert_eq!(objects.len(), 3);

    let mut response_lines: Vec<Vec<String>> = Vec::new();
    for o in objects {
        let bs = o.read().await.unwrap();
        let buf_read = BufReader::new(Cursor::new(bs));

        response_lines.push(buf_read.lines().map(|l| l.unwrap()).collect());
    }

    assert_eq!(&lines[00..10], response_lines[0].as_slice());
    assert_eq!(&lines[10..20], response_lines[1].as_slice());
    assert_eq!(&lines[20..30], response_lines[2].as_slice());
}

#[allow(dead_code)]
fn hdfs_name_node() -> String {
    std::env::var("HDFS_NAME_NODE").unwrap_or_else(|_| "default".into())
}

fn config(name_node: &str, batch_size: usize) -> HdfsConfig {
    let mut batch = BatchConfig::default();
    batch.max_events = Some(batch_size);
    batch.timeout_secs = Some(5.0);

    HdfsConfig {
        prefix: format!("tmp/{}/date=%F", random_string(10)),
        name_node: name_node.to_string(),

        encoding: (None::<FramingConfig>, TextSerializerConfig::default()).into(),
        compression: Compression::None,
        batch,
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

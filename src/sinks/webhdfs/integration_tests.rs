use std::{
    io::{BufRead, BufReader, Cursor},
    time::Duration,
};

use futures::stream;
use opendal::{Entry, Metakey};
use similar_asserts::assert_eq;
use vector_lib::codecs::{encoding::FramingConfig, TextSerializerConfig};
use vector_lib::event::{Event, LogEvent};

use super::WebHdfsConfig;
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
        .build(SinkContext::default())
        .await
        .expect("config build must with success");
    let result = health_check.await;

    assert!(result.is_err())
}

#[tokio::test]
async fn hdfs_healthchecks_valid_node_node() {
    let config = config(&webhdfs_endpoint(), 10);
    let (_, health_check) = config
        .build(SinkContext::default())
        .await
        .expect("config build must with success");
    let result = health_check.await;

    assert!(result.is_ok())
}

#[tokio::test]
async fn hdfs_rotate_files_after_the_buffer_size_is_reached() {
    let mut config = config(&webhdfs_endpoint(), 10);
    // Include event batch id in prefix to make sure the generated files are
    // in order.
    config.prefix = "%F-{{ .i }}-".to_string();

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

    let mut objects: Vec<Entry> = op
        .list_with("/")
        .recursive(true)
        .metakey(Metakey::Mode)
        .await
        .unwrap();

    // Sort file path in order, because we have the event id in path.
    objects.sort_by(|l, r| l.path().cmp(r.path()));
    assert_eq!(objects.len(), 3);

    let mut response_lines: Vec<Vec<String>> = Vec::new();
    for o in objects {
        let bs = op.read(o.path()).await.unwrap();
        let buf_read = BufReader::new(Cursor::new(bs));

        response_lines.push(buf_read.lines().map(|l| l.unwrap()).collect());
    }

    assert_eq!(&lines[00..10], response_lines[0].as_slice());
    assert_eq!(&lines[10..20], response_lines[1].as_slice());
    assert_eq!(&lines[20..30], response_lines[2].as_slice());
}

fn webhdfs_endpoint() -> String {
    std::env::var("WEBHDFS_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:9870".into())
}

fn config(endpoint: &str, batch_size: usize) -> WebHdfsConfig {
    let mut batch = BatchConfig::default();
    batch.max_events = Some(batch_size);
    batch.timeout_secs = Some(5.0);

    WebHdfsConfig {
        // Write test file in local with random_string.
        root: format!("/tmp/{}/", random_string(10)),
        prefix: "%F-".to_string(),
        endpoint: endpoint.to_string(),

        encoding: (None::<FramingConfig>, TextSerializerConfig::default()).into(),
        compression: Compression::None,
        batch,
        acknowledgements: Default::default(),
    }
}

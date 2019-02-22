#![cfg(feature = "cloudwatch-integration-tests")]

use futures::{future::poll_fn, stream, Sink};
use router::sinks::cloudwatch::{CloudwatchSink, CloudwatchSinkConfig};
use router::test_util::{block_on, random_lines};
use router::Record;

const STREAM_NAME: &'static str = "test-1";
const GROUP_NAME: &'static str = "router";

#[test]
fn test_insert_cloudwatch_log_event() {
    let config = CloudwatchSinkConfig {
        stream_name: STREAM_NAME.into(),
        group_name: GROUP_NAME.into(),
        region: Some("us-east-1".into()),
        buffer_size: 1,
    };

    let sink = CloudwatchSink::new(config).unwrap();

    let lines = random_lines(100).take(10).collect::<Vec<_>>();
    let records = lines
        .iter()
        .map(|line| Record::new_from_line(line.clone()))
        .collect::<Vec<_>>();

    let pump = sink.send_all(stream::iter_ok(records.into_iter()));

    let (mut sink, _) = block_on(pump).unwrap();

    block_on(poll_fn(move || sink.close())).unwrap();
}

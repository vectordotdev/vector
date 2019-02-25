#![cfg(feature = "kinesis-integration-tests")]

use futures::{future::poll_fn, stream, Sink};
use router::sinks::kinesis::{KinesisService, KinesisSinkConfig};
use router::test_util::random_lines;
use router::Record;
use rusoto_core::Region;
use tokio::runtime::Runtime;

const STREAM_NAME: &'static str = "RouterTest";

#[test]
fn test_kinesis_put_records() {
    let config = KinesisSinkConfig {
        stream_name: STREAM_NAME.into(),
        region: "us-east-1".into(),
        buffer_size: 2,
    };

    let sink = KinesisService::new(config);

    let timestamp = chrono::Utc::now();

    let lines = random_lines(100).take(11).collect::<Vec<_>>();
    let records = lines
        .iter()
        .map(|line| Record::new_from_line(line.clone()))
        .collect::<Vec<_>>();

    let pump = sink.send_all(stream::iter_ok(records.into_iter()));

    let mut rt = Runtime::new().unwrap();

    let (mut sink, _) = rt.block_on(pump).unwrap();

    rt.block_on(poll_fn(move || sink.close())).unwrap();
}

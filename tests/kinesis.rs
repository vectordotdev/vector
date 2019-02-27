#![cfg(feature = "kinesis-integration-tests")]

use futures::{future::poll_fn, stream, Future, Sink};
use router::sinks::kinesis::{KinesisService, KinesisSinkConfig};
use router::test_util::random_lines;
use router::Record;
use rusoto_core::Region;
use rusoto_kinesis::{Kinesis, KinesisClient};
use std::sync::Arc;
use tokio::runtime::Runtime;

const STREAM_NAME: &'static str = "RouterTest";

#[test]
fn test_kinesis_put_records() {
    let config = KinesisSinkConfig {
        stream_name: STREAM_NAME.into(),
        region: "us-east-1".into(),
        batch_size: 2,
    };

    let mut rt = Runtime::new().unwrap();

    let sink = KinesisService::new(config);

    let timestamp = chrono::Utc::now().timestamp_millis();

    let input_lines = random_lines(100).take(11).collect::<Vec<_>>();
    let records = input_lines
        .iter()
        .map(|line| Record::new_from_line(line.clone()))
        .collect::<Vec<_>>();

    let pump = sink.send_all(stream::iter_ok(records.into_iter()));

    let (mut sink, _) = rt.block_on(pump).unwrap();

    rt.block_on(poll_fn(move || sink.close())).unwrap();

    std::thread::sleep(std::time::Duration::from_secs(2));

    let timestamp = timestamp as f64 / 1000.0;
    let records = rt
        .block_on(fetch_records(STREAM_NAME.into(), timestamp))
        .unwrap();

    let output_lines = records
        .into_iter()
        .map(|e| String::from_utf8(e.data).unwrap())
        .collect::<Vec<_>>();

    assert_eq!(output_lines, input_lines)
}

fn fetch_records(
    stream_name: String,
    timestamp: f64,
) -> impl Future<Item = Vec<rusoto_kinesis::Record>, Error = ()> {
    let client = Arc::new(KinesisClient::new(Region::UsEast1));

    let stream_name1 = stream_name.clone();
    let describe = rusoto_kinesis::DescribeStreamInput {
        stream_name,
        ..Default::default()
    };

    let client1 = client.clone();
    let client2 = client.clone();

    client
        .describe_stream(describe)
        .map_err(|e| panic!("{:?}", e))
        .map(|res| {
            res.stream_description
                .shards
                .into_iter()
                .next()
                .expect("No shards")
        })
        .map(|shard| shard.shard_id)
        .and_then(move |shard_id| {
            let req = rusoto_kinesis::GetShardIteratorInput {
                stream_name: stream_name1,
                shard_id,
                shard_iterator_type: "AT_TIMESTAMP".into(),
                timestamp: Some(timestamp),
                ..Default::default()
            };

            client1
                .get_shard_iterator(req)
                .map_err(|e| panic!("{:?}", e))
        })
        .map(|iter| iter.shard_iterator.expect("No iterator age produced"))
        .and_then(move |shard_iterator| {
            let req = rusoto_kinesis::GetRecordsInput {
                shard_iterator,
                // limit: Some(limit),
                limit: None,
            };

            client2.get_records(req).map_err(|e| panic!("{:?}", e))
        })
        .map(|records| records.records)
}

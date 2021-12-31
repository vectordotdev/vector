#![cfg(feature = "aws-kinesis-streams-integration-tests")]
#![cfg(test)]

use std::sync::Arc;

use rusoto_core::Region;
use rusoto_kinesis::{Kinesis, KinesisClient};
use tokio::time::{sleep, Duration};

use super::*;
use crate::{
    aws::rusoto::RegionOrEndpoint,
    config::{SinkConfig, SinkContext},
    sinks::util::{encoding::StandardEncodings, BatchConfig, Compression},
    test_util::{components, random_lines_with_stream, random_string},
};

#[tokio::test]
async fn kinesis_put_records() {
    let stream = gen_stream();

    let region = Region::Custom {
        name: "localstack".into(),
        endpoint: "http://localhost:4566".into(),
    };

    ensure_stream(region.clone(), stream.clone()).await;

    let mut batch = BatchConfig::default();
    batch.max_events = Some(2);

    let config = KinesisSinkConfig {
        stream_name: stream.clone(),
        partition_key_field: None,
        region: RegionOrEndpoint::with_endpoint("http://localhost:4566"),
        encoding: StandardEncodings::Text.into(),
        compression: Compression::None,
        batch,
        request: Default::default(),
        assume_role: None,
        auth: Default::default(),
    };

    let cx = SinkContext::new_test();

    let sink = config.build(cx).await.unwrap().0;

    let timestamp = chrono::Utc::now().timestamp_millis();

    let (mut input_lines, events) = random_lines_with_stream(100, 11, None);

    components::init_test();
    let _ = sink.run(events).await.unwrap();
    sleep(Duration::from_secs(1)).await;
    components::SINK_TESTS.assert(&["region"]);

    let timestamp = timestamp as f64 / 1000.0;
    let records = fetch_records(stream, timestamp, region).await.unwrap();

    let mut output_lines = records
        .into_iter()
        .map(|e| String::from_utf8(e.data.to_vec()).unwrap())
        .collect::<Vec<_>>();

    input_lines.sort();
    output_lines.sort();
    assert_eq!(output_lines, input_lines)
}

async fn fetch_records(
    stream_name: String,
    timestamp: f64,
    region: Region,
) -> crate::Result<Vec<rusoto_kinesis::Record>> {
    let client = Arc::new(KinesisClient::new(region));

    let req = rusoto_kinesis::DescribeStreamInput {
        stream_name: stream_name.clone(),
        ..Default::default()
    };
    let resp = client.describe_stream(req).await?;
    let shard = resp
        .stream_description
        .shards
        .into_iter()
        .next()
        .expect("No shards");

    let req = rusoto_kinesis::GetShardIteratorInput {
        stream_name,
        shard_id: shard.shard_id,
        shard_iterator_type: "AT_TIMESTAMP".into(),
        timestamp: Some(timestamp),
        ..Default::default()
    };
    let resp = client.get_shard_iterator(req).await?;
    let shard_iterator = resp.shard_iterator.expect("No iterator age produced");

    let req = rusoto_kinesis::GetRecordsInput {
        shard_iterator,
        // limit: Some(limit),
        limit: None,
    };
    let resp = client.get_records(req).await?;
    Ok(resp.records)
}

async fn ensure_stream(region: Region, stream_name: String) {
    let client = KinesisClient::new(region);

    let req = rusoto_kinesis::CreateStreamInput {
        stream_name,
        shard_count: 1,
    };

    match client.create_stream(req).await {
        Ok(_) => (),
        Err(error) => panic!("Unable to check the stream {:?}", error),
    };

    // Wait for localstack to persist stream, otherwise it returns ResourceNotFound errors
    // during PutRecords
    //
    // I initially tried using `wait_for` with `DescribeStream` but localstack would
    // successfully return the stream before it was able to accept PutRecords requests
    sleep(Duration::from_secs(1)).await;
}

fn gen_stream() -> String {
    format!("test-{}", random_string(10).to_lowercase())
}

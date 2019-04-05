use super::Record;
use crate::buffers::Acker;
use crate::sinks::util::{
    retries::{FixedRetryPolicy, RetryLogic},
    BatchServiceSink, SinkExt,
};
use futures::{Poll, Sink};
use rand::random;
use rusoto_core::{Region, RusotoFuture};
use rusoto_kinesis::{
    Kinesis, KinesisClient, PutRecordsError, PutRecordsInput, PutRecordsOutput,
    PutRecordsRequestEntry,
};
use serde::{Deserialize, Serialize};
use std::{fmt, sync::Arc, time::Duration};
use tower::{
    layer::{InFlightLimitLayer, RetryLayer, TimeoutLayer},
    Service, ServiceBuilder,
};

#[derive(Clone)]
pub struct KinesisService {
    client: Arc<KinesisClient>,
    config: KinesisSinkConfig,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct KinesisSinkConfig {
    pub stream_name: String,
    pub region: Region,
    pub batch_size: usize,
}

impl KinesisService {
    pub fn new(
        config: KinesisSinkConfig,
        acker: Acker,
    ) -> impl Sink<SinkItem = Record, SinkError = ()> {
        let client = Arc::new(KinesisClient::new(config.region.clone()));

        let batch_size = config.batch_size;
        let kinesis = KinesisService { client, config };

        let policy = FixedRetryPolicy::new(5, Duration::from_secs(1), KinesisRetryLogic);

        let svc = ServiceBuilder::new()
            .layer(InFlightLimitLayer::new(1))
            .layer(RetryLayer::new(policy))
            .layer(TimeoutLayer::new(Duration::from_secs(10)))
            .build_service(kinesis)
            .expect("This is a bug, no spawning done");

        BatchServiceSink::new(svc, acker)
            .batched(Vec::new(), batch_size)
            .with(|record: Record| Ok(record.into()))
    }

    fn gen_partition_key(&mut self) -> String {
        random::<[char; 16]>()
            .into_iter()
            .fold(String::new(), |mut s, c| {
                s.push(*c);
                s
            })
    }
}

impl Service<Vec<Vec<u8>>> for KinesisService {
    type Response = PutRecordsOutput;
    type Error = PutRecordsError;
    type Future = RusotoFuture<PutRecordsOutput, PutRecordsError>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, items: Vec<Vec<u8>>) -> Self::Future {
        let records = items
            .into_iter()
            .map(|data| PutRecordsRequestEntry {
                data,
                partition_key: self.gen_partition_key(),
                ..Default::default()
            })
            .collect();

        let request = PutRecordsInput {
            records,
            stream_name: self.config.stream_name.clone(),
        };

        self.client.put_records(request)
    }
}

impl fmt::Debug for KinesisService {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("KinesisService")
            .field("config", &self.config)
            .finish()
    }
}

#[derive(Debug, Clone)]
struct KinesisRetryLogic;

impl RetryLogic for KinesisRetryLogic {
    type Error = PutRecordsError;
    type Response = PutRecordsOutput;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            PutRecordsError::ProvisionedThroughputExceeded(_) => true,
            PutRecordsError::Unknown(res) if res.status.is_server_error() => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    #![cfg(feature = "kinesis-integration-tests")]

    use crate::buffers::Acker;
    use crate::sinks::kinesis::{KinesisService, KinesisSinkConfig};
    use crate::test_util::random_lines_with_stream;
    use futures::{Future, Sink};
    use rusoto_core::Region;
    use rusoto_kinesis::{Kinesis, KinesisClient};
    use std::sync::Arc;
    use tokio::runtime::Runtime;

    const STREAM_NAME: &'static str = "RouterTest";

    #[test]
    fn kinesis_put_records() {
        let region = Region::Custom {
            name: "localstack".into(),
            endpoint: "http://localhost:4568".into(),
        };

        ensure_stream(region.clone());

        std::thread::sleep(std::time::Duration::from_secs(1));

        let config = KinesisSinkConfig {
            stream_name: STREAM_NAME.into(),
            region: region.clone(),
            batch_size: 2,
        };

        let mut rt = Runtime::new().unwrap();

        let sink = KinesisService::new(config, Acker::Null);

        let timestamp = chrono::Utc::now().timestamp_millis();

        let (input_lines, records) = random_lines_with_stream(100, 11);

        let pump = sink.send_all(records);
        rt.block_on(pump).unwrap();

        std::thread::sleep(std::time::Duration::from_secs(1));

        let timestamp = timestamp as f64 / 1000.0;
        let records = rt
            .block_on(fetch_records(STREAM_NAME.into(), timestamp, region))
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
        region: Region,
    ) -> impl Future<Item = Vec<rusoto_kinesis::Record>, Error = ()> {
        let client = Arc::new(KinesisClient::new(region));

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

    fn ensure_stream(region: Region) {
        let client = KinesisClient::new(region);

        let req = rusoto_kinesis::CreateStreamInput {
            stream_name: STREAM_NAME.into(),
            shard_count: 1,
        };

        match client.create_stream(req).sync() {
            Ok(_) => (),
            Err(e) => println!("Unable to check the stream {:?}", e),
        };
    }

}

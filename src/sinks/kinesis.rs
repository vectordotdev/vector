use super::Record;
use crate::sinks::util::batch::{BatchSink, VecBatcher};
use futures::{Future, Poll, Sink};
use rand::random;
use rusoto_core::Region;
use rusoto_kinesis::{Kinesis, KinesisClient, PutRecordsInput, PutRecordsRequestEntry};
use serde::{Deserialize, Serialize};
use tower_service::Service;

pub struct KinesisService {
    client: KinesisClient,
    config: KinesisSinkConfig,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct KinesisSinkConfig {
    pub stream_name: String,
    pub region: String,
    pub batch_size: usize,
}

impl KinesisService {
    pub fn new(config: KinesisSinkConfig) -> impl Sink<SinkItem = Record, SinkError = ()> {
        let region = config.region.clone().parse::<Region>().unwrap();
        let client = KinesisClient::new(region);

        let batcher = VecBatcher::new(config.batch_size);
        let service = KinesisService { client, config };

        // TODO: construct service middleware here

        BatchSink::new(batcher, service)
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
    type Response = ();
    type Error = ();
    type Future = Box<Future<Item = Self::Response, Error = Self::Error> + Send + 'static>;

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

        let fut = self
            .client
            .put_records(request)
            .map(|_| ())
            .map_err(|e| panic!("Kinesis Error: {:?}", e));

        Box::new(fut)
    }
}

use aws_sdk_kinesis::types::{Blob, SdkError};
use bytes::Bytes;
use tracing::Instrument;

use super::{KinesisClient, KinesisError, KinesisRecord, Record, SendRecord};

#[derive(Clone)]
pub struct KinesisStreamRecord {
    pub record: KinesisRecord,
}

impl Record for KinesisStreamRecord {
    type T = KinesisRecord;

    fn new(payload_bytes: &Bytes, partition_key: &str) -> Self {
        Self {
            record: KinesisRecord::builder()
                .data(Blob::new(&payload_bytes[..]))
                .partition_key(partition_key)
                .build(),
        }
    }

    fn encoded_length(&self) -> usize {
        let hash_key_size = self
            .record
            .explicit_hash_key
            .as_ref()
            .map(|s| s.len())
            .unwrap_or_default();

        // data is base64 encoded
        let data_len = self
            .record
            .data
            .as_ref()
            .map(|data| data.as_ref().len())
            .unwrap_or(0);

        let key_len = self
            .record
            .partition_key
            .as_ref()
            .map(|key| key.len())
            .unwrap_or(0);

        (data_len + 2) / 3 * 4 + hash_key_size + key_len + 10
    }

    fn get(self) -> Self::T {
        self.record
    }
}

#[derive(Clone)]
pub struct KinesisStreamClient {
    pub client: KinesisClient,
}

#[async_trait::async_trait]
impl SendRecord for KinesisStreamClient {
    type T = KinesisRecord;
    type E = KinesisError;

    async fn send(&self, records: Vec<Self::T>, stream_name: String) -> Option<SdkError<Self::E>> {
        self.client
            .put_records()
            .set_records(Some(records))
            .stream_name(stream_name)
            .send()
            .instrument(info_span!("request").or_current())
            .await
            .err()
    }
}

use aws_sdk_firehose::output::PutRecordBatchOutput;
use aws_sdk_firehose::types::{Blob, SdkError};
use bytes::Bytes;
use tracing::Instrument;

use crate::sinks::prelude::*;

use super::{KinesisClient, KinesisError, KinesisRecord, KinesisResponse, Record, SendRecord};

#[derive(Clone)]
pub struct KinesisFirehoseRecord {
    pub record: KinesisRecord,
}

impl Record for KinesisFirehoseRecord {
    type T = KinesisRecord;

    fn new(payload_bytes: &Bytes, _partition_key: &str) -> Self {
        Self {
            record: KinesisRecord::builder()
                .data(Blob::new(&payload_bytes[..]))
                .build(),
        }
    }

    fn encoded_length(&self) -> usize {
        let data_len = self
            .record
            .data
            .as_ref()
            .map(|x| x.as_ref().len())
            .unwrap_or(0);
        // data is simply base64 encoded, quoted, and comma separated
        (data_len + 2) / 3 * 4 + 3
    }

    fn get(self) -> Self::T {
        self.record
    }
}

#[derive(Clone)]
pub struct KinesisFirehoseClient {
    pub client: KinesisClient,
}

#[async_trait::async_trait]
impl SendRecord for KinesisFirehoseClient {
    type T = KinesisRecord;
    type E = KinesisError;

    async fn send(
        &self,
        records: Vec<Self::T>,
        stream_name: String,
    ) -> Result<KinesisResponse, SdkError<Self::E>> {
        let rec_count = records.len();
        let total_size = records.iter().fold(0, |acc, record| {
            acc + record.data().map(|v| v.as_ref().len()).unwrap_or_default()
        });
        self.client
            .put_record_batch()
            .set_records(Some(records))
            .delivery_stream_name(stream_name)
            .send()
            .instrument(info_span!("request").or_current())
            .await
            .map(|output: PutRecordBatchOutput| KinesisResponse {
                count: rec_count,
                failure_count: output.failed_put_count().unwrap_or(0) as usize,
                events_byte_size: JsonSize::new(total_size),
            })
    }
}

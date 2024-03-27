use aws_sdk_firehose::operation::put_record_batch::PutRecordBatchOutput;
use aws_smithy_runtime_api::client::result::SdkError;
use aws_smithy_types::Blob;
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
                .build()
                .expect("all builder records specified"),
        }
    }

    fn encoded_length(&self) -> usize {
        let data_len = self.record.data.as_ref().len();
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

impl SendRecord for KinesisFirehoseClient {
    type T = KinesisRecord;
    type E = KinesisError;

    async fn send(
        &self,
        records: Vec<Self::T>,
        stream_name: String,
    ) -> Result<
        KinesisResponse,
        SdkError<Self::E, aws_smithy_runtime_api::client::orchestrator::HttpResponse>,
    > {
        let rec_count = records.len();
        let total_size = records
            .iter()
            .fold(0, |acc, record| acc + record.data().as_ref().len());
        self.client
            .put_record_batch()
            .set_records(Some(records))
            .delivery_stream_name(stream_name)
            .send()
            .instrument(info_span!("request").or_current())
            .await
            .map(|output: PutRecordBatchOutput| KinesisResponse {
                failure_count: output.failed_put_count() as usize,
                events_byte_size: CountByteSize(rec_count, JsonSize::new(total_size)).into(),
            })
    }
}

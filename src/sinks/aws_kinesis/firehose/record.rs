use super::{KinesisClient, KinesisError, KinesisRecord, Record, SendRecord};
use aws_sdk_firehose::error::PutRecordBatchError;
use aws_sdk_firehose::output::PutRecordBatchOutput;
use aws_sdk_firehose::types::{Blob, SdkError};
use bytes::Bytes;
#[cfg(not(test))]
use tokio::time::{sleep, Duration};
use tracing::Instrument;

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

    async fn send(&self, records: Vec<Self::T>, stream_name: String) -> Option<SdkError<Self::E>> {
        let mut r = self.inner_send(records.clone(), stream_name.clone()).await;

        for _ in 1..=3 {
            if let Ok(resp) = &r {
                if resp.failed_put_count().unwrap_or(0) > 0 {
                    #[cfg(not(test))] // the wait fails during test for some reason.
                    sleep(Duration::from_millis(100)).await;

                    let mut failed_records = vec![];
                    let itr = records
                        .clone()
                        .into_iter()
                        .zip(resp.request_responses().unwrap().into_iter());
                    for (rec, response) in itr {
                        // TODO can just filter
                        if response.error_code().is_some() {
                            failed_records.push(rec.clone());
                        }
                    }

                    r = self.inner_send(failed_records, stream_name.clone()).await;
                } else {
                    return r.err();
                }
            }
        }

        return r.err();
    }
}

impl KinesisFirehoseClient {
    async fn inner_send(
        &self,
        records: Vec<KinesisRecord>,
        stream_name: String,
    ) -> Result<PutRecordBatchOutput, SdkError<PutRecordBatchError>> {
        self.client
            .put_record_batch()
            .set_records(Some(records))
            .delivery_stream_name(stream_name)
            .send()
            .instrument(info_span!("request").or_current())
            .await
    }
}

use aws_sdk_kinesis::operation::put_records::PutRecordsOutput;
use aws_smithy_runtime_api::client::{orchestrator::HttpResponse, result::SdkError};
use aws_smithy_types::Blob;
use bytes::Bytes;
use tracing::Instrument;

use super::{
    super::service::RecordResult, KinesisClient, KinesisError, KinesisRecord, KinesisResponse,
    Record, SendRecord,
};
use crate::sinks::prelude::*;

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
                .build()
                .expect("all required builder fields set"),
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
        let data_len = self.record.data.as_ref().len();
        let key_len = self.record.partition_key.len();

        data_len.div_ceil(3) * 4 + hash_key_size + key_len + 10
    }

    fn get(self) -> Self::T {
        self.record
    }
}

#[derive(Clone)]
pub struct KinesisStreamClient {
    pub client: KinesisClient,
}

impl SendRecord for KinesisStreamClient {
    type T = KinesisRecord;
    type E = KinesisError;

    async fn send(
        &self,
        records: Vec<Self::T>,
        stream_name: String,
    ) -> Result<KinesisResponse, SdkError<Self::E, HttpResponse>> {
        let rec_count = records.len();
        let total_size = records
            .iter()
            .fold(0, |acc, record| acc + record.data().as_ref().len());

        self.client
            .put_records()
            .set_records(Some(records))
            .stream_name(stream_name)
            .send()
            .instrument(info_span!("request").or_current())
            .await
            .map(|output: PutRecordsOutput| KinesisResponse {
                failed_records: extract_failed_records(&output),
                failure_count: output.failed_record_count().unwrap_or(0) as usize,
                events_byte_size: CountByteSize(rec_count, JsonSize::new(total_size)).into(),
            })
    }
}

fn extract_failed_records(output: &PutRecordsOutput) -> Vec<RecordResult> {
    output
        .records()
        .iter()
        .enumerate()
        .filter_map(|(index, record)| {
            record.error_code().map(|error_code| RecordResult {
                index,
                success: false,
                error_code: Some(error_code.to_string()),
                error_message: record.error_message().map(String::from),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_kinesis::{operation::put_records::PutRecordsOutput, types::PutRecordsResultEntry};

    #[test]
    fn test_extract_failed_records_all_success() {
        // Create mock successful records
        let record1 = PutRecordsResultEntry::builder()
            .sequence_number("seq1")
            .shard_id("shard1")
            .build();

        let record2 = PutRecordsResultEntry::builder()
            .sequence_number("seq2")
            .shard_id("shard2")
            .build();

        let output = PutRecordsOutput::builder()
            .records(record1)
            .records(record2)
            .failed_record_count(0)
            .build()
            .unwrap();

        let results = extract_failed_records(&output);

        // Should return empty since no failures
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_extract_failed_records_mixed_success_failure() {
        // Create mock records with mixed success/failure
        let success_record = PutRecordsResultEntry::builder()
            .sequence_number("seq1")
            .shard_id("shard1")
            .build();

        let failure_record = PutRecordsResultEntry::builder()
            .error_code("ProvisionedThroughputExceededException")
            .error_message("Rate exceeded for shard")
            .build();

        let output = PutRecordsOutput::builder()
            .records(success_record)
            .records(failure_record)
            .failed_record_count(1)
            .build()
            .unwrap();

        let results = extract_failed_records(&output);

        // Should only return the failed record
        assert_eq!(results.len(), 1);

        // Only record should be the failed one (originally at index 1)
        assert!(!results[0].success);
        assert_eq!(
            results[0].error_code.as_ref().unwrap(),
            "ProvisionedThroughputExceededException"
        );
        assert_eq!(
            results[0].error_message.as_ref().unwrap(),
            "Rate exceeded for shard"
        );
        assert_eq!(results[0].index, 1);
    }

    #[test]
    fn test_extract_failed_records_all_failures() {
        // Create mock failed records
        let failure_record1 = PutRecordsResultEntry::builder()
            .error_code("ProvisionedThroughputExceededException")
            .error_message("Rate exceeded")
            .build();

        let failure_record2 = PutRecordsResultEntry::builder()
            .error_code("InternalFailure")
            .error_message("Internal server error")
            .build();

        let output = PutRecordsOutput::builder()
            .records(failure_record1)
            .records(failure_record2)
            .failed_record_count(2)
            .build()
            .unwrap();

        let results = extract_failed_records(&output);

        assert_eq!(results.len(), 2);
        assert!(!results[0].success);
        assert!(!results[1].success);
        assert_eq!(
            results[0].error_code.as_ref().unwrap(),
            "ProvisionedThroughputExceededException"
        );
        assert_eq!(results[1].error_code.as_ref().unwrap(), "InternalFailure");
    }
}

use chrono::{serde::ts_milliseconds, DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Represents an AWS Kinesis Firehose request
///
/// Represents protocol v1.0 (the only protocol as of writing)
///
/// <https://docs.aws.amazon.com/firehose/latest/dev/httpdeliveryrequestresponse.html>
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FirehoseRequest {
    pub access_key: Option<String>,
    pub request_id: String,

    #[serde(with = "ts_milliseconds")]
    pub timestamp: DateTime<Utc>,

    pub records: Vec<EncodedFirehoseRecord>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EncodedFirehoseRecord {
    /// data is base64 encoded, gzip'd, bytes
    pub data: String,
}

/// Represents an AWS Kinesis Firehose response
///
/// Represents protocol v1.0 (the only protocol as of writing)
///
/// <https://docs.aws.amazon.com/firehose/latest/dev/httpdeliveryrequestresponse.html>
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FirehoseResponse {
    pub request_id: String,

    #[serde(with = "ts_milliseconds")]
    pub timestamp: DateTime<Utc>,

    pub error_message: Option<String>,
}

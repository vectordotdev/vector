use chrono::{serde::ts_milliseconds, DateTime, Utc};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum AwsCloudWatchLogsSubscriptionMessageType {
    ControlMessage,
    DataMessage,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AwsCloudWatchLogEvent {
    pub id: String,
    #[serde(with = "ts_milliseconds")]
    pub timestamp: DateTime<Utc>,
    pub message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AwsCloudWatchLogsSubscriptionMessage {
    pub owner: String,
    pub message_type: AwsCloudWatchLogsSubscriptionMessageType,
    pub log_group: String,
    pub log_stream: String,
    pub subscription_filters: Vec<String>,
    pub log_events: Vec<AwsCloudWatchLogEvent>,
}

impl AwsCloudWatchLogsSubscriptionMessageType {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            AwsCloudWatchLogsSubscriptionMessageType::ControlMessage => "CONTROL_MESSAGE",
            AwsCloudWatchLogsSubscriptionMessageType::DataMessage => "DATA_MESSAGE",
        }
    }
}

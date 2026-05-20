mod client;
mod config;

#[cfg(all(test, feature = "aws-sqs-integration-tests"))]
mod integration_tests;

use super::{
    client::Client,
    config::{BaseSSSinkConfig, message_deduplication_id, message_group_id},
    request_builder::{SSRequestBuilder, SendMessageEntry},
    service::SendMessageResponse,
    sink::SSSink,
};

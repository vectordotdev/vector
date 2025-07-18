mod client;
mod config;
#[cfg(all(test, feature = "aws-sns-integration-tests"))]
mod integration_tests;

use super::{
    client::Client,
    config::{message_deduplication_id, message_group_id, BaseSSSinkConfig},
    request_builder::{SSRequestBuilder, SendMessageEntry},
    service::SendMessageResponse,
    sink::SSSink,
};

mod config;
mod integration_tests;

pub use super::{
    client::Client,
    config::{message_deduplication_id, message_group_id, BaseSSSinkConfig},
    request_builder::{SSRequestBuilder, SendMessageEntry},
    service::SendMessageResponse,
    sink::SSSink,
};

mod client;

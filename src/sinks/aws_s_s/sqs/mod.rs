mod config;
mod integration_tests;

pub use super::{
    client::Client,
    config::{BaseSSSinkConfig, MessageIdConfig},
    request_builder::{SSRequestBuilder, SendMessageEntry},
    service::SendMessageResponse,
    sink::SSSink,
};

mod client;

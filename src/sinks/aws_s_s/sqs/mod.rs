mod config;
mod integration_tests;
mod request_builder;

pub use super::{
    client::Client,
    config::{BaseSSSinkConfig, ConfigWithIds},
    request_builder::{MessageBuilder, SendMessageEntry},
    service::SendMessageResponse,
    sink::SqsSink,
};

mod client;

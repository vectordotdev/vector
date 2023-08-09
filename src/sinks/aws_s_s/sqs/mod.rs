mod config;
mod integration_tests;

pub use super::{
    client::Client,
    config::{BaseSSSinkConfig, ConfigWithIds},
    request_builder::SendMessageEntry,
    service::SendMessageResponse,
    sink::SqsSink,
};

mod client;

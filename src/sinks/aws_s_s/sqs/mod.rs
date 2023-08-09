mod config;
mod integration_tests;
mod request_builder;

pub use super::{
    config::{BaseSSSinkConfig, ConfigWithIds},
    request_builder::{MessageBuilder, SendMessageEntry},
    sink::SqsSink,
};

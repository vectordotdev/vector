mod config;
mod sink;

use crate::{
    config::{DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    event::Event,
    internal_events::{ConsoleEventProcessed, ConsoleFieldNotFound},
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfiguration},
        StreamSink,
    },
};
use async_trait::async_trait;
use futures::{
    future,
    stream::{BoxStream, StreamExt},
    FutureExt,
};
use serde::{Deserialize, Serialize};

use crate::sinks::util::encoding::StandardEncodings;
use config::ConsoleSinkConfig;
use tokio::io::{self, AsyncWriteExt};
use vector_core::buffers::Acker;

inventory::submit! {
    SinkDescription::new::<ConsoleSinkConfig>("console")
}

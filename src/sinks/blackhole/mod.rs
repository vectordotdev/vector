mod config;
mod sink;

use crate::sinks::blackhole::config::BlackholeConfig;
use crate::{
    config::{DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    internal_events::BlackholeEventReceived,
    sinks::util::StreamSink,
};
use async_trait::async_trait;
use futures::{future, stream::BoxStream, FutureExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{
    select,
    sync::watch,
    time::{interval, sleep_until},
};
use vector_core::ByteSizeOf;
use vector_core::{buffers::Acker, event::Event};

inventory::submit! {
    SinkDescription::new::<BlackholeConfig>("blackhole")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinks::blackhole::config::BlackholeConfig;
    use crate::sinks::blackhole::sink::BlackholeSink;
    use crate::test_util::random_events_with_stream;

    #[tokio::test]
    async fn blackhole() {
        let config = BlackholeConfig {
            print_interval_secs: 10,
            rate: None,
        };
        let sink = Box::new(BlackholeSink::new(config, Acker::Null));

        let (_input_lines, events) = random_events_with_stream(100, 10, None);
        let _ = sink.run(Box::pin(events)).await.unwrap();
    }
}

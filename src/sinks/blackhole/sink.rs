use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use async_trait::async_trait;
use futures::{stream::BoxStream, StreamExt};
use tokio::{
    select,
    sync::watch,
    time::{interval, sleep_until},
};
use vector_common::{
    config::ComponentKey,
    internal_event::{
        ByteSize, BytesSent, CountByteSize, EventsSent, InternalEventHandle as _, Output, Protocol,
        Source,
    },
};
use vector_core::EstimatedJsonEncodedSizeOf;

use crate::{
    event::{EventArray, EventContainer},
    sinks::{blackhole::config::BlackholeConfig, util::StreamSink},
};

pub struct BlackholeSink {
    total_events: Arc<AtomicUsize>,
    total_raw_bytes: Arc<AtomicUsize>,
    config: BlackholeConfig,
    last: Option<Instant>,
    source_keys: Vec<ComponentKey>,
}

impl BlackholeSink {
    pub fn new(config: BlackholeConfig, source_keys: Vec<ComponentKey>) -> Self {
        BlackholeSink {
            config,
            total_events: Arc::new(AtomicUsize::new(0)),
            total_raw_bytes: Arc::new(AtomicUsize::new(0)),
            last: None,
            source_keys,
        }
    }
}

#[async_trait]
impl StreamSink<EventArray> for BlackholeSink {
    async fn run(mut self: Box<Self>, mut input: BoxStream<'_, EventArray>) -> Result<(), ()> {
        // Spin up a task that does the periodic reporting.  This is decoupled from the main sink so
        // that rate limiting support can be added more simply without having to interleave it with
        // the printing.
        let total_events = Arc::clone(&self.total_events);
        let total_raw_bytes = Arc::clone(&self.total_raw_bytes);
        let (shutdown, mut tripwire) = watch::channel(());

        let events_sent = EventsSent::sources_matrix(self.source_keys, None);
        let bytes_sent = register!(BytesSent::from(Protocol("blackhole".into())));

        if self.config.print_interval_secs.as_secs() > 0 {
            let interval_dur = self.config.print_interval_secs;
            tokio::spawn(async move {
                let mut print_interval = interval(interval_dur);
                loop {
                    select! {
                        _ = print_interval.tick() => {
                            info!(
                                events = total_events.load(Ordering::Relaxed),
                                raw_bytes_collected = total_raw_bytes.load(Ordering::Relaxed),
                                "Collected events."
                            );
                        },
                        _ = tripwire.changed() => break,
                    }
                }

                info!(
                    events = total_events.load(Ordering::Relaxed),
                    raw_bytes_collected = total_raw_bytes.load(Ordering::Relaxed),
                    "Collected events."
                );
            });
        }

        while let Some(events) = input.next().await {
            if let Some(rate) = self.config.rate {
                let factor: f32 = 1.0 / rate as f32;
                let secs: f32 = factor * (events.len() as f32);
                let until = self.last.unwrap_or_else(Instant::now) + Duration::from_secs_f32(secs);
                sleep_until(until.into()).await;
                self.last = Some(until);
            }

            let _ = self.total_events.fetch_add(events.len(), Ordering::AcqRel);

            for event in events.iter_events() {
                let bytes = event.estimated_json_encoded_size_of();
                let _ = self.total_raw_bytes.fetch_add(bytes, Ordering::AcqRel);
                bytes_sent.emit(ByteSize(bytes));

                if let Some(handle) = events_sent.get(&event.metadata().source_id()) {
                    handle.emit(CountByteSize(1, bytes));
                }
            }
        }

        // Notify the reporting task to shutdown.
        let _ = shutdown.send(());

        Ok(())
    }
}

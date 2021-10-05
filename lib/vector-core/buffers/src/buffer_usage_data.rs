use core_common::internal_event::emit;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{sync::Arc, time::Duration};
use tokio::time::interval;
use tracing::{Instrument, Span};

use crate::internal_events::{BufferEventsReceived, BufferEventsSent, EventsDropped};

pub struct BufferUsageData {
    received_event_count: AtomicUsize,
    received_event_byte_size: AtomicUsize,
    sent_event_count: AtomicUsize,
    sent_event_byte_size: AtomicUsize,
    dropped_event_count: Option<AtomicUsize>,
}

impl BufferUsageData {
    pub fn new(dropped_event_count: Option<AtomicUsize>, span: Span) -> Arc<Self> {
        let buffer_usage_data = Arc::new(Self {
            received_event_count: AtomicUsize::new(0),
            received_event_byte_size: AtomicUsize::new(0),
            sent_event_count: AtomicUsize::new(0),
            sent_event_byte_size: AtomicUsize::new(0),
            dropped_event_count,
        });

        let usage_data = buffer_usage_data.clone();
        tokio::spawn(
            async move {
                let mut interval = interval(Duration::from_secs(2));
                loop {
                    interval.tick().await;

                    emit(&BufferEventsReceived {
                        count: usage_data.received_event_count.load(Ordering::Relaxed),
                        byte_size: usage_data.received_event_byte_size.load(Ordering::Relaxed),
                    });

                    emit(&BufferEventsSent {
                        count: usage_data.sent_event_count.load(Ordering::Relaxed),
                        byte_size: usage_data.sent_event_byte_size.load(Ordering::Relaxed),
                    });

                    if let Some(dropped_event_count) = &usage_data.dropped_event_count {
                        emit(&EventsDropped {
                            count: dropped_event_count.load(Ordering::Relaxed),
                        });
                    }
                }
            }
            .instrument(span),
        );

        buffer_usage_data
    }

    pub fn increment_received_event_count(&self, count: usize) {
        self.received_event_count
            .fetch_add(count, Ordering::Relaxed);
    }

    pub fn increment_received_event_byte_size(&self, byte_size: usize) {
        self.received_event_byte_size
            .fetch_add(byte_size, Ordering::Relaxed);
    }

    pub fn increment_sent_event_count(&self, count: usize) {
        self.sent_event_count.fetch_add(count, Ordering::Relaxed);
    }

    pub fn increment_sent_event_byte_size(&self, byte_size: usize) {
        self.sent_event_byte_size
            .fetch_add(byte_size, Ordering::Relaxed);
    }

    pub fn try_increment_dropped_event_count(&self, count: usize) {
        if let Some(dropped_event_count) = &self.dropped_event_count {
            dropped_event_count.fetch_add(count, Ordering::Relaxed);
        }
    }
}

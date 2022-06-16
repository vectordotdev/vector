use std::{
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use tokio::time::interval;
use tracing::{Instrument, Span};
use vector_common::internal_event::emit;

use crate::{
    internal_events::{BufferCreated, BufferEventsDropped, BufferEventsReceived, BufferEventsSent},
    spawn_named,
};

#[derive(Clone, Debug)]
pub struct BufferUsageHandle {
    state: Arc<BufferUsageData>,
}

impl BufferUsageHandle {
    /// Creates a no-op [`BufferUsageHandle`] handle.
    ///
    /// No usage data is written or stored.
    pub(crate) fn noop() -> Self {
        BufferUsageHandle {
            state: Arc::new(BufferUsageData::new(0)),
        }
    }

    /// Gets a snapshot of the buffer usage data, representing an instantaneous view of the
    /// different values.
    pub fn snapshot(&self) -> BufferUsageSnapshot {
        self.state.snapshot()
    }

    /// Sets the limits for this buffer component.
    ///
    /// Limits are exposed as gauges to provide stable values when superimposed on dashboards/graphs
    /// with the "actual" usage amounts.
    pub fn set_buffer_limits(&self, max_bytes: Option<u64>, max_events: Option<usize>) {
        if let Some(max_bytes) = max_bytes {
            self.state
                .max_size_bytes
                .store(max_bytes, Ordering::Relaxed);
        }

        if let Some(max_events) = max_events {
            self.state
                .max_size_events
                .store(max_events, Ordering::Relaxed);
        }
    }

    /// Increments the number of events (and their total size) received by this buffer component.
    ///
    /// This represents the events being sent into the buffer.
    pub fn increment_received_event_count_and_byte_size(&self, count: u64, byte_size: u64) {
        self.state
            .received_event_count
            .fetch_add(count, Ordering::Relaxed);
        self.state
            .received_byte_size
            .fetch_add(byte_size, Ordering::Relaxed);
    }

    /// Increments the number of events (and their total size) sent by this buffer component.
    ///
    /// This represents the events being read out of the buffer.
    pub fn increment_sent_event_count_and_byte_size(&self, count: u64, byte_size: u64) {
        self.state
            .sent_event_count
            .fetch_add(count, Ordering::Relaxed);
        self.state
            .sent_byte_size
            .fetch_add(byte_size, Ordering::Relaxed);
    }

    /// Increment the number of dropped events (and their total size) for this buffer component.
    pub fn increment_dropped_event_count_and_byte_size(
        &self,
        count: u64,
        byte_size: u64,
        intentional: bool,
    ) {
        if intentional {
            self.state
                .dropped_event_count_intentional
                .fetch_add(count, Ordering::Relaxed);
            self.state
                .dropped_event_byte_size_intentional
                .fetch_add(byte_size, Ordering::Relaxed);
        } else {
            self.state
                .dropped_event_count
                .fetch_add(count, Ordering::Relaxed);
            self.state
                .dropped_event_byte_size
                .fetch_add(byte_size, Ordering::Relaxed);
        }
    }
}

#[derive(Debug)]
pub struct BufferUsageData {
    idx: usize,
    received_event_count: AtomicU64,
    received_byte_size: AtomicU64,
    sent_event_count: AtomicU64,
    sent_byte_size: AtomicU64,
    dropped_event_count: AtomicU64,
    dropped_event_byte_size: AtomicU64,
    dropped_event_count_intentional: AtomicU64,
    dropped_event_byte_size_intentional: AtomicU64,
    max_size_bytes: AtomicU64,
    max_size_events: AtomicUsize,
}

impl BufferUsageData {
    pub fn new(idx: usize) -> Self {
        Self {
            idx,
            received_event_count: AtomicU64::new(0),
            received_byte_size: AtomicU64::new(0),
            sent_event_count: AtomicU64::new(0),
            sent_byte_size: AtomicU64::new(0),
            dropped_event_count: AtomicU64::new(0),
            dropped_event_byte_size: AtomicU64::new(0),
            dropped_event_count_intentional: AtomicU64::new(0),
            dropped_event_byte_size_intentional: AtomicU64::new(0),
            max_size_bytes: AtomicU64::new(0),
            max_size_events: AtomicUsize::new(0),
        }
    }

    fn snapshot(&self) -> BufferUsageSnapshot {
        BufferUsageSnapshot {
            received_event_count: self.received_event_count.load(Ordering::Relaxed),
            received_byte_size: self.received_byte_size.load(Ordering::Relaxed),
            sent_event_count: self.sent_event_count.load(Ordering::Relaxed),
            sent_byte_size: self.sent_byte_size.load(Ordering::Relaxed),
            dropped_event_count: self.dropped_event_count.load(Ordering::Relaxed),
            dropped_event_byte_size: self.dropped_event_byte_size.load(Ordering::Relaxed),
            dropped_event_count_intentional: self
                .dropped_event_count_intentional
                .load(Ordering::Relaxed),
            dropped_event_byte_size_intentional: self
                .dropped_event_byte_size_intentional
                .load(Ordering::Relaxed),
            max_size_bytes: self.max_size_bytes.load(Ordering::Relaxed),
            max_size_events: self.max_size_events.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug)]
pub struct BufferUsageSnapshot {
    pub received_event_count: u64,
    pub received_byte_size: u64,
    pub sent_event_count: u64,
    pub sent_byte_size: u64,
    pub dropped_event_count: u64,
    pub dropped_event_byte_size: u64,
    pub dropped_event_count_intentional: u64,
    pub dropped_event_byte_size_intentional: u64,
    pub max_size_bytes: u64,
    pub max_size_events: usize,
}

pub struct BufferUsage {
    span: Span,
    stages: Vec<Arc<BufferUsageData>>,
}

impl BufferUsage {
    /// Creates an instance of [`BufferUsage`] attached to the given span.
    ///
    /// As buffers can have multiple stages, callers have the ability to register each stage via [`add_stage`]
    pub fn from_span(span: Span) -> BufferUsage {
        Self {
            span,
            stages: Vec::new(),
        }
    }

    /// Adds a new stage to track usage for.
    ///
    /// A [`BufferUsageHandle`] is returned that the caller can use to actually update the usage
    /// metrics with.  This handle will only update the usage metrics for the particular stage it
    /// was added for.
    pub fn add_stage(&mut self, idx: usize) -> BufferUsageHandle {
        let data = Arc::new(BufferUsageData::new(idx));
        let handle = BufferUsageHandle {
            state: Arc::clone(&data),
        };

        self.stages.push(data);
        handle
    }

    pub fn install(self, buffer_id: &str) {
        let span = self.span;
        let stages = self.stages;

        let task = async move {
            let mut interval = interval(Duration::from_secs(2));
            loop {
                interval.tick().await;

                for stage in &stages {
                    let max_size_bytes = match stage.max_size_bytes.load(Ordering::Relaxed) {
                        0 => None,
                        n => Some(n),
                    };

                    let max_size_events = match stage.max_size_events.load(Ordering::Relaxed) {
                        0 => None,
                        n => Some(n),
                    };

                    emit(BufferCreated {
                        idx: stage.idx,
                        max_size_bytes,
                        max_size_events,
                    });

                    emit(BufferEventsReceived {
                        idx: stage.idx,
                        count: stage.received_event_count.swap(0, Ordering::Relaxed),
                        byte_size: stage.received_byte_size.swap(0, Ordering::Relaxed),
                    });

                    emit(BufferEventsSent {
                        idx: stage.idx,
                        count: stage.sent_event_count.swap(0, Ordering::Relaxed),
                        byte_size: stage.sent_byte_size.swap(0, Ordering::Relaxed),
                    });

                    emit(BufferEventsDropped {
                        idx: stage.idx,
                        intentional: true,
                        reason: "drop_newest",
                        count: stage
                            .dropped_event_count_intentional
                            .swap(0, Ordering::Relaxed),
                        byte_size: stage
                            .dropped_event_byte_size_intentional
                            .swap(0, Ordering::Relaxed),
                    });

                    emit(BufferEventsDropped {
                        idx: stage.idx,
                        intentional: false,
                        reason: "corrupted_events",
                        count: stage.dropped_event_count.swap(0, Ordering::Relaxed),
                        byte_size: stage.dropped_event_byte_size.swap(0, Ordering::Relaxed),
                    });
                }
            }
        };

        let task_name = format!("buffer usage reporter ({})", buffer_id);
        spawn_named(task.instrument(span.or_current()), task_name.as_str());
    }
}

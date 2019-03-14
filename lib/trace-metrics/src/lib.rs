extern crate tokio_trace_core;
extern crate hotmic;

use hotmic::Sink;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};
use tokio_trace_core::{
    span::{Attributes, Id, Record},
    Event, Metadata, Subscriber,
};

/// Metrics collector
pub type Collector = Sink<String>;

pub struct MetricsSubscriber {
    spans: Mutex<HashMap<Id, Span>>,
    collector: Collector,
    next_id: AtomicUsize,
}

struct Span {
    follows: Vec<Id>,
}

impl MetricsSubscriber {
    pub fn new(collector: Collector) -> Self {
        MetricsSubscriber {
            collector,
            spans: Mutex::new(HashMap::new()),
            next_id: AtomicUsize::new(1),
        }
    }
}

impl Subscriber for MetricsSubscriber {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn new_span(&self, _span: &Attributes) -> Id {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let id = Id::from_u64(id as u64);
        self.spans
            .lock()
            .unwrap()
            .insert(id.clone(), Span::new());
        id
    }

    fn record(&self, span: &Id, values: &Record) {
        // TODO: write this
    }

    fn record_follows_from(&self, span_id: &Id, follows: &Id) {
        let mut spans = self.spans.lock().unwrap();
        if let Some(span) = &mut spans.get_mut(span_id) {
            span.follows.push(follows.clone());
        }
    }

    fn event(&self, event: &Event) {
        // TODO: write this
    }

    fn enter(&self, span: &Id) {
        // TODO: write this
    }

    fn exit(&self, span: &Id) {
        // TODO: write this
    }
}

impl Span {
    pub fn new() -> Self {
        Span {
            follows: Vec::new()
        }
    }
}

extern crate hotmic;
extern crate tokio_trace_core;

use hotmic::Sink;
use std::{collections::HashMap, sync::Mutex};
use tokio_trace_core::{
    span::{Attributes, Id, Record},
    subscriber::Interest,
    Event, Metadata, Subscriber,
};

/// Metrics collector
pub type Collector = Sink<String>;

pub struct MetricsSubscriber<S> {
    inner: S,
    spans: Mutex<HashMap<Id, Span>>,
    collector: Collector,
}

#[derive(Debug, Default)]
struct Span {
    key: String,
    start: Option<u64>,
}

impl<S> MetricsSubscriber<S> {
    pub fn new(inner: S, collector: Collector) -> Self {
        MetricsSubscriber {
            inner,
            collector,
            spans: Mutex::new(HashMap::new()),
        }
    }
}

impl<S> Subscriber for MetricsSubscriber<S>
where
    S: Subscriber,
{
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn new_span(&self, span: &Attributes) -> Id {
        let metadata = span.metadata();

        let id = self.inner.new_span(span);

        let span = Span {
            key: metadata.name().into(),
            ..Default::default()
        };

        self.spans.lock().unwrap().insert(id.clone(), span);
        id
    }

    fn record(&self, span: &Id, values: &Record) {
        self.inner.record(span, values);
    }

    fn record_follows_from(&self, span: &Id, follows: &Id) {
        self.inner.record_follows_from(span, follows);
    }

    fn event(&self, event: &Event) {
        self.inner.event(event);
    }

    fn enter(&self, span: &Id) {
        self.inner.enter(span);

        let mut spans = self.spans.lock().unwrap();
        if let Some(span) = &mut spans.get_mut(span) {
            span.start = Some(self.collector.clock().start());
        }
    }

    fn exit(&self, span: &Id) {
        self.inner.exit(span);

        let mut spans = self.spans.lock().unwrap();
        if let Some(span) = &mut spans.get_mut(span) {
            if let Some(start) = span.start {
                let end = self.collector.clock().end();
                self.collector.update_timing(span.key.clone(), start, end);
            }
        }
    }

    // extra non required fn
    fn register_callsite(&self, metadata: &Metadata) -> Interest {
        self.inner.register_callsite(metadata)
    }

    fn clone_span(&self, id: &Id) -> Id {
        self.inner.clone_span(id)
    }

    fn drop_span(&self, id: Id) {
        self.inner.drop_span(id);
    }
}

extern crate hotmic;
extern crate tokio_trace_core;

use hotmic::Sink;
use std::{collections::HashMap, sync::Mutex};
use tokio_trace_core::{
    span::{Attributes, Id, Record},
    Event, Interest, Metadata, Subscriber,
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
    start_duration: Option<u64>,
    start_execution: Option<u64>,
    end_duration: Option<u64>,
    ref_count: usize,
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

impl<S: Subscriber> Subscriber for MetricsSubscriber<S> {
    fn enabled(&self, metadata: &Metadata) -> bool {
        if metadata.name().contains("event") {
            self.inner.enabled(metadata)
        } else {
            true
        }
    }

    fn new_span(&self, span: &Attributes) -> Id {
        let metadata = span.metadata();

        let id = self.inner.new_span(span);
        let key = metadata.name().to_string();

        let span = Span {
            key,
            ref_count: 1,
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
            let start = self.collector.clock().start();
            span.start_execution = Some(start);

            if let None = span.start_duration {
                span.start_duration = Some(start);
            }
        }
    }

    fn exit(&self, span: &Id) {
        self.inner.exit(span);

        let mut spans = self.spans.lock().unwrap();
        if let Some(span) = &mut spans.get_mut(span) {
            let end = self.collector.clock().end();

            if let Some(start) = span.start_execution {
                self.collector
                    .update_timing(span.key.clone() + "_execution", start, end);
            }

            if let Some(_start) = span.start_duration {
                span.end_duration = Some(end);
            }
        }
    }

    // extra non required fn
    fn register_callsite(&self, metadata: &Metadata) -> Interest {
        if metadata.name().contains("event") {
            self.inner.register_callsite(metadata)
        } else {
            Interest::always()
        }
    }

    fn clone_span(&self, id: &Id) -> Id {
        // TODO: track span id changes???
        let id = self.inner.clone_span(id);

        let mut spans = self.spans.lock().unwrap();
        if let Some(span) = &mut spans.get_mut(&id) {
            span.ref_count += 1;
        }

        id
    }

    fn drop_span(&self, id: Id) {
        let mut spans = self.spans.lock().unwrap();

        if let Some(span) = &mut spans.get_mut(&id) {
            span.ref_count -= 1;

            if span.ref_count == 0 {
                if let Some(start) = span.start_duration {
                    if let Some(end) = span.end_duration {
                        self.collector
                            .update_timing(span.key.clone() + "_duration", start, end);
                    }
                }
            }
        }

        drop(spans);
        self.inner.drop_span(id.clone());
    }
}

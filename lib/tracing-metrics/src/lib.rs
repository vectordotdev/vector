//! A `tracing` metrics based subscriber
//!
//! This subscriber takes another subscriber like `tracing-fmt` and wraps it
//! with this basic subscriber. It will enable all spans and events that match the
//! metric capturing criteria. This means every span is enabled regardless of its level
//! and any event with a field name ending with `_counter` or `_gauge`.

use hotmic::Sink;
use std::{
    collections::{HashMap, HashSet},
    fmt,
    sync::{Mutex, RwLock},
};
use tracing_core::{
    field::{Field, Visit},
    span::{Attributes, Id, Record},
    Event, Interest, Metadata, Subscriber,
};

/// Metrics collector
// TODO(lucio): move this to a trait
pub type Collector = Sink<&'static str>;

/// The subscriber that wraps another subscriber and produces metrics
pub struct MetricsSubscriber<S> {
    inner: S,
    spans: Mutex<HashMap<Id, Span>>,
    interest: RwLock<HashSet<&'static str>>,
    collector: Collector,
}

/// A `tracing_core::field::Visit` implementation that captures fields
/// that contain `counter` or `gague` in their name and dispatches the `i64`
/// or `u64` value to the underlying metrics sink.
pub struct MetricVisitor {
    collector: Collector,
}

#[derive(Debug, Default)]
struct Span {
    key: &'static str,
    start_duration: Option<u64>,
    start_execution: Option<u64>,
    end_duration: Option<u64>,
    ref_count: usize,
}

impl<S> MetricsSubscriber<S> {
    /// Create a new `MetricsSubscriber` with the underlying subscriber and collector.
    pub fn new(inner: S, collector: Collector) -> Self {
        MetricsSubscriber {
            inner,
            collector,
            interest: RwLock::new(HashSet::new()),
            spans: Mutex::new(HashMap::new()),
        }
    }
}

impl<S: Subscriber> Subscriber for MetricsSubscriber<S> {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        self.inner.enabled(metadata)
    }

    fn new_span(&self, span: &Attributes<'_>) -> Id {
        let metadata = span.metadata();

        let id = self.inner.new_span(span);
        let key = metadata.name();

        let span = Span {
            key,
            ref_count: 1,
            ..Default::default()
        };

        self.spans.lock().unwrap().insert(id.clone(), span);
        id
    }

    fn record(&self, span: &Id, values: &Record<'_>) {
        self.inner.record(span, values);
    }

    fn record_follows_from(&self, span: &Id, follows: &Id) {
        self.inner.record_follows_from(span, follows);
    }

    fn event(&self, event: &Event<'_>) {
        let mut recorder = MetricVisitor::new(self.collector.clone());
        event.record(&mut recorder);

        let selective_interest = {
            self.interest
                .read()
                .unwrap()
                .contains(event.metadata().name())
        };

        // we marked this as only the metrics sub being interested in it
        if !selective_interest {
            self.inner.event(event);
        }
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

            // TODO: bring this back when we can do it without an allocation
            // if let Some(start) = span.start_execution {
            // self.collector
            // .update_timing(span.key.clone() + "_execution", start, end);
            // }

            if let Some(_start) = span.start_duration {
                span.end_duration = Some(end);
            }
        }
    }

    // extra non required fn
    fn register_callsite(&self, metadata: &'static Metadata<'static>) -> Interest {
        if metadata.name().contains("event")
            && metadata
                .fields()
                .iter()
                .any(|f| f.name().ends_with("_counter") || f.name().ends_with("_gauge"))
            && !metadata
                .fields()
                .iter()
                .any(|f| f.name().contains("message"))
        {
            self.inner.register_callsite(metadata);
            self.interest.write().unwrap().insert(metadata.name());
            Interest::always()
        } else if metadata.name().contains("event") {
            self.inner.register_callsite(metadata)
        } else {
            self.inner.register_callsite(metadata);
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
                        self.collector.update_timing(span.key, start, end);
                    }
                }
            }
        }

        drop(spans);
        #[allow(deprecated)]
        self.inner.drop_span(id.clone());
    }
}

impl MetricVisitor {
    /// Create a new visitor with the underlying collector.
    pub fn new(collector: Collector) -> Self {
        MetricVisitor { collector }
    }
}

impl Visit for MetricVisitor {
    fn record_str(&mut self, _field: &Field, _value: &str) {}

    fn record_debug(&mut self, _field: &Field, _value: &dyn fmt::Debug) {}

    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name().ends_with("_counter") {
            self.collector.update_count(field.name(), value as i64);
        } else if field.name().ends_with("_gauge") {
            self.collector.update_gauge(field.name(), value);
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        if field.name().ends_with("_counter") {
            self.collector.update_count(field.name(), value);
        } else if field.name().ends_with("_gauge") {
            self.collector.update_gauge(field.name(), value as u64);
        }
    }
}

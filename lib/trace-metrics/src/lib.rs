//! A `tokio-trace` metrics based subscriber
//!
//! This subscriber takes another subscriber like `tokio-trace-fmt` and wraps it
//! with this basic subscriber. It will enable all spans and events that match the
//! metric capturing criteria. This means every span is enabled regardless of its level
//! and any event that contains a `counter` or a `gauge` field name.
//!
//! # Example
//!
//! ```
//! # #[macro_use] extern crate tokio_trace;
//! # extern crate tokio_trace_fmt;
//! # extern crate trace_metrics;
//! # extern crate hotmic;
//! # use hotmic::Receiver;
//! # use trace_metrics::MetricsSubscriber;
//! # use tokio_trace_fmt::FmtSubscriber;
//! // Get the metrics sink
//! let mut receiver = Receiver::builder().build();
//! let sink = receiver.get_sink();
//!
//! // Setup the subscribers
//! let fmt_subscriber = FmtSubscriber::builder().finish();
//! let metric_subscriber = MetricsSubscriber::new(fmt_subscriber, sink);
//!
//! tokio_trace::subscriber::with_default(metric_subscriber, || {
//!     info!({ do_something_counter = 1 }, "Do some logging");
//! })
//! ```

#[warn(missing_debug_implementations, missing_docs)]
extern crate hotmic;
extern crate tokio_trace_core;

use hotmic::Sink;
use std::{collections::HashMap, fmt, sync::Mutex};
use tokio_trace_core::{
    field::{Field, Visit},
    span::{Attributes, Id, Record},
    Event, Interest, Metadata, Subscriber,
};

/// Metrics collector
// TODO(lucio): move this to a trait
pub type Collector = Sink<String>;

/// The subscriber that wraps another subscriber and produces metrics
pub struct MetricsSubscriber<S> {
    inner: S,
    spans: Mutex<HashMap<Id, Span>>,
    collector: Collector,
}

/// A `tokio_trace_core::field::Visit` implementation that captures fields
/// that contain `counter` or `gague` in their name and dispatches the `i64`
/// or `u64` value to the underlying metrics sink.
pub struct MetricVisitor {
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
    /// Create a new `MetricsSubscriber` with the underlying subscriber and collector.
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
        // TODO(lucio): also enable all callsites taht contain `counter` and `gauge`
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
        let mut recorder = MetricVisitor::new(self.collector.clone());
        event.record(&mut recorder);
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

impl MetricVisitor {
    /// Create a new visitor with the underlying collector.
    pub fn new(collector: Collector) -> Self {
        MetricVisitor { collector }
    }
}

impl Visit for MetricVisitor {
    fn record_str(&mut self, _field: &Field, _value: &str) {}

    fn record_debug(&mut self, _field: &Field, _value: &fmt::Debug) {}

    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name().contains("counter") {
            self.collector
                .update_count(field.name().to_string(), value as i64);
        } else if field.name().contains("guage") {
            self.collector.update_gauge(field.name().to_string(), value);
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        if field.name().contains("counter") {
            self.collector.update_count(field.name().to_string(), value);
        } else if field.name().contains("guage") {
            self.collector
                .update_gauge(field.name().to_string(), value as u64);
        }
    }
}

#![allow(clippy::print_stdout)] // tests
#![allow(clippy::print_stderr)] // tests
#![deny(missing_docs)]

//! This is a framework for testing components for their compliance with
//! the component spec in `docs/specs/component.md` by capturing emitted
//! internal events and metrics, and testing that they fit the required
//! patterns.

use std::env;

use futures::{stream, SinkExt, Stream, StreamExt};
use lazy_static::lazy_static;
use vector_core::event_test_util;

use crate::{
    event::{Event, EventArray, Metric, MetricValue},
    metrics::{self, Controller},
    sinks::VectorSink,
};

/// The standard set of tags for sources that poll connections over HTTP.
pub const HTTP_PULL_SOURCE_TAGS: [&str; 2] = ["endpoint", "protocol"];

/// The standard set of tags for sources that accept connections over HTTP.
pub const HTTP_PUSH_SOURCE_TAGS: [&str; 2] = ["http_path", "protocol"];

/// The standard set of tags for all `TcpSource`-based sources.
pub const TCP_SOURCE_TAGS: [&str; 2] = ["peer_addr", "protocol"];

/// The standard set of tags for all sinks that write a file.
pub const FILE_SINK_TAGS: [&str; 2] = ["file", "protocol"];

/// The standard set of tags for all `HttpSink`-based sinks.
pub const HTTP_SINK_TAGS: [&str; 2] = ["endpoint", "protocol"];

/// The standard set of tags for all `AWS`-based sinks.
pub const AWS_SINK_TAGS: [&str; 2] = ["protocol", "region"];

/// This struct is used to describe a set of component tests.
pub struct ComponentTests {
    /// The list of event (suffixes) that must be emitted by the component
    events: &'static [&'static str],
    /// The list of counter metrics (with given tags) that must be incremented
    tagged_counters: &'static [&'static str],
    /// The list of counter metrics (with no particular tags) that must be incremented
    untagged_counters: &'static [&'static str],
}

lazy_static! {
    /// The component test specification for all sources
    pub static ref SOURCE_TESTS: ComponentTests = ComponentTests {
        events: &["BytesReceived", "EventsReceived", "EventsSent"],
        tagged_counters: &[
            "component_received_bytes_total",
        ],
        untagged_counters: &[
            "component_received_events_total",
            "component_received_event_bytes_total",
            "component_sent_events_total",
            "component_sent_event_bytes_total",
        ],
    };
    /// The component test specification for all sinks
    pub static ref SINK_TESTS: ComponentTests = ComponentTests {
        events: &["EventsSent", "BytesSent"], // EventsReceived is emitted in the topology
        tagged_counters: &[
            "component_sent_bytes_total",
        ],
        untagged_counters: &[
            "component_sent_events_total",
            "component_sent_event_bytes_total",
        ],
    };
    /// The component test specification for components with multiple outputs
    pub static ref COMPONENT_MULTIPLE_OUTPUTS_TESTS: ComponentTests = ComponentTests {
        events: &["EventsSent"],
        tagged_counters: &[
            "component_sent_events_total",
            "component_sent_event_bytes_total",
        ],
        untagged_counters: &[
        ],
    };
}

impl ComponentTests {
    /// Run the test specification, and assert that all tests passed
    pub fn assert(&self, tags: &[&str]) {
        let mut test = ComponentTester::new();
        test.emitted_all_events(self.events);
        test.emitted_all_counters(self.tagged_counters, tags);
        test.emitted_all_counters(self.untagged_counters, &[]);
        if !test.errors.is_empty() {
            panic!(
                "Failed to assert compliance, errors:\n    {}\n",
                test.errors.join("\n    ")
            );
        }
    }
}

/// Initialize the necessary bits needed to run a component test specification.
pub fn init_test() {
    super::trace_init();
    event_test_util::clear_recorded_events();

    // Handle multiple initializations.
    if let Err(error) = metrics::init_test() {
        if error != metrics::Error::AlreadyInitialized {
            panic!("Failed to initialize metrics recorder: {:?}", error);
        }
    }
}

/// Tests if the given metric contains all the given tag names
fn has_tags(metric: &Metric, names: &[&str]) -> bool {
    metric
        .tags()
        .map(|tags| names.iter().all(|name| tags.contains_key(*name)))
        .unwrap_or_else(|| names.is_empty())
}

/// Standard metrics test environment data
struct ComponentTester {
    metrics: Vec<Metric>,
    errors: Vec<String>,
}

impl ComponentTester {
    fn new() -> Self {
        let mut metrics = Controller::get().unwrap().capture_metrics();

        if env::var("DEBUG_COMPONENT_COMPLIANCE").is_ok() {
            event_test_util::debug_print_events();
            metrics.sort_by(|a, b| a.name().cmp(b.name()));
            for metric in &metrics {
                println!("{}", metric);
            }
        }

        let errors = Vec::new();
        Self { metrics, errors }
    }

    fn emitted_all_counters(&mut self, names: &[&str], tags: &[&str]) {
        let tag_suffix = (!tags.is_empty())
            .then(|| format!("{{{}}}", tags.join(",")))
            .unwrap_or_else(String::new);
        for name in names {
            if !self.metrics.iter().any(|m| {
                matches!(m.value(), MetricValue::Counter { .. })
                    && m.name() == *name
                    && has_tags(m, tags)
            }) {
                self.errors
                    .push(format!("Missing metric named {}{}", name, tag_suffix));
            }
        }
    }

    fn emitted_all_events(&mut self, names: &[&str]) {
        for name in names {
            if !event_test_util::contains_name(name) {
                self.errors.push(format!("Missing emitted event {}", name));
            }
        }
    }
}

/// Convenience wrapper for running sink tests
pub async fn run_sink<S>(sink: VectorSink, events: S, tags: &[&str])
where
    S: Stream<Item = EventArray> + Send,
{
    init_test();
    sink.run(events).await.expect("Running sink failed");
    SINK_TESTS.assert(tags);
}

/// Convenience wrapper for running sink tests with a stream of `Event`
pub async fn run_sink_events<S>(sink: VectorSink, events: S, tags: &[&str])
where
    S: Stream<Item = Event> + Send,
{
    init_test();
    let events = events.map(Into::into);
    sink.run(events).await.expect("Running sink failed");
    SINK_TESTS.assert(tags);
}

/// Convenience wrapper for running a sink with a single event
pub async fn run_sink_event(sink: VectorSink, event: Event, tags: &[&str]) {
    init_test();
    run_sink(sink, stream::once(std::future::ready(event.into())), tags).await
}

/// Convenience wrapper for running sinks with `send_all`
pub async fn sink_send_all<I>(sink: VectorSink, events: I, tags: &[&str])
where
    I: IntoIterator<Item = Event>,
    I::IntoIter: Send,
{
    sink_send_stream(sink, stream::iter(events.into_iter().map(Ok)), tags).await
}

/// Convenience wrapper for running sinks with a stream of events
pub async fn sink_send_stream<S>(sink: VectorSink, events: S, tags: &[&str])
where
    S: Stream<Item = Result<Event, ()>> + Send + Unpin,
{
    init_test();
    let mut events = events.map(|result| result.map(|event| event.into()));
    match sink {
        VectorSink::Sink(mut sink) => {
            sink.send_all(&mut events)
                .await
                .expect("Sending event stream to sink failed");
        }
        VectorSink::Stream(stream) => {
            let events = events.filter_map(|x| async move { x.ok() }).boxed();
            stream
                .run(events)
                .await
                .expect("Sending event stream to sink failed");
        }
    }
    SINK_TESTS.assert(tags);
}

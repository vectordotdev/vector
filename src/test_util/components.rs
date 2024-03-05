#![allow(clippy::print_stdout)] // tests
#![allow(clippy::print_stderr)] // tests
#![deny(missing_docs)]

//! This is a framework for testing components for their compliance with
//! the component spec in `docs/specs/component.md` by capturing emitted
//! internal events and metrics, and testing that they fit the required
//! patterns.

use std::{env, time::Duration};

use futures::{stream, SinkExt, Stream, StreamExt};
use futures_util::Future;
use once_cell::sync::Lazy;
use tokio::{pin, select, time::sleep};
use vector_lib::event_test_util;

use crate::{
    config::{SourceConfig, SourceContext},
    event::{Event, EventArray, Metric, MetricValue},
    metrics::Controller,
    sinks::VectorSink,
    SourceSender,
};

/// The most basic set of tags for sources, regardless of whether or not they pull data or have it pushed in.
pub const SOURCE_TAGS: [&str; 1] = ["protocol"];

/// The most basic set of error tags for components.
pub const COMPONENT_ERROR_TAGS: [&str; 1] = ["error_type"];

/// The standard set of tags for sources that have their data pushed in from an external source.
pub const PUSH_SOURCE_TAGS: [&str; 2] = ["endpoint", "protocol"];

/// The standard set of tags for sources that pull their data from an external source.
pub const PULL_SOURCE_TAGS: [&str; 2] = ["endpoint", "protocol"];

/// The standard set of tags for sources that poll connections over HTTP.
pub const HTTP_PULL_SOURCE_TAGS: [&str; 2] = ["endpoint", "protocol"];

/// The standard set of tags for sources that accept connections over HTTP.
pub const HTTP_PUSH_SOURCE_TAGS: [&str; 2] = ["http_path", "protocol"];

/// The standard set of tags for all generic socket-based sources that accept connections i.e. `TcpSource`.
pub const SOCKET_PUSH_SOURCE_TAGS: [&str; 1] = ["protocol"];

/// The standard set of tags for all generic socket-based sources that poll connections i.e. Redis.
pub const SOCKET_PULL_SOURCE_TAGS: [&str; 2] = ["remote_addr", "protocol"];

/// The standard set of tags for all sources that read a file.
pub const FILE_SOURCE_TAGS: [&str; 1] = ["file"];

/// The most basic set of tags for sinks, regardless of whether or not they push data or have it pulled out.
pub const SINK_TAGS: [&str; 1] = ["protocol"];

/// The set of tags for sinks measuring data volume with source and service identification.
pub const DATA_VOLUME_SINK_TAGS: [&str; 2] = ["source", "service"];

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

/// The component test specification for all sources.
pub static SOURCE_TESTS: Lazy<ComponentTests> = Lazy::new(|| ComponentTests {
    events: &["BytesReceived", "EventsReceived", "EventsSent"],
    tagged_counters: &["component_received_bytes_total"],
    untagged_counters: &[
        "component_received_events_total",
        "component_received_event_bytes_total",
        "component_sent_events_total",
        "component_sent_event_bytes_total",
    ],
});

/// The component error test specification (sources and sinks).
pub static COMPONENT_TESTS_ERROR: Lazy<ComponentTests> = Lazy::new(|| ComponentTests {
    events: &["Error"],
    tagged_counters: &["component_errors_total"],
    untagged_counters: &[],
});

/// The component test specification for all transforms.
pub static TRANSFORM_TESTS: Lazy<ComponentTests> = Lazy::new(|| ComponentTests {
    events: &["EventsReceived", "EventsSent"],
    tagged_counters: &[],
    untagged_counters: &[
        "component_received_events_total",
        "component_received_event_bytes_total",
        "component_sent_events_total",
        "component_sent_event_bytes_total",
    ],
});

/// The component test specification for sinks that are push-based.
pub static SINK_TESTS: Lazy<ComponentTests> = Lazy::new(|| {
    ComponentTests {
        events: &["BytesSent", "EventsSent"], // EventsReceived is emitted in the topology
        tagged_counters: &["component_sent_bytes_total"],
        untagged_counters: &[
            "component_sent_events_total",
            "component_sent_event_bytes_total",
        ],
    }
});

/// The component test specification for sinks with source and service identification.
pub static DATA_VOLUME_SINK_TESTS: Lazy<ComponentTests> = Lazy::new(|| {
    ComponentTests {
        events: &["BytesSent", "EventsSent"], // EventsReceived is emitted in the topology
        tagged_counters: &[
            "component_sent_events_total",
            "component_sent_event_bytes_total",
        ],
        untagged_counters: &[],
    }
});

/// The component test specification for sinks which simply expose data, or do not otherwise "send" it anywhere.
pub static NONSENDING_SINK_TESTS: Lazy<ComponentTests> = Lazy::new(|| ComponentTests {
    events: &["EventsSent"],
    tagged_counters: &[
        "component_sent_events_total",
        "component_sent_event_bytes_total",
    ],
    untagged_counters: &[],
});

/// The component test specification for components with multiple outputs.
pub static COMPONENT_MULTIPLE_OUTPUTS_TESTS: Lazy<ComponentTests> = Lazy::new(|| ComponentTests {
    events: &["EventsSent"],
    tagged_counters: &[
        "component_sent_events_total",
        "component_sent_event_bytes_total",
    ],
    untagged_counters: &[],
});

impl ComponentTests {
    /// Run the test specification, and assert that all tests passed.
    #[track_caller]
    pub fn assert(&self, tags: &[&str]) {
        let mut test = ComponentTester::new();
        test.emitted_all_events(self.events);
        test.emitted_all_counters(self.tagged_counters, tags);
        test.emitted_all_counters(self.untagged_counters, &[]);
        if !test.errors.is_empty() {
            panic!(
                "Failed to assert compliance, errors:\n{}\n",
                test.errors.join("\n")
            );
        }
    }
}

/// Initialize the necessary bits needed to run a component test specification.
pub fn init_test() {
    super::trace_init();
    event_test_util::clear_recorded_events();
}

/// Tests if the given metric contains all the given tag names
fn has_tags(metric: &Metric, names: &[&str]) -> bool {
    metric
        .tags()
        .map(|tags| names.iter().all(|name| tags.contains_key(name)))
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
            .unwrap_or_default();

        for name in names {
            if !self.metrics.iter().any(|m| {
                matches!(m.value(), MetricValue::Counter { .. })
                    && m.name() == *name
                    && has_tags(m, tags)
            }) {
                // If we didn't find a direct match, see if any other metrics exist which are counters of the same name,
                // which could represent metrics being emitted but without the correct tag(s).
                let partial_matches = self
                    .metrics
                    .iter()
                    .filter(|m| {
                        matches!(m.value(), MetricValue::Counter { .. })
                            && m.name() == *name
                            && !has_tags(m, tags)
                    })
                    .map(|m| {
                        let tags = m
                            .tags()
                            .map(|t| format!("{{{}}}", itertools::join(t.keys(), ",")))
                            .unwrap_or_default();
                        format!("\n    -> Found similar metric `{}{}`", m.name(), tags)
                    })
                    .collect::<Vec<_>>();
                let partial = partial_matches.join("");

                self.errors.push(format!(
                    "  - Missing metric `{}{}`{}",
                    name, tag_suffix, partial
                ));
            }
        }
    }

    fn emitted_all_events(&mut self, names: &[&str]) {
        for name in names {
            if let Err(err_msg) = event_test_util::contains_name_once(name) {
                self.errors.push(format!("  - {}", err_msg));
            }
        }
    }
}

/// Runs and returns a future and asserts that the provided test specification passes.
pub async fn assert_source<T>(
    tests: &Lazy<ComponentTests>,
    tags: &[&str],
    f: impl Future<Output = T>,
) -> T {
    init_test();

    let result = f.await;

    tests.assert(tags);

    result
}

/// Convenience wrapper for running source tests.
pub async fn assert_source_compliance<T>(tags: &[&str], f: impl Future<Output = T>) -> T {
    assert_source(&SOURCE_TESTS, tags, f).await
}

/// Runs source tests with timeout and asserts happy path compliance.
pub async fn run_and_assert_source_compliance<SC>(
    source: SC,
    timeout: Duration,
    tags: &[&str],
) -> Vec<Event>
where
    SC: SourceConfig,
{
    run_and_assert_source_advanced(source, |_| {}, Some(timeout), None, &SOURCE_TESTS, tags).await
}

/// Runs source tests with an event count limit and asserts happy path compliance.
pub async fn run_and_assert_source_compliance_n<SC>(
    source: SC,
    event_count: usize,
    tags: &[&str],
) -> Vec<Event>
where
    SC: SourceConfig,
{
    run_and_assert_source_advanced(source, |_| {}, None, Some(event_count), &SOURCE_TESTS, tags)
        .await
}

/// Runs and returns a future and asserts that the provided test specification passes.
pub async fn assert_source_error<T>(tags: &[&str], f: impl Future<Output = T>) -> T {
    init_test();

    let result = f.await;

    COMPONENT_TESTS_ERROR.assert(tags);

    result
}

/// Runs source tests with timeout and asserts error path compliance.
pub async fn run_and_assert_source_error<SC>(
    source: SC,
    timeout: Duration,
    tags: &[&str],
) -> Vec<Event>
where
    SC: SourceConfig,
{
    run_and_assert_source_advanced(
        source,
        |_| {},
        Some(timeout),
        None,
        &COMPONENT_TESTS_ERROR,
        tags,
    )
    .await
}

/// Runs source tests with setup, timeout, and event count limit and asserts happy path compliance.
pub async fn run_and_assert_source_compliance_advanced<SC>(
    source: SC,
    setup: impl FnOnce(&mut SourceContext),
    timeout: Option<Duration>,
    event_count: Option<usize>,
    tags: &[&str],
) -> Vec<Event>
where
    SC: SourceConfig,
{
    run_and_assert_source_advanced(source, setup, timeout, event_count, &SOURCE_TESTS, tags).await
}

/// Runs and asserts source test specifications with configurations.
pub async fn run_and_assert_source_advanced<SC>(
    source: SC,
    setup: impl FnOnce(&mut SourceContext),
    timeout: Option<Duration>,
    event_count: Option<usize>,
    tests: &Lazy<ComponentTests>,
    tags: &[&str],
) -> Vec<Event>
where
    SC: SourceConfig,
{
    assert_source(tests, tags, async move {
        // Build the source and set ourselves up to both drive it to completion as well as collect all the events it sends out.
        let (tx, mut rx) = SourceSender::new_test();
        let mut context = SourceContext::new_test(tx, None);

        setup(&mut context);

        let mut source = source
            .build(context)
            .await
            .expect("source should not fail to build");

        // If a timeout was given, use that, otherwise, use an infinitely long one.
        let source_timeout = sleep(timeout.unwrap_or_else(|| Duration::from_nanos(u64::MAX)));
        pin!(source_timeout);

        let mut events = Vec::new();

        // Try and drive both our timeout and the source itself, while collecting any events that the source sends out in
        // the meantime.  We store these locally and return them all at the end.
        loop {
            // If an event count was given, and we've hit it, break out of the loop.
            if let Some(count) = event_count {
                if events.len() == count {
                    break;
                }
            }

            select! {
                _ = &mut source_timeout => break,
                Some(event) = rx.next() => events.push(event),
                _ = &mut source => break,
            }
        }

        drop(source);

        // Drain any remaining events that we didn't get to before our timeout.
        //
        // If an event count was given, break out if we've reached the limit. Otherwise, just drain the remaining events
        // until no more are left, which avoids timing issues with missing events that came in right when the timeout
        // fired.
        while let Some(event) = rx.next().await {
            if let Some(count) = event_count {
                if events.len() == count {
                    break;
                }
            }

            events.push(event);
        }

        events
    })
    .await
}

/// Runs and asserts compliance for transforms.
pub async fn assert_transform_compliance<T>(f: impl Future<Output = T>) -> T {
    init_test();

    let result = f.await;

    TRANSFORM_TESTS.assert(&[]);

    result
}

/// Convenience wrapper for running sink tests
pub async fn assert_sink_compliance<T>(tags: &[&str], f: impl Future<Output = T>) -> T {
    init_test();

    let result = f.await;

    SINK_TESTS.assert(tags);

    result
}

/// Runs and asserts sink compliance.
pub async fn run_and_assert_sink_compliance<S, I>(sink: VectorSink, events: S, tags: &[&str])
where
    S: Stream<Item = I> + Send,
    I: Into<EventArray>,
{
    assert_sink_compliance(tags, async move {
        let events = events.map(Into::into);
        sink.run(events).await.expect("Running sink failed")
    })
    .await;
}

/// Convenience wrapper for running sink tests
pub async fn assert_data_volume_sink_compliance<T>(tags: &[&str], f: impl Future<Output = T>) -> T {
    init_test();

    let result = f.await;

    DATA_VOLUME_SINK_TESTS.assert(tags);

    result
}

/// Runs and asserts compliance for data volume sink tests.
pub async fn run_and_assert_data_volume_sink_compliance<S, I>(
    sink: VectorSink,
    events: S,
    tags: &[&str],
) where
    S: Stream<Item = I> + Send,
    I: Into<EventArray>,
{
    assert_data_volume_sink_compliance(tags, async move {
        let events = events.map(Into::into);
        sink.run(events).await.expect("Running sink failed")
    })
    .await;
}

/// Asserts compliance for nonsending sink tests.
pub async fn assert_nonsending_sink_compliance<T>(tags: &[&str], f: impl Future<Output = T>) -> T {
    init_test();

    let result = f.await;

    NONSENDING_SINK_TESTS.assert(tags);

    result
}

/// Runs and asserts compliance for nonsending sink tests.
pub async fn run_and_assert_nonsending_sink_compliance<S, I>(
    sink: VectorSink,
    events: S,
    tags: &[&str],
) where
    S: Stream<Item = I> + Send,
    I: Into<EventArray>,
{
    assert_nonsending_sink_compliance(tags, async move {
        let events = events.map(Into::into);
        sink.run(events).await.expect("Running sink failed")
    })
    .await;
}

/// Convenience wrapper for running sink error tests
pub async fn assert_sink_error<T>(tags: &[&str], f: impl Future<Output = T>) -> T {
    init_test();

    let result = f.await;

    COMPONENT_TESTS_ERROR.assert(tags);

    result
}

/// Runs and asserts sink error compliance.
pub async fn run_and_assert_sink_error<S, I>(sink: VectorSink, events: S, tags: &[&str])
where
    S: Stream<Item = I> + Send,
    I: Into<EventArray>,
{
    assert_sink_error(tags, async move {
        let events = events.map(Into::into);
        sink.run(events).await.expect("Running sink failed")
    })
    .await;
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

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::time::Duration;
use vector_lib::buffers::{BufferConfig, BufferType, WhenFull};
use vector_lib::config::MEMORY_BUFFER_DEFAULT_MAX_EVENTS;

use crate::{config::Config, test_util, test_util::start_topology};
use crate::{config::SinkOuter, test_util::mock::backpressure_source};
use crate::{test_util::mock::backpressure_sink, topology::builder::SOURCE_SENDER_BUFFER_SIZE};

// Based on how we pump events from `SourceSender` into `Fanout`, there's always one extra event we
// may pull out of `SourceSender` but can't yet send into `Fanout`, so we account for that here.
const EXTRA_SOURCE_PUMP_EVENT: usize = 1;

/// Connects a single source to a single sink and makes sure the sink backpressure is propagated
/// to the source.
#[tokio::test]
async fn serial_backpressure() {
    test_util::trace_init();

    let mut config = Config::builder();

    let events_to_sink = 100;

    let expected_sourced_events = events_to_sink
        + MEMORY_BUFFER_DEFAULT_MAX_EVENTS.get()
        + *SOURCE_SENDER_BUFFER_SIZE
        + EXTRA_SOURCE_PUMP_EVENT;

    let source_counter = Arc::new(AtomicUsize::new(0));
    config.add_source("in", backpressure_source(&source_counter));
    config.add_sink("out", &["in"], backpressure_sink(events_to_sink));

    let (_topology, _) = start_topology(config.build().unwrap(), false).await;

    // allow the topology to run
    wait_until_expected(&source_counter, expected_sourced_events).await;

    let sourced_events = source_counter.load(Ordering::Acquire);

    assert_eq!(sourced_events, expected_sourced_events);
}

/// Connects a single source to two sinks and makes sure that the source is only able
/// to emit events that the slower sink accepts.
#[tokio::test]
async fn default_fan_out() {
    test_util::trace_init();

    let mut config = Config::builder();

    let events_to_sink = 100;

    let expected_sourced_events = events_to_sink
        + MEMORY_BUFFER_DEFAULT_MAX_EVENTS.get()
        + *SOURCE_SENDER_BUFFER_SIZE
        + EXTRA_SOURCE_PUMP_EVENT;

    let source_counter = Arc::new(AtomicUsize::new(0));
    config.add_source("in", backpressure_source(&source_counter));
    config.add_sink("out1", &["in"], backpressure_sink(events_to_sink * 2));

    config.add_sink("out2", &["in"], backpressure_sink(events_to_sink));

    let (_topology, _) = start_topology(config.build().unwrap(), false).await;

    // allow the topology to run
    wait_until_expected(&source_counter, expected_sourced_events).await;

    let sourced_events = source_counter.load(Ordering::Relaxed);

    assert_eq!(sourced_events, expected_sourced_events);
}

/// Connects a single source to two sinks. One of the sinks is configured to drop events that exceed
/// the buffer size. Asserts that the sink that drops events does not cause backpressure, but the
/// other one does.
#[tokio::test]
async fn buffer_drop_fan_out() {
    test_util::trace_init();

    let mut config = Config::builder();

    let events_to_sink = 100;

    let expected_sourced_events = events_to_sink
        + MEMORY_BUFFER_DEFAULT_MAX_EVENTS.get()
        + *SOURCE_SENDER_BUFFER_SIZE
        + EXTRA_SOURCE_PUMP_EVENT;

    let source_counter = Arc::new(AtomicUsize::new(0));
    config.add_source("in", backpressure_source(&source_counter));
    config.add_sink("out1", &["in"], backpressure_sink(events_to_sink));

    let mut sink_outer = SinkOuter::new(
        vec!["in".to_string()],
        backpressure_sink(events_to_sink / 2),
    );
    sink_outer.buffer = BufferConfig::Single(BufferType::Memory {
        max_events: MEMORY_BUFFER_DEFAULT_MAX_EVENTS,
        when_full: WhenFull::DropNewest,
    });
    config.add_sink_outer("out2", sink_outer);

    let (_topology, _) = start_topology(config.build().unwrap(), false).await;

    // allow the topology to run
    wait_until_expected(&source_counter, expected_sourced_events).await;

    let sourced_events = source_counter.load(Ordering::Relaxed);

    assert_eq!(sourced_events, expected_sourced_events);
}

/// Connects 2 sources to a single sink, and asserts that the sum of the events produced
/// by the sources is how many the single sink accepted.
#[tokio::test]
#[ignore]
async fn multiple_inputs_backpressure() {
    test_util::trace_init();

    // TODO: I think this test needs to be reworked slightly.
    //
    // The test is meant to indicate that the sum of the events produced by both sources matches what the sink receives,
    // but the sources run in an unbounded fashion, so all we're testing currently is that the sink eventually gets N
    // events, where N is `expected_sourced_events`.
    //
    // Instead, we would need to do something where we we actually _didn't_ consume any events in the sink, and asserted
    // that when both sources could no longer send events, the total number of events they managed to send equals
    // `expected_sourced_events`, as that value is intended to be representative of how many events should be sendable
    // before all of the interstitial buffers have been filled, etc.
    //
    // As-is, it seems like `expected_sourced_events` is much larger after a change to how we calculate available
    // parallelism, which leads to this test failing to complete within the timeout, hence the `#[ignore]`.
    let mut config = Config::builder();

    let events_to_sink = 100;

    let expected_sourced_events = events_to_sink
        + MEMORY_BUFFER_DEFAULT_MAX_EVENTS.get()
        + *SOURCE_SENDER_BUFFER_SIZE * 2
        + EXTRA_SOURCE_PUMP_EVENT * 2;

    let source_counter = Arc::new(AtomicUsize::new(0));
    config.add_source("in1", backpressure_source(&source_counter));
    config.add_source("in2", backpressure_source(&source_counter));
    config.add_sink("out", &["in1", "in2"], backpressure_sink(events_to_sink));

    let (_topology, _) = start_topology(config.build().unwrap(), false).await;

    // allow the topology to run
    wait_until_expected(&source_counter, expected_sourced_events).await;

    let sourced_events_sum = source_counter.load(Ordering::Relaxed);

    assert_eq!(sourced_events_sum, expected_sourced_events);
}

// Wait until the source has sent at least the expected number of events, plus a small additional
// margin to ensure we allow it to run over the expected amount if it's going to.
async fn wait_until_expected(source_counter: impl AsRef<AtomicUsize>, expected: usize) {
    crate::test_util::wait_for_atomic_usize(source_counter, |count| count >= expected).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
}

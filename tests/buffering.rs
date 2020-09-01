#![cfg(feature = "leveldb")]

use futures::{
    compat::{Future01CompatExt, Sink01CompatExt},
    SinkExt,
};
use prost::Message;
use tempfile::tempdir;
use tokio::runtime::Runtime;
use tracing::trace;
use vector::{
    buffers::BufferConfig,
    config, event,
    test_util::{
        random_events_with_stream, runtime, start_topology, trace_init, wait_for_atomic_usize,
        CountReceiver,
    },
    topology,
};

mod support;

fn terminate_abruptly(rt: Runtime, topology: topology::RunningTopology) {
    drop(rt);
    drop(topology);
}

#[test]
fn test_buffering() {
    trace_init();

    let data_dir = tempdir().unwrap();
    let data_dir = data_dir.path().to_path_buf();
    trace!(message = "Test data dir", ?data_dir);

    let num_events: usize = 10;
    let line_length = 100;
    let max_size = 10_000;
    let expected_events_count = num_events * 2;

    assert!(
        line_length * expected_events_count <= max_size,
        "Test parameters are invalid, this test implies  that all lines will fit
        into the buffer, but the buffer is not big enough"
    );

    // Run vector with a dead sink, and then shut it down without sink ever
    // accepting any data.
    let (in_tx, source_config, source_event_counter) = support::source_with_event_counter();
    let sink_config = support::sink_dead();
    let config = {
        let mut config = config::Config::empty();
        config.add_source("in", source_config);
        config.add_sink("out", &["in"], sink_config);
        config.sinks["out"].buffer = BufferConfig::Disk {
            max_size,
            when_full: Default::default(),
        };
        config.global.data_dir = Some(data_dir.clone());
        config
    };

    let mut rt = runtime();
    let (topology, input_events) = rt.block_on(async move {
        let (topology, _crash) = start_topology(config, false).await;
        let (input_events, mut input_events_stream) =
            random_events_with_stream(line_length, num_events);

        let _ = in_tx
            .sink_compat()
            .sink_map_err(|err| panic!(err))
            .send_all(&mut input_events_stream)
            .await
            .unwrap();

        // We need to wait for at least the source to process events.
        // This is to avoid a race after we send all events, at that point two things
        // can happen in any order, we reaching `terminate_abruptly` and source processing
        // all of the events. We need for the source to process events before `terminate_abruptly`
        // so we wait for that here.
        wait_for_atomic_usize(source_event_counter, |x| x == num_events).await;

        (topology, input_events)
    });

    // Now we give it some time for the events to propagate to File.
    // This is to avoid a race after the source processes all events, at that point two things
    // can happen in any order, we reaching `terminate_abruptly` and events being written
    // to file. We need for the events to be written to the file before `terminate_abruptly`.
    // We can't know when exactly all of the events have been written, so we have to guess.
    // But it should be shortly after source processing all of the events.
    std::thread::sleep(std::time::Duration::from_secs(1));
    // Simulate a crash.
    terminate_abruptly(rt, topology);

    // Then run vector again with a sink that accepts events now. It should
    // send all of the events from the first run.
    let (in_tx, source_config) = support::source();
    let (out_rx, sink_config) = support::sink(10);
    let config = {
        let mut config = config::Config::empty();
        config.add_source("in", source_config);
        config.add_sink("out", &["in"], sink_config);
        config.sinks["out"].buffer = BufferConfig::Disk {
            max_size,
            when_full: Default::default(),
        };
        config.global.data_dir = Some(data_dir);
        config
    };

    let mut rt = runtime();
    rt.block_on(async move {
        let (topology, _crash) = start_topology(config, false).await;

        let (input_events2, mut input_events_stream) =
            random_events_with_stream(line_length, num_events);

        let _ = in_tx
            .sink_compat()
            .sink_map_err(|err| panic!(err))
            .send_all(&mut input_events_stream)
            .await
            .unwrap();

        let output_events = CountReceiver::receive_events(out_rx);

        topology.stop().compat().await.unwrap();

        let output_events = output_events.await;
        assert_eq!(expected_events_count, output_events.len());
        assert_eq!(input_events, &output_events[..num_events]);
        assert_eq!(input_events2, &output_events[num_events..]);
    });
}

#[test]
fn test_max_size() {
    trace_init();

    let data_dir = tempdir().unwrap();
    let data_dir = data_dir.path().to_path_buf();
    trace!(message = "Test data dir", ?data_dir);

    let num_events: usize = 1000;
    let line_length = 1000;
    let (input_events, mut input_events_stream) =
        random_events_with_stream(line_length, num_events);

    let max_size = input_events
        .clone()
        .into_iter()
        .take(num_events / 2)
        .map(event::proto::EventWrapper::from)
        .map(|ew| ew.encoded_len())
        .sum();

    // Run vector with a dead sink, and then shut it down without sink ever
    // accepting any data.
    let (in_tx, source_config, source_event_counter) = support::source_with_event_counter();
    let sink_config = support::sink_dead();
    let config = {
        let mut config = config::Config::empty();
        config.add_source("in", source_config);
        config.add_sink("out", &["in"], sink_config);
        config.sinks["out"].buffer = BufferConfig::Disk {
            max_size,
            when_full: Default::default(),
        };
        config.global.data_dir = Some(data_dir.clone());
        config
    };

    let mut rt = runtime();
    let topology = rt.block_on(async move {
        let (topology, _crash) = start_topology(config, false).await;

        let _ = in_tx
            .sink_compat()
            .sink_map_err(|err| panic!(err))
            .send_all(&mut input_events_stream)
            .await
            .unwrap();

        // We need to wait for at least the source to process events.
        // This is to avoid a race after we send all events, at that point two things
        // can happen in any order, we reaching `terminate_abruptly` and source processing
        // all of the events. We need for the source to process events before `terminate_abruptly`
        // so we wait for that here.
        wait_for_atomic_usize(source_event_counter, |x| x == num_events).await;

        topology
    });

    // Now we give it some time for the events to propagate to File.
    // This is to avoid a race after the source processes all events, at that point two things
    // can happen in any order, we reaching `terminate_abruptly` and events being written
    // to file. We need for the events to be written to the file before `terminate_abruptly`.
    // We can't know when exactly all of the events have been written, so we have to guess.
    // But it should be shortly after source processing all of the events.
    std::thread::sleep(std::time::Duration::from_secs(1));
    // Simulate a crash.
    terminate_abruptly(rt, topology);

    // Then run vector again with a sink that accepts events now. It should
    // send all of the events from the first run that fit in the limited buffer
    // space.
    let (_in_tx, source_config) = support::source();
    let (out_rx, sink_config) = support::sink(10);
    let config = {
        let mut config = config::Config::empty();
        config.add_source("in", source_config);
        config.add_sink("out", &["in"], sink_config);
        config.sinks["out"].buffer = BufferConfig::Disk {
            max_size,
            when_full: Default::default(),
        };
        config.global.data_dir = Some(data_dir);
        config
    };

    let mut rt = runtime();
    rt.block_on(async move {
        let (topology, _crash) = start_topology(config, false).await;

        let output_events = CountReceiver::receive_events(out_rx);

        topology.stop().compat().await.unwrap();

        let output_events = output_events.await;
        assert_eq!(num_events / 2, output_events.len());
        assert_eq!(&input_events[..num_events / 2], &output_events[..]);
    });
}

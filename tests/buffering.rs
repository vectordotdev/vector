#![allow(clippy::match_bool)]
#![allow(clippy::redundant_clone)]
#![cfg(feature = "leveldb")]

use futures01::{Future, Sink};
use prost::Message;
use tempfile::tempdir;
use tracing::trace;
use vector::event;
use vector::test_util::{self, next_addr, wait_for_atomic_usize};
use vector::topology::{self, config};
use vector::{buffers::BufferConfig, runtime, sinks};

mod support;

fn terminate_gracefully(mut rt: runtime::Runtime, topology: topology::RunningTopology) {
    rt.block_on(topology.stop()).unwrap();
    test_util::shutdown_on_idle(rt);
}

fn terminate_abruptly(rt: runtime::Runtime, topology: topology::RunningTopology) {
    rt.shutdown_now().wait().unwrap();
    drop(topology);
}

#[test]
fn test_buffering() {
    test_util::trace_init();

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

    let mut rt = test_util::runtime();

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let (input_events, input_events_stream) =
        test_util::random_events_with_stream(line_length, num_events);
    let send = in_tx
        .sink_map_err(|err| panic!(err))
        .send_all(input_events_stream);
    let _ = rt.block_on(send).unwrap();

    // We need to wait for at least the source to process events.
    // This is to avoid a race after we send all events, at that point two things
    // can happen in any order, we reaching `terminate_abruptly` and source processing
    // all of the events. We need for the source to process events before `terminate_abruptly`
    // so we wait for that here.
    wait_for_atomic_usize(source_event_counter, |x| x == num_events);
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
        config.global.data_dir = Some(data_dir.clone());
        config
    };

    let mut rt = test_util::runtime();

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let (input_events2, input_events_stream) =
        test_util::random_events_with_stream(line_length, num_events);

    let send = in_tx
        .sink_map_err(|err| panic!(err))
        .send_all(input_events_stream);
    let _ = rt.block_on(send).unwrap();

    let output_events = test_util::receive_events(out_rx);

    terminate_gracefully(rt, topology);

    let output_events = output_events.wait();
    assert_eq!(expected_events_count, output_events.len());
    assert_eq!(input_events, &output_events[..num_events]);
    assert_eq!(input_events2, &output_events[num_events..]);
}

#[test]
fn test_max_size() {
    test_util::trace_init();

    let data_dir = tempdir().unwrap();
    let data_dir = data_dir.path().to_path_buf();
    trace!(message = "Test data dir", ?data_dir);

    let num_events: usize = 1000;
    let line_length = 1000;
    let (input_events, input_events_stream) =
        test_util::random_events_with_stream(line_length, num_events);

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

    let mut rt = test_util::runtime();

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let send = in_tx
        .sink_map_err(|err| panic!(err))
        .send_all(input_events_stream);
    let _ = rt.block_on(send).unwrap();

    // We need to wait for at least the source to process events.
    // This is to avoid a race after we send all events, at that point two things
    // can happen in any order, we reaching `terminate_abruptly` and source processing
    // all of the events. We need for the source to process events before `terminate_abruptly`
    // so we wait for that here.
    wait_for_atomic_usize(source_event_counter, |x| x == num_events);
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
        config.global.data_dir = Some(data_dir.clone());
        config
    };

    let mut rt = test_util::runtime();

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let output_events = test_util::receive_events(out_rx);

    terminate_gracefully(rt, topology);

    let output_events = output_events.wait();
    assert_eq!(num_events / 2, output_events.len());
    assert_eq!(&input_events[..num_events / 2], &output_events[..]);
}

#[test]
fn test_max_size_resume() {
    test_util::trace_init();

    let data_dir = tempdir().unwrap();
    let data_dir = data_dir.path().to_path_buf();

    let num_events: usize = 1000;
    let line_length = 1000;
    let max_size = num_events * line_length / 2;

    let out_addr = next_addr();

    let mut config = config::Config::empty();
    let (in1_tx, mut source_config) = support::source();
    source_config.set_data_type(config::DataType::Log);
    config.add_source("in1", source_config);
    let (in2_tx, mut source_config) = support::source();
    source_config.set_data_type(config::DataType::Log);
    config.add_source("in2", source_config);
    config.add_sink(
        "out",
        &["in1", "in2"],
        sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
    );
    config.sinks["out"].buffer = BufferConfig::Disk {
        max_size,
        when_full: Default::default(),
    };
    config.global.data_dir = Some(data_dir.clone());

    // Use a multi-thread runtime here.
    let mut rt = runtime::Runtime::new().unwrap();

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    // Send all of the input events _before_ the output sink is ready.
    // This causes the writers to stop writing to the on-disk buffer, and once
    // the output sink is available and the size of the buffer begins to
    // decrease, they should start writing again.
    let (_, input_events_stream) = test_util::random_events_with_stream(line_length, num_events);
    let send1 = in1_tx
        .sink_map_err(|err| panic!(err))
        .send_all(input_events_stream);
    let (_, input_events_stream) = test_util::random_events_with_stream(line_length, num_events);
    let send2 = in2_tx
        .sink_map_err(|err| panic!(err))
        .send_all(input_events_stream);
    let _ = rt.block_on(send1.join(send2)).unwrap();

    // Simulate a delay before enabling the sink as if sink server is going up.
    std::thread::sleep(std::time::Duration::from_millis(1000));

    let output_lines = test_util::receive(&out_addr);

    terminate_gracefully(rt, topology);

    let output_lines = output_lines.wait();
    assert_eq!(num_events * 2, output_lines.len());
}

#[test]
fn test_reclaim_disk_space() {
    test_util::trace_init();

    let data_dir = tempdir().unwrap();
    let data_dir = data_dir.path().to_path_buf();

    let num_events: usize = 10_000;
    let line_length = 1000;
    let max_size = 1_000_000_000;

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

    let mut rt = test_util::runtime();

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let (input_events, input_events_stream) =
        test_util::random_events_with_stream(line_length, num_events);
    let send = in_tx
        .sink_map_err(|err| panic!(err))
        .send_all(input_events_stream);
    let _ = rt.block_on(send).unwrap();

    // We need to wait for at least the source to process events.
    // This is to avoid a race after we send all events, at that point two things
    // can happen in any order, we reaching `terminate_abruptly` and source processing
    // all of the events. We need for the source to process events before `terminate_abruptly`
    // so we wait for that here.
    wait_for_atomic_usize(source_event_counter, |x| x == num_events);
    // Now we give it some time for the events to propagate to File.
    // This is to avoid a race after the source processes all events, at that point two things
    // can happen in any order, we reaching `terminate_abruptly` and events being written
    // to file. We need for the events to be written to the file before `terminate_abruptly`.
    // We can't know when exactly all of the events have been written, so we have to guess.
    // But it should be shortly after source processing all of the events.
    std::thread::sleep(std::time::Duration::from_secs(1));
    // Simulate a crash.
    terminate_abruptly(rt, topology);

    let before_disk_size: u64 = compute_disk_size(&data_dir);

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
        config.global.data_dir = Some(data_dir.clone());
        config
    };

    let mut rt = test_util::runtime();

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let (input_events2, input_events_stream) =
        test_util::random_events_with_stream(line_length, num_events);

    let send = in_tx
        .sink_map_err(|err| panic!(err))
        .send_all(input_events_stream);
    let _ = rt.block_on(send).unwrap();

    let output_events = test_util::receive_events(out_rx);

    terminate_gracefully(rt, topology);

    let output_events = output_events.wait();
    assert_eq!(num_events * 2, output_events.len());
    assert_eq!(input_events, &output_events[..num_events]);
    assert_eq!(input_events2, &output_events[num_events..]);

    let after_disk_size: u64 = compute_disk_size(&data_dir);

    // Ensure that the disk space after is less than half of the size that it
    // was before we reclaimed the space.
    assert!(after_disk_size < before_disk_size / 2);
}

fn compute_disk_size(dir: impl AsRef<std::path::Path>) -> u64 {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.metadata().ok())
        .filter(|metadata| metadata.is_file())
        .map(|m| m.len())
        .sum()
}

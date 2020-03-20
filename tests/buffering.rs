#![cfg(feature = "leveldb")]

use futures01::{Future, Sink, Stream};
use prost::Message;
use tempfile::tempdir;
use tracing::trace;
use vector::event::{self, Event};
use vector::test_util::{
    self, block_on, next_addr, random_lines, receive, runtime, send_lines, shutdown_on_idle,
    wait_for_tcp,
};
use vector::topology::{self, config};
use vector::{buffers::BufferConfig, runtime, sinks, sources};

mod support;

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

    let mut rt = runtime();

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let (input_events, input_events_stream) =
        test_util::random_events_with_stream(line_length, num_events);
    let send = in_tx
        .sink_map_err(|err| panic!(err))
        .send_all(input_events_stream);
    let _ = rt.block_on(send).unwrap();

    // A race caused by `rt.block_on(send).unwrap()` is handled here. For some
    // reason, at times less events than were sent actually arrive to the
    // `source`.
    // We mitigate that by waiting on the event counter provided by our source
    // mock.
    test_util::wait_for_atomic_usize(source_event_counter, |x| x == num_events);

    rt.block_on(topology.stop()).unwrap();
    shutdown_on_idle(rt);

    // Then run vector again with a sink that accepts events now. It should
    // send all of the events from the first run.
    let (in_tx, source_config, source_event_counter) = support::source_with_event_counter();
    let (mut out_rx, sink_config) = support::sink(expected_events_count + 100);
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

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    let (input_events2, input_events_stream) =
        test_util::random_events_with_stream(line_length, num_events);

    let send = in_tx
        .sink_map_err(|err| panic!(err))
        .send_all(input_events_stream);
    let _ = rt.block_on(send).unwrap();

    // A race caused by `rt.block_on(send).unwrap()` is handled here. For some
    // reason, at times less events than were sent actually arrive to the
    // `source`.
    // We mitigate that by waiting on the event counter provided by our source
    // mock.
    test_util::wait_for_atomic_usize(source_event_counter, |x| x == num_events);

    rt.block_on(topology.stop()).unwrap();
    shutdown_on_idle(rt);

    out_rx.close();
    let output_events = out_rx.collect().wait().unwrap();

    assert_eq!(expected_events_count, output_events.len());
    assert_eq!(input_events, &output_events[..num_events]);
    assert_eq!(input_events2, &output_events[num_events..]);
}

#[test]
fn test_max_size() {
    vector::test_util::trace_init();

    let data_dir = tempdir().unwrap();
    let data_dir = data_dir.path().to_path_buf();

    let num_lines: usize = 1000;
    let line_size = 1000;
    let input_lines = random_lines(line_size).take(num_lines).collect::<Vec<_>>();

    let max_size = input_lines
        .clone()
        .into_iter()
        .take(num_lines / 2)
        .map(|line| {
            let mut e = Event::from(line);
            e.as_mut_log().insert("host", "127.0.0.1");
            event::proto::EventWrapper::from(e)
        })
        .map(|ew| ew.encoded_len())
        .sum();

    let in_addr = next_addr();
    let out_addr = next_addr();

    // Run vector while sink server is not running, and then shut it down abruptly
    let mut config = config::Config::empty();
    config.add_source(
        "in",
        sources::socket::SocketConfig::make_tcp_config(in_addr),
    );
    config.add_sink(
        "out",
        &["in"],
        sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
    );
    config.sinks["out"].buffer = BufferConfig::Disk {
        max_size,
        when_full: Default::default(),
    };
    config.global.data_dir = Some(data_dir.clone());

    let mut rt = runtime::Runtime::new().unwrap();

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
    wait_for_tcp(in_addr);

    let send = send_lines(in_addr, input_lines.clone().into_iter());
    rt.block_on(send).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(100));

    rt.shutdown_now().wait().unwrap();
    drop(topology);

    // Start sink server, then run vector again. It should send the lines from the first run that fit in the limited space
    let mut config = config::Config::empty();
    config.add_source(
        "in",
        sources::socket::SocketConfig::make_tcp_config(in_addr),
    );
    config.add_sink(
        "out",
        &["in"],
        sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
    );
    config.sinks["out"].buffer = BufferConfig::Disk {
        max_size,
        when_full: Default::default(),
    };
    config.global.data_dir = Some(data_dir);

    let mut rt = runtime::Runtime::new().unwrap();

    let output_lines = receive(&out_addr);

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    wait_for_tcp(in_addr);

    rt.block_on(topology.stop()).unwrap();

    shutdown_on_idle(rt);

    let output_lines = output_lines.wait();
    assert_eq!(num_lines / 2, output_lines.len());
    assert_eq!(&input_lines[..num_lines / 2], &output_lines[..]);
}

#[test]
fn test_max_size_resume() {
    vector::test_util::trace_init();

    let data_dir = tempdir().unwrap();
    let data_dir = data_dir.path().to_path_buf();

    let num_lines: usize = 1000;
    let line_size = 1000;
    let max_size = num_lines * line_size / 2;

    let in_addr1 = next_addr();
    let in_addr2 = next_addr();
    let out_addr = next_addr();

    let mut config = config::Config::empty();
    config.add_source(
        "in1",
        sources::socket::SocketConfig::make_tcp_config(in_addr1),
    );
    config.add_source(
        "in2",
        sources::socket::SocketConfig::make_tcp_config(in_addr2),
    );
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

    let mut rt = runtime::Runtime::new().unwrap();

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
    wait_for_tcp(in_addr1);
    wait_for_tcp(in_addr2);

    // Send all of the input lines _before_ the output sink is ready. This causes the writers to stop
    // writing to the on-disk buffer, and once the output sink is available and the size of the buffer
    // begins to decrease, they should starting writing again.
    let input_lines1 = random_lines(line_size).take(num_lines).collect::<Vec<_>>();
    let send1 = send_lines(in_addr1, input_lines1.clone().into_iter());
    let input_lines2 = random_lines(line_size).take(num_lines).collect::<Vec<_>>();
    let send2 = send_lines(in_addr2, input_lines2.clone().into_iter());
    rt.block_on(send1.join(send2)).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(100));

    let output_lines = receive(&out_addr);

    rt.block_on(topology.stop()).unwrap();

    shutdown_on_idle(rt);

    let output_lines = output_lines.wait();
    assert_eq!(num_lines * 2, output_lines.len());
}

#[test]
#[ignore]
fn test_reclaim_disk_space() {
    vector::test_util::trace_init();

    let data_dir = tempdir().unwrap();
    let data_dir = data_dir.path().to_path_buf();

    let num_lines: usize = 10_000;
    let line_size = 1000;

    let in_addr = next_addr();
    let out_addr = next_addr();

    // Run vector while sink server is not running, and then shut it down abruptly
    let mut config = config::Config::empty();
    config.add_source(
        "in",
        sources::socket::SocketConfig::make_tcp_config(in_addr),
    );
    config.add_sink(
        "out",
        &["in"],
        sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
    );
    config.sinks["out"].buffer = BufferConfig::Disk {
        max_size: 1_000_000_000,
        when_full: Default::default(),
    }
    .into();
    config.global.data_dir = Some(data_dir.clone());

    let mut rt = runtime();

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
    wait_for_tcp(in_addr);

    let input_lines = random_lines(line_size).take(num_lines).collect::<Vec<_>>();
    let send = send_lines(in_addr, input_lines.clone().into_iter());
    rt.block_on(send).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(10000));

    rt.shutdown_now().wait().unwrap();
    drop(topology);

    let before_disk_size: u64 = walkdir::WalkDir::new(&data_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.metadata().ok())
        .filter(|metadata| metadata.is_file())
        .map(|m| m.len())
        .sum();

    let in_addr = next_addr();
    let out_addr = next_addr();

    // Start sink server, then run vector again. It should send all of the lines from the first run.
    let mut config = config::Config::empty();
    config.add_source(
        "in",
        sources::socket::SocketConfig::make_tcp_config(in_addr),
    );
    config.add_sink(
        "out",
        &["in"],
        sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
    );
    config.sinks["out"].buffer = BufferConfig::Disk {
        max_size: 1_000_000_000,
        when_full: Default::default(),
    };
    config.global.data_dir = Some(data_dir.clone());

    let mut rt = runtime();

    let output_lines = receive(&out_addr);

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    wait_for_tcp(in_addr);

    let input_lines2 = random_lines(line_size).take(num_lines).collect::<Vec<_>>();
    let send = send_lines(in_addr, input_lines2.clone().into_iter());
    rt.block_on(send).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(1000));

    block_on(topology.stop()).unwrap();

    shutdown_on_idle(rt);

    let output_lines = output_lines.wait();
    assert_eq!(num_lines * 2, output_lines.len());
    assert_eq!(&input_lines[..], &output_lines[..num_lines]);
    assert_eq!(&input_lines2[..], &output_lines[num_lines..]);

    let after_disk_size: u64 = walkdir::WalkDir::new(&data_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.metadata().ok())
        .filter(|metadata| metadata.is_file())
        .map(|m| m.len())
        .sum();

    // Ensure that the disk space after is less than half of the size that it
    // was before we reclaimed the space.
    assert!(after_disk_size < before_disk_size / 2);
}

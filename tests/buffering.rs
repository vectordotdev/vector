use futures::Future;
use router::test_util::{
    next_addr, random_lines, receive_lines, send_lines, shutdown_on_idle, wait_for_tcp,
};
use router::topology::{self, config};
use router::{buffers::BufferConfig, sinks, sources};
use tempfile::tempdir;

#[test]
fn test_buffering() {
    let data_dir = tempdir().unwrap();
    let data_dir = data_dir.path().to_path_buf();

    let num_lines: usize = 10;

    let in_addr = next_addr();
    let out_addr = next_addr();

    // Run router while sink server is not running, and then shut it down abruptly
    let mut topology = config::Config::empty();
    topology.add_source("in", sources::tcp::TcpConfig::new(in_addr));
    topology.add_sink(
        "out",
        &["in"],
        sinks::tcp::TcpSinkConfig { address: out_addr },
    );
    topology.sinks["out"].buffer = BufferConfig::Disk { max_size: 10_000 };
    topology.data_dir = Some(data_dir.clone());
    let (server, _trigger, _healthcheck, _warnings) = topology::build(topology).unwrap();

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    rt.spawn(server);
    wait_for_tcp(in_addr);

    let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
    let send = send_lines(in_addr, input_lines.clone().into_iter());
    rt.block_on(send).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(100));

    rt.shutdown_now().wait().unwrap();

    // Start sink server, then run router again. It should send all of the lines from the first run.
    let mut topology = config::Config::empty();
    topology.add_source("in", sources::tcp::TcpConfig::new(in_addr));
    topology.add_sink(
        "out",
        &["in"],
        sinks::tcp::TcpSinkConfig { address: out_addr },
    );
    topology.sinks["out"].buffer = BufferConfig::Disk { max_size: 10_000 };
    topology.data_dir = Some(data_dir);
    let (server, trigger, _healthcheck, _warnings) = topology::build(topology).unwrap();

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let output_lines = receive_lines(&out_addr, &rt.executor());

    rt.spawn(server);

    wait_for_tcp(in_addr);

    let input_lines2 = random_lines(100).take(num_lines).collect::<Vec<_>>();
    let send = send_lines(in_addr, input_lines2.clone().into_iter());
    rt.block_on(send).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(100));

    drop(trigger);

    shutdown_on_idle(rt);

    let output_lines = output_lines.wait().unwrap();
    assert_eq!(num_lines * 2 - 1, output_lines.len());
    assert_eq!(&input_lines[1..], &output_lines[..num_lines - 1]);
    assert_eq!(input_lines2, &output_lines[num_lines - 1..]);
}

#[test]
fn test_max_size() {
    let data_dir = tempdir().unwrap();
    let data_dir = data_dir.path().to_path_buf();

    let num_lines: usize = 1000;
    let line_size = 1000;
    let max_size = num_lines * (line_size + 24/* protobuf encoding takes a few extra bytes */) / 2;

    let in_addr = next_addr();
    let out_addr = next_addr();

    // Run router while sink server is not running, and then shut it down abruptly
    let mut topology = config::Config::empty();
    topology.add_source("in", sources::tcp::TcpConfig::new(in_addr));
    topology.add_sink(
        "out",
        &["in"],
        sinks::tcp::TcpSinkConfig { address: out_addr },
    );
    topology.sinks["out"].buffer = BufferConfig::Disk { max_size };
    topology.data_dir = Some(data_dir.clone());
    let (server, _trigger, _healthcheck, _warnings) = topology::build(topology).unwrap();

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    rt.spawn(server);
    wait_for_tcp(in_addr);

    let input_lines = random_lines(line_size).take(num_lines).collect::<Vec<_>>();
    let send = send_lines(in_addr, input_lines.clone().into_iter());
    rt.block_on(send).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(100));

    rt.shutdown_now().wait().unwrap();

    // Start sink server, then run router again. It should send the lines from the first run that fit in the limited space
    let mut topology = config::Config::empty();
    topology.add_source("in", sources::tcp::TcpConfig::new(in_addr));
    topology.add_sink(
        "out",
        &["in"],
        sinks::tcp::TcpSinkConfig { address: out_addr },
    );
    topology.sinks["out"].buffer = BufferConfig::Disk { max_size };
    topology.data_dir = Some(data_dir);
    let (server, trigger, _healthcheck, _warnings) = topology::build(topology).unwrap();

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let output_lines = receive_lines(&out_addr, &rt.executor());

    rt.spawn(server);

    wait_for_tcp(in_addr);

    drop(trigger);

    shutdown_on_idle(rt);

    let output_lines = output_lines.wait().unwrap();
    assert_eq!(num_lines / 2, output_lines.len());
    assert_eq!(&input_lines[1..num_lines / 2 + 1], &output_lines[..]);
}

#[test]
fn test_max_size_resume() {
    let data_dir = tempdir().unwrap();
    let data_dir = data_dir.path().to_path_buf();

    let num_lines: usize = 1000;
    let line_size = 1000;
    let max_size = num_lines * line_size / 2;

    let in_addr1 = next_addr();
    let in_addr2 = next_addr();
    let out_addr = next_addr();

    let mut topology = config::Config::empty();
    topology.add_source("in1", sources::tcp::TcpConfig::new(in_addr1));
    topology.add_source("in2", sources::tcp::TcpConfig::new(in_addr2));
    topology.add_sink(
        "out",
        &["in1", "in2"],
        sinks::tcp::TcpSinkConfig { address: out_addr },
    );
    topology.sinks["out"].buffer = BufferConfig::Disk { max_size };
    topology.data_dir = Some(data_dir.clone());
    let (server, trigger, _healthcheck, _warnings) = topology::build(topology).unwrap();

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    rt.spawn(server);
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

    let output_lines = receive_lines(&out_addr, &rt.executor());

    drop(trigger);

    shutdown_on_idle(rt);

    let output_lines = output_lines.wait().unwrap();
    assert_eq!(num_lines * 2, output_lines.len());
}

#[test]
fn test_reclaim_disk_space() {
    let data_dir = tempdir().unwrap();
    let data_dir = data_dir.path().to_path_buf();

    let num_lines: usize = 10_000;
    let line_size = 1000;

    let in_addr = next_addr();
    let out_addr = next_addr();

    // Run router while sink server is not running, and then shut it down abruptly
    let mut topology = config::Config::empty();
    topology.add_source("in", sources::tcp::TcpConfig::new(in_addr));
    topology.add_sink(
        "out",
        &["in"],
        sinks::tcp::TcpSinkConfig { address: out_addr },
    );
    topology.sinks["out"].buffer = BufferConfig::Disk {
        max_size: 1_000_000_000,
    };
    topology.data_dir = Some(data_dir.clone());
    let (server, _trigger, _healthcheck, _warnings) = topology::build(topology).unwrap();

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    rt.spawn(server);
    wait_for_tcp(in_addr);

    let input_lines = random_lines(line_size).take(num_lines).collect::<Vec<_>>();
    let send = send_lines(in_addr, input_lines.clone().into_iter());
    rt.block_on(send).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(500));

    rt.shutdown_now().wait().unwrap();

    let before_disk_size: u64 = walkdir::WalkDir::new(&data_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.metadata().ok())
        .filter(|metadata| metadata.is_file())
        .map(|m| m.len())
        .sum();

    // Start sink server, then run router again. It should send all of the lines from the first run.
    let mut topology = config::Config::empty();
    topology.add_source("in", sources::tcp::TcpConfig::new(in_addr));
    topology.add_sink(
        "out",
        &["in"],
        sinks::tcp::TcpSinkConfig { address: out_addr },
    );
    topology.sinks["out"].buffer = BufferConfig::Disk {
        max_size: 1_000_000_000,
    };
    topology.data_dir = Some(data_dir.clone());
    let (server, trigger, _healthcheck, _warnings) = topology::build(topology).unwrap();

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let output_lines = receive_lines(&out_addr, &rt.executor());

    rt.spawn(server);

    wait_for_tcp(in_addr);

    let input_lines2 = random_lines(line_size).take(num_lines).collect::<Vec<_>>();
    let send = send_lines(in_addr, input_lines2.clone().into_iter());
    rt.block_on(send).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(500));

    drop(trigger);

    shutdown_on_idle(rt);

    let output_lines = output_lines.wait().unwrap();
    assert_eq!(num_lines * 2 - 1, output_lines.len());
    assert_eq!(&input_lines[1..], &output_lines[..num_lines - 1]);
    assert_eq!(input_lines2, &output_lines[num_lines - 1..]);

    let after_disk_size: u64 = walkdir::WalkDir::new(&data_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.metadata().ok())
        .filter(|metadata| metadata.is_file())
        .map(|m| m.len())
        .sum();

    assert!(after_disk_size < before_disk_size);
}

use futures::Future;
use prost::Message;
use tempfile::tempdir;
use vector::event::{self, Event};
use vector::test_util::{
    block_on, next_addr, random_lines, receive, send_lines, shutdown_on_idle, wait_for_tcp,
};
use vector::topology::{self, config};
use vector::{buffers::BufferConfig, sinks, sources};

#[test]
fn test_buffering() {
    let data_dir = tempdir().unwrap();
    let data_dir = data_dir.path().to_path_buf();

    let num_lines: usize = 10;

    let in_addr = next_addr();
    let out_addr = next_addr();

    // Run vector while sink server is not running, and then shut it down abruptly
    let mut config = config::Config::empty();
    config.add_source("in", sources::tcp::TcpConfig::new(in_addr));
    config.add_sink(
        "out",
        &["in"],
        sinks::tcp::TcpSinkConfig::new(out_addr.to_string()),
    );
    config.sinks["out"].buffer = BufferConfig::Disk {
        max_size: 10_000,
        when_full: Default::default(),
    };
    config.data_dir = Some(data_dir.clone());

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
    wait_for_tcp(in_addr);

    let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
    let send = send_lines(in_addr, input_lines.clone().into_iter());
    rt.block_on(send).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(100));

    rt.shutdown_now().wait().unwrap();
    drop(topology);

    // Start sink server, then run vector again. It should send all of the lines from the first run.
    let mut config = config::Config::empty();
    config.add_source("in", sources::tcp::TcpConfig::new(in_addr));
    config.add_sink(
        "out",
        &["in"],
        sinks::tcp::TcpSinkConfig::new(out_addr.to_string()),
    );
    config.sinks["out"].buffer = BufferConfig::Disk {
        max_size: 10_000,
        when_full: Default::default(),
    };
    config.data_dir = Some(data_dir);

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let output_lines = receive(&out_addr);

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    wait_for_tcp(in_addr);

    let input_lines2 = random_lines(100).take(num_lines).collect::<Vec<_>>();
    let send = send_lines(in_addr, input_lines2.clone().into_iter());
    rt.block_on(send).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(100));

    block_on(topology.stop()).unwrap();

    shutdown_on_idle(rt);

    let output_lines = output_lines.wait();
    assert_eq!(num_lines * 2, output_lines.len());
    assert_eq!(input_lines, &output_lines[..num_lines]);
    assert_eq!(input_lines2, &output_lines[num_lines..]);
}

#[test]
fn test_max_size() {
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
            e.as_mut_log()
                .insert_implicit("host".into(), "127.0.0.1".into());
            event::proto::EventWrapper::from(e)
        })
        .map(|ew| ew.encoded_len())
        .sum();

    let in_addr = next_addr();
    let out_addr = next_addr();

    // Run vector while sink server is not running, and then shut it down abruptly
    let mut config = config::Config::empty();
    config.add_source("in", sources::tcp::TcpConfig::new(in_addr));
    config.add_sink(
        "out",
        &["in"],
        sinks::tcp::TcpSinkConfig::new(out_addr.to_string()),
    );
    config.sinks["out"].buffer = BufferConfig::Disk {
        max_size,
        when_full: Default::default(),
    };
    config.data_dir = Some(data_dir.clone());

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
    wait_for_tcp(in_addr);

    let send = send_lines(in_addr, input_lines.clone().into_iter());
    rt.block_on(send).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(100));

    rt.shutdown_now().wait().unwrap();
    drop(topology);

    // Start sink server, then run vector again. It should send the lines from the first run that fit in the limited space
    let mut config = config::Config::empty();
    config.add_source("in", sources::tcp::TcpConfig::new(in_addr));
    config.add_sink(
        "out",
        &["in"],
        sinks::tcp::TcpSinkConfig::new(out_addr.to_string()),
    );
    config.sinks["out"].buffer = BufferConfig::Disk {
        max_size,
        when_full: Default::default(),
    };
    config.data_dir = Some(data_dir);

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let output_lines = receive(&out_addr);

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();

    wait_for_tcp(in_addr);

    block_on(topology.stop()).unwrap();

    shutdown_on_idle(rt);

    let output_lines = output_lines.wait();
    assert_eq!(num_lines / 2, output_lines.len());
    assert_eq!(&input_lines[..num_lines / 2], &output_lines[..]);
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

    let mut config = config::Config::empty();
    config.add_source("in1", sources::tcp::TcpConfig::new(in_addr1));
    config.add_source("in2", sources::tcp::TcpConfig::new(in_addr2));
    config.add_sink(
        "out",
        &["in1", "in2"],
        sinks::tcp::TcpSinkConfig::new(out_addr.to_string()),
    );
    config.sinks["out"].buffer = BufferConfig::Disk {
        max_size,
        when_full: Default::default(),
    };
    config.data_dir = Some(data_dir.clone());

    let mut rt = tokio::runtime::Runtime::new().unwrap();

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

    block_on(topology.stop()).unwrap();

    shutdown_on_idle(rt);

    let output_lines = output_lines.wait();
    assert_eq!(num_lines * 2, output_lines.len());
}

#[test]
#[ignore]
fn test_reclaim_disk_space() {
    let data_dir = tempdir().unwrap();
    let data_dir = data_dir.path().to_path_buf();

    let num_lines: usize = 10_000;
    let line_size = 1000;

    let in_addr = next_addr();
    let out_addr = next_addr();

    // Run vector while sink server is not running, and then shut it down abruptly
    let mut config = config::Config::empty();
    config.add_source("in", sources::tcp::TcpConfig::new(in_addr));
    config.add_sink(
        "out",
        &["in"],
        sinks::tcp::TcpSinkConfig::new(out_addr.to_string()),
    );
    config.sinks["out"].buffer = BufferConfig::Disk {
        max_size: 1_000_000_000,
        when_full: Default::default(),
    }
    .into();
    config.data_dir = Some(data_dir.clone());

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();
    wait_for_tcp(in_addr);

    let input_lines = random_lines(line_size).take(num_lines).collect::<Vec<_>>();
    let send = send_lines(in_addr, input_lines.clone().into_iter());
    rt.block_on(send).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(500));

    rt.shutdown_now().wait().unwrap();
    drop(topology);

    let before_disk_size: u64 = walkdir::WalkDir::new(&data_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.metadata().ok())
        .filter(|metadata| metadata.is_file())
        .map(|m| m.len())
        .sum();

    // Start sink server, then run vector again. It should send all of the lines from the first run.
    let mut config = config::Config::empty();
    config.add_source("in", sources::tcp::TcpConfig::new(in_addr));
    config.add_sink(
        "out",
        &["in"],
        sinks::tcp::TcpSinkConfig::new(out_addr.to_string()),
    );
    config.sinks["out"].buffer = BufferConfig::Disk {
        max_size: 1_000_000_000,
        when_full: Default::default(),
    };
    config.data_dir = Some(data_dir.clone());

    let mut rt = tokio::runtime::Runtime::new().unwrap();

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

    println!("after {}, before {}", after_disk_size, before_disk_size);
    assert!(after_disk_size < before_disk_size);
}

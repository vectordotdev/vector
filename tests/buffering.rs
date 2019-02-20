use futures::{Future, Stream};
use router::test_util::{next_addr, random_lines, send_lines};
use router::topology::{self, config};
use router::{buffers::BufferConfig, sinks, sources};
use std::net::SocketAddr;
use tempfile::tempdir;
use tokio::codec::{FramedRead, LinesCodec};
use tokio::net::TcpListener;

#[test]
fn test_buffering() {
    let data_dir = tempdir().unwrap().into_path();

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
    topology.sinks["out"].buffer = BufferConfig::Disk {};
    topology.data_dir = Some(data_dir.clone());
    let (server, _trigger, _healthcheck, _warnings) = topology::build(topology).unwrap();

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    rt.spawn(server);
    while let Err(_) = std::net::TcpStream::connect(in_addr) {}

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
    topology.sinks["out"].buffer = BufferConfig::Disk {};
    topology.data_dir = Some(data_dir);
    let (server, trigger, _healthcheck, _warnings) = topology::build(topology).unwrap();

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let output_lines = receive_lines(&out_addr, &rt.executor());

    rt.spawn(server);

    while let Err(_) = std::net::TcpStream::connect(in_addr) {}

    let input_lines2 = random_lines(100).take(num_lines).collect::<Vec<_>>();
    let send = send_lines(in_addr, input_lines2.clone().into_iter());
    rt.block_on(send).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(100));

    drop(trigger);

    rt.shutdown_on_idle().wait().unwrap();

    let output_lines = output_lines.wait().unwrap();
    assert_eq!(num_lines * 2 - 1, output_lines.len());
    assert_eq!(&input_lines[1..], &output_lines[..num_lines - 1]);
    assert_eq!(input_lines2, &output_lines[num_lines - 1..]);
}

fn receive_lines(
    addr: &SocketAddr,
    executor: &tokio::runtime::TaskExecutor,
) -> impl Future<Item = Vec<String>, Error = ()> {
    let listener = TcpListener::bind(addr).unwrap();

    let lines = listener
        .incoming()
        .take(1)
        .map(|socket| FramedRead::new(socket, LinesCodec::new()))
        .flatten()
        .map_err(|e| panic!("{:?}", e))
        .collect();

    futures::sync::oneshot::spawn(lines, executor)
}

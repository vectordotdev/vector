extern crate futures;
extern crate rand;
extern crate regex;
extern crate router;
extern crate stream_cancel;
extern crate tokio;
#[macro_use]
extern crate approx;

use futures::{Future, Sink, Stream};
use regex::RegexSet;
use router::{sinks, sources, transforms};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use stream_cancel::Tripwire;
use tokio::codec::{BytesCodec, FramedRead, FramedWrite, LinesCodec};
use tokio::net::{TcpListener, TcpStream};

static NEXT_PORT: AtomicUsize = AtomicUsize::new(1234);
fn next_addr() -> SocketAddr {
    use std::net::{IpAddr, Ipv4Addr};

    let port = NEXT_PORT.fetch_add(1, Ordering::AcqRel) as u16;
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port)
}

#[test]
fn test_pipe() {
    let (trigger, tripwire) = Tripwire::new();

    let num_lines: usize = 10000;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let splunk_in = sources::splunk::raw_tcp(in_addr, tripwire);
    let splunk_out = sinks::splunk::raw_tcp(out_addr)
        .map(|sink| sink.sink_map_err(|e| panic!("tcp sink error: {:?}", e)))
        .map_err(|e| panic!("error creating tcp sink: {:?}", e));
    let server = splunk_out.and_then(|sink| splunk_in.forward(sink).map(|_| ()));

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let output_lines = receive_lines(&out_addr, &rt.executor());

    rt.spawn(server);
    // Wait for server to accept traffic
    while let Err(_) = std::net::TcpStream::connect(in_addr) {}

    let input_lines = random_lines().take(num_lines).collect::<Vec<_>>();
    let send = send_lines(in_addr, input_lines.clone().into_iter());
    rt.block_on(send).unwrap();

    // Shut down server
    drop(trigger);

    rt.shutdown_on_idle().wait().unwrap();
    let output_lines = output_lines.wait().unwrap();
    assert_eq!(num_lines, output_lines.len());
    assert_eq!(input_lines, output_lines);
}

#[test]
fn test_sample() {
    let (trigger, tripwire) = Tripwire::new();

    let num_lines: usize = 10000;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let splunk_in = sources::splunk::raw_tcp(in_addr, tripwire);
    let splunk_out = sinks::splunk::raw_tcp(out_addr)
        .map(|sink| sink.sink_map_err(|e| panic!("tcp sink error: {:?}", e)))
        .map_err(|e| panic!("error creating tcp sink: {:?}", e));
    let empty: &[&str] = &[];
    let sampler = transforms::Sampler::new(10, RegexSet::new(empty).unwrap());
    let server = splunk_out.and_then(|sink| {
        splunk_in
            .filter(move |r| sampler.filter(r))
            .forward(sink)
            .map(|_| ())
    });

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let output_lines = receive_lines(&out_addr, &rt.executor());

    rt.spawn(server);
    // Wait for server to accept traffic
    while let Err(_) = std::net::TcpStream::connect(in_addr) {}

    let input_lines = random_lines().take(num_lines).collect::<Vec<_>>();
    let send = send_lines(in_addr, input_lines.clone().into_iter());
    rt.block_on(send).unwrap();

    // Shut down server
    drop(trigger);

    rt.shutdown_on_idle().wait().unwrap();
    let output_lines = output_lines.wait().unwrap();
    let num_output_lines = output_lines.len();

    let output_lines_ratio = num_output_lines as f32 / num_lines as f32;
    assert_relative_eq!(output_lines_ratio, 0.1, epsilon = 0.01);

    let mut input_lines = input_lines.into_iter();
    // Assert that all of the output lines were present in the input and in the same order
    for output_line in output_lines {
        let next_line = input_lines
            .by_ref()
            .skip_while(|l| l != &output_line)
            .next();
        assert_eq!(Some(output_line), next_line);
    }
}

#[test]
fn test_merge() {
    let (trigger, tripwire) = Tripwire::new();

    let num_lines: usize = 10000;

    let in_addr1 = next_addr();
    let in_addr2 = next_addr();
    let out_addr = next_addr();

    let splunk_in1 = sources::splunk::raw_tcp(in_addr1, tripwire.clone());
    let splunk_in2 = sources::splunk::raw_tcp(in_addr2, tripwire.clone());
    let splunk_out = sinks::splunk::raw_tcp(out_addr)
        .map(|sink| sink.sink_map_err(|e| panic!("tcp sink error: {:?}", e)))
        .map_err(|e| panic!("error creating tcp sink: {:?}", e));
    let server =
        splunk_out.and_then(|sink| splunk_in1.select(splunk_in2).forward(sink).map(|_| ()));

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let output_lines = receive_lines(&out_addr, &rt.executor());

    rt.spawn(server);
    // Wait for server to accept traffic
    while let Err(_) = std::net::TcpStream::connect(in_addr1) {}
    while let Err(_) = std::net::TcpStream::connect(in_addr2) {}

    let input_lines1 = random_lines().take(num_lines).collect::<Vec<_>>();
    let input_lines2 = random_lines().take(num_lines).collect::<Vec<_>>();
    let send1 = send_lines(in_addr1, input_lines1.clone().into_iter());
    let send2 = send_lines(in_addr2, input_lines2.clone().into_iter());
    let send = send1.join(send2);
    rt.block_on(send).unwrap();

    // Shut down server
    drop(trigger);

    rt.shutdown_on_idle().wait().unwrap();
    let output_lines = output_lines.wait().unwrap();
    let num_output_lines = output_lines.len();

    assert_eq!(num_output_lines, num_lines * 2);

    let mut input_lines1 = input_lines1.into_iter().peekable();
    let mut input_lines2 = input_lines2.into_iter().peekable();
    // Assert that all of the output lines were present in the input and in the same order
    for output_line in &output_lines {
        if Some(output_line) == input_lines1.peek() {
            input_lines1.next();
        } else if Some(output_line) == input_lines2.peek() {
            input_lines2.next();
        } else {
            panic!("Got line in output that wasn't in input");
        }
    }
    assert_eq!(input_lines1.next(), None);
    assert_eq!(input_lines2.next(), None);
}

#[test]
fn test_fork() {
    let (trigger, tripwire) = Tripwire::new();

    let num_lines: usize = 10000;

    let in_addr = next_addr();
    let out_addr1 = next_addr();
    let out_addr2 = next_addr();

    let splunk_in = sources::splunk::raw_tcp(in_addr, tripwire);
    let splunk_out1 = sinks::splunk::raw_tcp(out_addr1)
        .map(|sink| sink.sink_map_err(|e| panic!("tcp sink error: {:?}", e)))
        .map_err(|e| panic!("error creating tcp sink: {:?}", e));
    let splunk_out2 = sinks::splunk::raw_tcp(out_addr2)
        .map(|sink| sink.sink_map_err(|e| panic!("tcp sink error: {:?}", e)))
        .map_err(|e| panic!("error creating tcp sink: {:?}", e));
    let server = splunk_out1
        .join(splunk_out2)
        .and_then(|(sink1, sink2)| splunk_in.forward(sink1.fanout(sink2)).map(|_| ()));

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let output_lines1 = receive_lines(&out_addr1, &rt.executor());
    let output_lines2 = receive_lines(&out_addr2, &rt.executor());

    rt.spawn(server);
    // Wait for server to accept traffic
    while let Err(_) = std::net::TcpStream::connect(in_addr) {}

    let input_lines = random_lines().take(num_lines).collect::<Vec<_>>();
    let send = send_lines(in_addr, input_lines.clone().into_iter());
    rt.block_on(send).unwrap();

    // Shut down server
    drop(trigger);

    rt.shutdown_on_idle().wait().unwrap();
    let output_lines1 = output_lines1.wait().unwrap();
    let output_lines2 = output_lines2.wait().unwrap();
    assert_eq!(num_lines, output_lines1.len());
    assert_eq!(num_lines, output_lines2.len());
    assert_eq!(input_lines, output_lines1);
    assert_eq!(input_lines, output_lines2);
}

fn random_lines() -> impl Iterator<Item = String> {
    use rand::distributions::Alphanumeric;
    use rand::{rngs::SmallRng, thread_rng, Rng, SeedableRng};

    let mut rng = SmallRng::from_rng(thread_rng()).unwrap();

    std::iter::repeat(()).map(move |_| rng.sample_iter(&Alphanumeric).take(100).collect::<String>())
}

fn send_lines(
    addr: SocketAddr,
    lines: impl Iterator<Item = String>,
) -> impl Future<Item = (), Error = ()> {
    let lines = futures::stream::iter_ok::<_, ()>(lines);

    TcpStream::connect(&addr)
        .map_err(|e| panic!("{:}", e))
        .and_then(|socket| {
            let out =
                FramedWrite::new(socket, LinesCodec::new()).sink_map_err(|e| panic!("{:?}", e));

            lines
                .forward(out)
                .map(|(_source, sink)| sink)
                .and_then(|sink| {
                    // This waits for FIN from the server so we don't start shutting it down before it's fully received the test data
                    let socket = sink.into_inner().into_inner();
                    socket.shutdown(std::net::Shutdown::Write).unwrap();
                    FramedRead::new(socket, BytesCodec::new())
                        .for_each(|_| Ok(()))
                        .map_err(|e| panic!("{:}", e))
                })
        })
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

use crate::runtime::Runtime;
use crate::{event::LogEvent, Event};

use futures01::{future, stream, sync::mpsc, try_ready, Async, Future, Poll, Sink, Stream};
use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::iter;
use std::mem;
use std::net::{Shutdown, SocketAddr};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use stream_cancel::{StreamExt, Trigger, Tripwire};
use tokio01::codec::{FramedRead, FramedWrite, LinesCodec};
use tokio01::net::{TcpListener, TcpStream};
use tokio01::util::FutureExt;
use tokio_openssl::SslConnectorExt;

#[macro_export]
macro_rules! assert_downcast_matches {
    ($e:expr, $t:ty, $v:pat) => {{
        match $e.downcast_ref::<$t>() {
            Some($v) => (),
            got => panic!("assertion failed: got wrong error variant {:?}", got),
        }
    }};
}

static NEXT_PORT: AtomicUsize = AtomicUsize::new(1234);

pub fn next_addr() -> SocketAddr {
    use std::net::{IpAddr, Ipv4Addr};

    let port = NEXT_PORT.fetch_add(1, Ordering::AcqRel) as u16;
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port)
}

pub fn trace_init() {
    let env = std::env::var("TEST_LOG").unwrap_or_else(|_| "off".to_string());

    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter(env)
        .finish();

    let _ = tracing_log::LogTracer::init();
    let _ = tracing::dispatcher::set_global_default(tracing::Dispatch::new(subscriber));
}

pub fn send_lines(
    addr: SocketAddr,
    lines: impl Iterator<Item = String>,
) -> impl Future<Item = (), Error = ()> {
    let lines = futures01::stream::iter_ok::<_, ()>(lines);

    TcpStream::connect(&addr)
        .map_err(|e| panic!("{:}", e))
        .and_then(|socket| {
            let out =
                FramedWrite::new(socket, LinesCodec::new()).sink_map_err(|e| panic!("{:?}", e));

            lines
                .forward(out)
                .and_then(|(_source, sink)| {
                    let socket = sink.into_inner().into_inner();
                    // In tokio 0.1 `AsyncWrite::shutdown` for `TcpStream` is a noop.
                    // See https://docs.rs/tokio-tcp/0.1.4/src/tokio_tcp/stream.rs.html#917
                    // Use `TcpStream::shutdown` instead - it actually does something.
                    socket
                        .shutdown(Shutdown::Both)
                        .map_err(|e| panic!("{:}", e))
                })
                .map(|_| ())
        })
}

pub fn send_lines_tls(
    addr: SocketAddr,
    host: String,
    lines: impl Iterator<Item = String>,
) -> impl Future<Item = (), Error = ()> {
    let lines = futures01::stream::iter_ok::<_, ()>(lines);

    let mut connector =
        SslConnector::builder(SslMethod::tls()).expect("Failed to create TLS builder");
    connector.set_verify(SslVerifyMode::NONE);
    let connector = connector.build();

    TcpStream::connect(&addr)
        .map_err(|e| panic!("{:}", e))
        .and_then(move |socket| {
            connector
                .connect_async(&host, socket)
                .map_err(|e| panic!("{:}", e))
                .and_then(|stream| {
                    let out = FramedWrite::new(stream, LinesCodec::new())
                        .sink_map_err(|e| panic!("{:?}", e));

                    lines
                        .forward(out)
                        .and_then(|(_source, sink)| {
                            let mut stream = sink.into_inner().into_inner();
                            // We should catch TLS shutdown errors here,
                            // but doing so results in a repeatable
                            // "Resource temporarily available" error,
                            // and tests will be checking that contents
                            // are received anyways.
                            stream.get_mut().shutdown().ok();
                            Ok(())
                        })
                        .map(|_| ())
                })
        })
}

pub fn temp_file() -> std::path::PathBuf {
    let path = std::env::temp_dir();
    let file_name = random_string(16);
    path.join(file_name + ".log")
}

pub fn temp_dir() -> std::path::PathBuf {
    let path = std::env::temp_dir();
    let dir_name = random_string(16);
    path.join(dir_name)
}

pub fn random_lines_with_stream(
    len: usize,
    count: usize,
) -> (Vec<String>, impl Stream<Item = Event, Error = ()>) {
    let lines = (0..count).map(|_| random_string(len)).collect::<Vec<_>>();
    let stream = stream::iter_ok(lines.clone().into_iter().map(Event::from));
    (lines, stream)
}

pub fn random_events_with_stream(
    len: usize,
    count: usize,
) -> (Vec<Event>, impl Stream<Item = Event, Error = ()>) {
    random_events_with_stream_generic(count, move || Event::from(random_string(len)))
}

pub fn random_nested_events_with_stream(
    len: usize,
    breadth: usize,
    depth: usize,
    count: usize,
) -> (Vec<Event>, impl Stream<Item = Event, Error = ()>) {
    random_events_with_stream_generic(count, move || {
        let mut log = LogEvent::new();

        let tree = random_pseudonested_map(len, breadth, depth);
        for (k, v) in tree.into_iter() {
            log.insert(k, v);
        }

        Event::Log(log)
    })
}

pub fn random_string(len: usize) -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .collect::<String>()
}

pub fn random_lines(len: usize) -> impl Iterator<Item = String> {
    std::iter::repeat(()).map(move |_| random_string(len))
}

pub fn random_map(max_size: usize, field_len: usize) -> HashMap<String, String> {
    let size = thread_rng().gen_range(0, max_size);

    (0..size)
        .map(move |_| (random_string(field_len), random_string(field_len)))
        .collect()
}

pub fn random_maps(
    max_size: usize,
    field_len: usize,
) -> impl Iterator<Item = HashMap<String, String>> {
    iter::repeat(()).map(move |_| random_map(max_size, field_len))
}

pub fn collect_n<T>(mut rx: mpsc::Receiver<T>, n: usize) -> impl Future<Item = Vec<T>, Error = ()> {
    let mut events = Vec::new();

    future::poll_fn(move || {
        while events.len() < n {
            let e = try_ready!(rx.poll()).unwrap();
            events.push(e);
        }
        Ok(Async::Ready(mem::replace(&mut events, Vec::new())))
    })
}

pub fn lines_from_file<P: AsRef<Path>>(path: P) -> Vec<String> {
    trace!(message = "Reading file.", path = %path.as_ref().display());
    let mut file = File::open(path).unwrap();
    let mut output = String::new();
    file.read_to_string(&mut output).unwrap();
    output.lines().map(|s| s.to_owned()).collect()
}

pub fn wait_for(mut f: impl FnMut() -> bool) {
    let wait = std::time::Duration::from_millis(5);
    let limit = std::time::Duration::from_secs(5);
    let mut attempts = 0;
    while !f() {
        std::thread::sleep(wait);
        attempts += 1;
        if attempts * wait > limit {
            panic!("timed out while waiting");
        }
    }
}

pub fn block_on<F, R, E>(future: F) -> Result<R, E>
where
    F: Send + 'static + Future<Item = R, Error = E>,
    R: Send + 'static,
    E: Send + 'static,
{
    let mut rt = runtime();

    rt.block_on(future)
}

pub fn runtime() -> Runtime {
    Runtime::single_threaded().unwrap()
}

pub fn wait_for_tcp(addr: SocketAddr) {
    wait_for(|| std::net::TcpStream::connect(addr).is_ok())
}

pub fn wait_for_atomic_usize<T, F>(val: T, unblock: F)
where
    T: AsRef<AtomicUsize>,
    F: Fn(usize) -> bool,
{
    let val = val.as_ref();
    wait_for(|| unblock(val.load(Ordering::SeqCst)))
}

pub fn shutdown_on_idle(runtime: Runtime) {
    block_on(
        runtime
            .shutdown_on_idle()
            .timeout(std::time::Duration::from_secs(10)),
    )
    .unwrap()
}

#[derive(Debug)]
pub struct CollectN<S>
where
    S: Stream,
{
    stream: Option<S>,
    remaining: usize,
    items: Option<Vec<S::Item>>,
}

impl<S: Stream> CollectN<S> {
    pub fn new(s: S, n: usize) -> Self {
        Self {
            stream: Some(s),
            remaining: n,
            items: Some(Vec::new()),
        }
    }
}

impl<S> Future for CollectN<S>
where
    S: Stream,
{
    type Item = (S, Vec<S::Item>);
    type Error = S::Error;

    fn poll(&mut self) -> Poll<(S, Vec<S::Item>), S::Error> {
        let stream = self.stream.take();
        if stream.is_none() {
            panic!("Stream is missing");
        }
        let mut stream = stream.unwrap();

        loop {
            if self.remaining == 0 {
                return Ok(Async::Ready((stream, self.items.take().unwrap())));
            }
            match stream.poll() {
                Ok(Async::Ready(Some(e))) => {
                    self.items.as_mut().unwrap().push(e);
                    self.remaining -= 1;
                }
                Ok(Async::Ready(None)) => {
                    return Ok(Async::Ready((stream, self.items.take().unwrap())));
                }
                Ok(Async::NotReady) => {
                    self.stream.replace(stream);
                    return Ok(Async::NotReady);
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct CollectCurrent<S>
where
    S: Stream,
{
    stream: Option<S>,
}

impl<S: Stream> CollectCurrent<S> {
    pub fn new(s: S) -> Self {
        Self { stream: Some(s) }
    }
}

impl<S> Future for CollectCurrent<S>
where
    S: Stream,
{
    type Item = (S, Vec<S::Item>);
    type Error = S::Error;

    fn poll(&mut self) -> Poll<(S, Vec<S::Item>), S::Error> {
        if let Some(mut stream) = self.stream.take() {
            let mut items = vec![];

            loop {
                match stream.poll() {
                    Ok(Async::Ready(Some(e))) => items.push(e),
                    Ok(Async::Ready(None)) | Ok(Async::NotReady) => {
                        return Ok(Async::Ready((stream, items)));
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
        } else {
            panic!("Future already completed");
        }
    }
}

pub struct Receiver<T> {
    handle: futures01::sync::oneshot::SpawnHandle<Vec<T>, ()>,
    count: Arc<AtomicUsize>,
    trigger: Trigger,
    _runtime: Runtime,
}

impl<T> Receiver<T> {
    pub fn count(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }

    pub fn wait(self) -> Vec<T> {
        self.trigger.cancel();
        self.handle.wait().unwrap()
    }
}

pub fn receive(addr: &SocketAddr) -> Receiver<String> {
    let runtime = runtime();

    let listener = TcpListener::bind(addr).unwrap();

    let count = Arc::new(AtomicUsize::new(0));
    let count_clone = Arc::clone(&count);

    let (trigger, tripwire) = Tripwire::new();

    let lines = listener
        .incoming()
        .take_until(tripwire)
        .map(|socket| FramedRead::new(socket, LinesCodec::new()))
        .flatten()
        .inspect(move |_| {
            count_clone.fetch_add(1, Ordering::Relaxed);
        })
        .map_err(|e| panic!("{:?}", e))
        .collect();

    let handle = futures01::sync::oneshot::spawn(lines, &runtime.executor());
    Receiver {
        handle,
        count,
        trigger,
        _runtime: runtime,
    }
}

pub fn receive_events<S>(stream: S) -> Receiver<Event>
where
    S: Stream<Item = Event> + Send + 'static,
    <S as Stream>::Error: std::fmt::Debug,
{
    let runtime = runtime();

    let count = Arc::new(AtomicUsize::new(0));
    let count_clone = Arc::clone(&count);

    let (trigger, tripwire) = Tripwire::new();

    let events = stream
        .take_until(tripwire)
        .inspect(move |_| {
            count_clone.fetch_add(1, Ordering::Relaxed);
        })
        .map_err(|e| panic!("{:?}", e))
        .collect();

    let handle = futures01::sync::oneshot::spawn(events, &runtime.executor());
    Receiver {
        handle,
        count,
        trigger,
        _runtime: runtime,
    }
}

pub struct CountReceiver {
    handle: futures01::sync::oneshot::SpawnHandle<usize, ()>,
    trigger: Trigger,
    _runtime: Runtime,
}

impl CountReceiver {
    pub fn cancel(self) {
        self.trigger.cancel();
    }

    pub fn wait(self) -> usize {
        self.handle.wait().unwrap()
    }
}

pub fn count_receive(addr: &SocketAddr) -> CountReceiver {
    let runtime = Runtime::new().unwrap();

    let listener = TcpListener::bind(addr).unwrap();

    let (trigger, tripwire) = Tripwire::new();

    let count = listener
        .incoming()
        .take_until(tripwire)
        .map(|socket| FramedRead::new(socket, LinesCodec::new()))
        .flatten()
        .map_err(|e| panic!("{:?}", e))
        .fold(0, |n, _| future::ok(n + 1));

    let handle = futures01::sync::oneshot::spawn(count, &runtime.executor());
    CountReceiver {
        handle,
        trigger,
        _runtime: runtime,
    }
}

fn random_events_with_stream_generic<F>(
    count: usize,
    generator: F,
) -> (Vec<Event>, impl Stream<Item = Event, Error = ()>)
where
    F: Fn() -> Event,
{
    let events = (0..count).map(|_| generator()).collect::<Vec<_>>();
    let stream = stream::iter_ok(events.clone().into_iter());
    (events, stream)
}

fn random_pseudonested_map(len: usize, breadth: usize, depth: usize) -> HashMap<String, String> {
    if breadth == 0 || depth == 0 {
        return HashMap::new();
    }

    if depth == 1 {
        let mut leaf = HashMap::new();
        leaf.insert(random_string(len), random_string(len));
        return leaf;
    }

    let mut tree = HashMap::new();
    for _ in 0..breadth {
        let prefix = random_string(len);
        let subtree = random_pseudonested_map(len, breadth, depth - 1);

        let subtree: HashMap<String, String> = subtree
            .into_iter()
            .map(|(mut key, value)| {
                key.insert(0, '.');
                key.insert_str(0, &prefix[..]);
                (key, value)
            })
            .collect();

        for (key, value) in subtree.into_iter() {
            tree.insert(key, value);
        }
    }
    tree
}

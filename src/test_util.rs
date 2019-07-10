use crate::Event;
use futures::{future, stream, sync::mpsc, try_ready, Async, Future, Poll, Sink, Stream};
use std::fs::File;
use std::io::Read;
use std::mem;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use stream_cancel::{StreamExt, Trigger, Tripwire};
use tokio::codec::{FramedRead, FramedWrite, LinesCodec};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::{Builder, Runtime};
use tokio::util::FutureExt;

static NEXT_PORT: AtomicUsize = AtomicUsize::new(1234);
pub fn next_addr() -> SocketAddr {
    use std::net::{IpAddr, Ipv4Addr};

    let port = NEXT_PORT.fetch_add(1, Ordering::AcqRel) as u16;
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port)
}

pub fn trace_init() {
    let env = std::env::var("TEST_LOG").unwrap_or_else(|_| "off".to_string());

    let subscriber = tracing_fmt::FmtSubscriber::builder()
        .with_filter(tracing_fmt::filter::EnvFilter::from(env))
        .finish();
    let _ = tracing::dispatcher::set_global_default(tracing::Dispatch::new(subscriber));
}

pub fn send_lines(
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
                .and_then(|(_source, sink)| {
                    let socket = sink.into_inner().into_inner();
                    tokio::io::shutdown(socket).map_err(|e| panic!("{:}", e))
                })
                .map(|_| ())
        })
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
    let events = (0..count)
        .map(|_| random_string(len))
        .map(Event::from)
        .collect::<Vec<_>>();
    let stream = stream::iter_ok(events.clone().into_iter());
    (events, stream)
}

pub fn random_string(len: usize) -> String {
    use rand::distributions::Alphanumeric;
    use rand::{thread_rng, Rng};

    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .collect::<String>()
}

pub fn random_lines(len: usize) -> impl Iterator<Item = String> {
    std::iter::repeat(()).map(move |_| random_string(len))
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
    let mut file = File::open(path).unwrap();
    let mut output = String::new();
    file.read_to_string(&mut output).unwrap();
    output.lines().map(|s| s.to_owned()).collect()
}

pub fn wait_for(f: impl Fn() -> bool) {
    let wait = std::time::Duration::from_millis(5);
    let limit = std::time::Duration::from_secs(5);
    let mut attempts = 0;
    while !f() {
        std::thread::sleep(wait.clone());
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
    Builder::new().core_threads(1).build().unwrap()
}

pub fn wait_for_tcp(addr: SocketAddr) {
    wait_for(|| std::net::TcpStream::connect(addr).is_ok())
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

pub struct Receiver {
    handle: futures::sync::oneshot::SpawnHandle<Vec<String>, ()>,
    count: Arc<AtomicUsize>,
    trigger: Trigger,
    _runtime: Runtime,
}

impl Receiver {
    pub fn count(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }

    pub fn wait(self) -> Vec<String> {
        self.trigger.cancel();
        self.handle.wait().unwrap()
    }
}

pub fn receive(addr: &SocketAddr) -> Receiver {
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

    let handle = futures::sync::oneshot::spawn(lines, &runtime.executor());
    Receiver {
        handle,
        count,
        trigger,
        _runtime: runtime,
    }
}

pub struct CountReceiver {
    handle: futures::sync::oneshot::SpawnHandle<usize, ()>,
    trigger: Trigger,
    _runtime: Runtime,
}

impl CountReceiver {
    pub fn wait(self) -> usize {
        self.trigger.cancel();
        self.handle.wait().unwrap()
    }
}

pub fn count_receive(addr: &SocketAddr) -> CountReceiver {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let listener = TcpListener::bind(addr).unwrap();

    let (trigger, tripwire) = Tripwire::new();

    let count = listener
        .incoming()
        .take_until(tripwire)
        .map(|socket| FramedRead::new(socket, LinesCodec::new()))
        .flatten()
        .map_err(|e| panic!("{:?}", e))
        .fold(0, |n, _| future::ok(n + 1));

    let handle = futures::sync::oneshot::spawn(count, &runtime.executor());
    CountReceiver {
        handle,
        trigger,
        _runtime: runtime,
    }
}

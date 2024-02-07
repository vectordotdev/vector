#![allow(missing_docs)]
use std::{
    collections::HashMap,
    convert::Infallible,
    fs::File,
    future::{ready, Future},
    io::Read,
    iter,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{ready, Context, Poll},
};

use chrono::{SubsecRound, Utc};
use flate2::read::MultiGzDecoder;
use futures::{stream, task::noop_waker_ref, FutureExt, SinkExt, Stream, StreamExt, TryStreamExt};
use openssl::ssl::{SslConnector, SslFiletype, SslMethod, SslVerifyMode};
use portpicker::pick_unused_port;
use rand::{thread_rng, Rng};
use rand_distr::Alphanumeric;
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt, Result as IoResult},
    net::{TcpListener, TcpStream, ToSocketAddrs},
    runtime,
    sync::oneshot,
    task::JoinHandle,
    time::{sleep, Duration, Instant},
};
use tokio_stream::wrappers::TcpListenerStream;
#[cfg(unix)]
use tokio_stream::wrappers::UnixListenerStream;
use tokio_util::codec::{Encoder, FramedRead, FramedWrite, LinesCodec};
use vector_lib::event::{BatchNotifier, Event, EventArray, LogEvent, MetricTags, MetricValue};
use vector_lib::{
    buffers::topology::channel::LimitedReceiver,
    event::{Metric, MetricKind},
};
#[cfg(test)]
use zstd::Decoder as ZstdDecoder;

use crate::{
    config::{Config, GenerateConfig},
    topology::{RunningTopology, ShutdownErrorReceiver},
    trace,
};

const WAIT_FOR_SECS: u64 = 5; // The default time to wait in `wait_for`
const WAIT_FOR_MIN_MILLIS: u64 = 5; // The minimum time to pause before retrying
const WAIT_FOR_MAX_MILLIS: u64 = 500; // The maximum time to pause before retrying

#[cfg(any(test, feature = "component-test-utils"))]
pub mod components;

#[cfg(test)]
pub mod http;

#[cfg(test)]
pub mod metrics;

#[cfg(test)]
pub mod mock;

pub mod stats;

#[macro_export]
macro_rules! assert_downcast_matches {
    ($e:expr, $t:ty, $v:pat) => {{
        match $e.downcast_ref::<$t>() {
            Some($v) => (),
            got => panic!("Assertion failed: got wrong error variant {:?}", got),
        }
    }};
}

#[macro_export]
macro_rules! log_event {
    ($($key:expr => $value:expr),*  $(,)?) => {
        #[allow(unused_variables)]
        {
            let mut event = $crate::event::Event::Log($crate::event::LogEvent::default());
            let log = event.as_mut_log();
            $(
                log.insert($key, $value);
            )*
            event
        }
    };
}

pub fn test_generate_config<T>()
where
    for<'de> T: GenerateConfig + serde::Deserialize<'de>,
{
    let cfg = toml::to_string(&T::generate_config()).unwrap();

    toml::from_str::<T>(&cfg)
        .unwrap_or_else(|e| panic!("Invalid config generated from string:\n\n{}\n'{}'", e, cfg));
}

pub fn open_fixture(path: impl AsRef<Path>) -> crate::Result<serde_json::Value> {
    let test_file = match File::open(path) {
        Ok(file) => file,
        Err(e) => return Err(e.into()),
    };
    let value: serde_json::Value = serde_json::from_reader(test_file)?;
    Ok(value)
}

pub fn next_addr_for_ip(ip: IpAddr) -> SocketAddr {
    let port = pick_unused_port(ip);
    SocketAddr::new(ip, port)
}

pub fn next_addr() -> SocketAddr {
    next_addr_for_ip(IpAddr::V4(Ipv4Addr::LOCALHOST))
}

pub fn next_addr_v6() -> SocketAddr {
    next_addr_for_ip(IpAddr::V6(Ipv6Addr::LOCALHOST))
}

pub fn trace_init() {
    #[cfg(unix)]
    let color = {
        use std::io::IsTerminal;
        std::io::stdout().is_terminal()
    };
    // Windows: ANSI colors are not supported by cmd.exe
    // Color is false for everything except unix.
    #[cfg(not(unix))]
    let color = false;

    let levels = std::env::var("TEST_LOG").unwrap_or_else(|_| "error".to_string());

    trace::init(color, false, &levels, 10);

    // Initialize metrics as well
    vector_lib::metrics::init_test();
}

pub async fn send_lines(
    addr: SocketAddr,
    lines: impl IntoIterator<Item = String>,
) -> Result<SocketAddr, Infallible> {
    send_encodable(addr, LinesCodec::new(), lines).await
}

pub async fn send_encodable<I, E: From<std::io::Error> + std::fmt::Debug>(
    addr: SocketAddr,
    encoder: impl Encoder<I, Error = E>,
    lines: impl IntoIterator<Item = I>,
) -> Result<SocketAddr, Infallible> {
    let stream = TcpStream::connect(&addr).await.unwrap();

    let local_addr = stream.local_addr().unwrap();

    let mut sink = FramedWrite::new(stream, encoder);

    let mut lines = stream::iter(lines.into_iter()).map(Ok);
    sink.send_all(&mut lines).await.unwrap();

    let stream = sink.get_mut();
    stream.shutdown().await.unwrap();

    Ok(local_addr)
}

pub async fn send_lines_tls(
    addr: SocketAddr,
    host: String,
    lines: impl Iterator<Item = String>,
    ca: impl Into<Option<&Path>>,
    client_cert: impl Into<Option<&Path>>,
    client_key: impl Into<Option<&Path>>,
) -> Result<SocketAddr, Infallible> {
    let stream = TcpStream::connect(&addr).await.unwrap();

    let local_addr = stream.local_addr().unwrap();

    let mut connector = SslConnector::builder(SslMethod::tls()).unwrap();
    if let Some(ca) = ca.into() {
        connector.set_ca_file(ca).unwrap();
    } else {
        connector.set_verify(SslVerifyMode::NONE);
    }

    if let Some(cert_file) = client_cert.into() {
        connector.set_certificate_chain_file(cert_file).unwrap();
    }

    if let Some(key_file) = client_key.into() {
        connector
            .set_private_key_file(key_file, SslFiletype::PEM)
            .unwrap();
    }

    let ssl = connector
        .build()
        .configure()
        .unwrap()
        .into_ssl(&host)
        .unwrap();

    let mut stream = tokio_openssl::SslStream::new(ssl, stream).unwrap();
    Pin::new(&mut stream).connect().await.unwrap();
    let mut sink = FramedWrite::new(stream, LinesCodec::new());

    let mut lines = stream::iter(lines).map(Ok);
    sink.send_all(&mut lines).await.unwrap();

    let stream = sink.get_mut().get_mut();
    stream.shutdown().await.unwrap();

    Ok(local_addr)
}

pub fn temp_file() -> PathBuf {
    let path = std::env::temp_dir();
    let file_name = random_string(16);
    path.join(file_name + ".log")
}

pub fn temp_dir() -> PathBuf {
    let path = std::env::temp_dir();
    let dir_name = random_string(16);
    path.join(dir_name)
}

pub fn map_event_batch_stream(
    stream: impl Stream<Item = Event>,
    batch: Option<BatchNotifier>,
) -> impl Stream<Item = EventArray> {
    stream.map(move |event| event.with_batch_notifier_option(&batch).into())
}

// TODO refactor to have a single implementation for `Event`, `LogEvent` and `Metric`.
fn map_batch_stream(
    stream: impl Stream<Item = LogEvent>,
    batch: Option<BatchNotifier>,
) -> impl Stream<Item = EventArray> {
    stream.map(move |log| vec![log.with_batch_notifier_option(&batch)].into())
}

pub fn generate_lines_with_stream<Gen: FnMut(usize) -> String>(
    generator: Gen,
    count: usize,
    batch: Option<BatchNotifier>,
) -> (Vec<String>, impl Stream<Item = EventArray>) {
    let lines = (0..count).map(generator).collect::<Vec<_>>();
    let stream = map_batch_stream(
        stream::iter(lines.clone()).map(LogEvent::from_str_legacy),
        batch,
    );
    (lines, stream)
}

pub fn random_lines_with_stream(
    len: usize,
    count: usize,
    batch: Option<BatchNotifier>,
) -> (Vec<String>, impl Stream<Item = EventArray>) {
    let generator = move |_| random_string(len);
    generate_lines_with_stream(generator, count, batch)
}

pub fn generate_events_with_stream<Gen: FnMut(usize) -> Event>(
    generator: Gen,
    count: usize,
    batch: Option<BatchNotifier>,
) -> (Vec<Event>, impl Stream<Item = EventArray>) {
    let events = (0..count).map(generator).collect::<Vec<_>>();
    let stream = map_batch_stream(
        stream::iter(events.clone()).map(|event| event.into_log()),
        batch,
    );
    (events, stream)
}

pub fn random_metrics_with_stream(
    count: usize,
    batch: Option<BatchNotifier>,
    tags: Option<MetricTags>,
) -> (Vec<Event>, impl Stream<Item = EventArray>) {
    let timestamp = Utc::now().trunc_subsecs(3);
    let events: Vec<_> = (0..count)
        .map(|index| {
            let ts = timestamp + (std::time::Duration::from_secs(2) * index as u32);
            Event::Metric(
                Metric::new(
                    format!("counter_{}", thread_rng().gen::<u32>()),
                    MetricKind::Incremental,
                    MetricValue::Counter {
                        value: index as f64,
                    },
                )
                .with_timestamp(Some(ts))
                .with_tags(tags.clone()),
            )
            // this ensures we get Origin Metadata, with an undefined service but that's ok.
            .with_source_type("a_source_like_none_other")
        })
        .collect();

    let stream = map_event_batch_stream(stream::iter(events.clone()), batch);
    (events, stream)
}

pub fn random_events_with_stream(
    len: usize,
    count: usize,
    batch: Option<BatchNotifier>,
) -> (Vec<Event>, impl Stream<Item = EventArray>) {
    let events = (0..count)
        .map(|_| Event::from(LogEvent::from_str_legacy(random_string(len))))
        .collect::<Vec<_>>();
    let stream = map_batch_stream(
        stream::iter(events.clone()).map(|event| event.into_log()),
        batch,
    );
    (events, stream)
}

pub fn random_updated_events_with_stream<F>(
    len: usize,
    count: usize,
    batch: Option<BatchNotifier>,
    update_fn: F,
) -> (Vec<Event>, impl Stream<Item = EventArray>)
where
    F: Fn((usize, LogEvent)) -> LogEvent,
{
    let events = (0..count)
        .map(|_| LogEvent::from_str_legacy(random_string(len)))
        .enumerate()
        .map(update_fn)
        .map(Event::Log)
        .collect::<Vec<_>>();
    let stream = map_batch_stream(
        stream::iter(events.clone()).map(|event| event.into_log()),
        batch,
    );
    (events, stream)
}

pub fn random_string(len: usize) -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect::<String>()
}

pub fn random_lines(len: usize) -> impl Iterator<Item = String> {
    iter::repeat_with(move || random_string(len))
}

pub fn random_map(max_size: usize, field_len: usize) -> HashMap<String, String> {
    let size = thread_rng().gen_range(0..max_size);

    (0..size)
        .map(move |_| (random_string(field_len), random_string(field_len)))
        .collect()
}

pub fn random_maps(
    max_size: usize,
    field_len: usize,
) -> impl Iterator<Item = HashMap<String, String>> {
    iter::repeat_with(move || random_map(max_size, field_len))
}

pub async fn collect_n<S>(rx: S, n: usize) -> Vec<S::Item>
where
    S: Stream,
{
    rx.take(n).collect().await
}

pub async fn collect_n_stream<T, S: Stream<Item = T> + Unpin>(stream: &mut S, n: usize) -> Vec<T> {
    let mut events = Vec::with_capacity(n);

    while events.len() < n {
        let e = stream.next().await.unwrap();
        events.push(e);
    }
    events
}

pub async fn collect_ready<S>(mut rx: S) -> Vec<S::Item>
where
    S: Stream + Unpin,
{
    let waker = noop_waker_ref();
    let mut cx = Context::from_waker(waker);

    let mut vec = Vec::new();
    loop {
        match rx.poll_next_unpin(&mut cx) {
            Poll::Ready(Some(item)) => vec.push(item),
            Poll::Ready(None) | Poll::Pending => return vec,
        }
    }
}

pub async fn collect_limited<T: Send + 'static>(mut rx: LimitedReceiver<T>) -> Vec<T> {
    let mut items = Vec::new();
    while let Some(item) = rx.next().await {
        items.push(item);
    }
    items
}

pub async fn collect_n_limited<T: Send + 'static>(mut rx: LimitedReceiver<T>, n: usize) -> Vec<T> {
    let mut items = Vec::new();
    while items.len() < n {
        match rx.next().await {
            Some(item) => items.push(item),
            None => break,
        }
    }
    items
}

pub fn lines_from_file<P: AsRef<Path>>(path: P) -> Vec<String> {
    trace!(message = "Reading file.", path = %path.as_ref().display());
    let mut file = File::open(path).unwrap();
    let mut output = String::new();
    file.read_to_string(&mut output).unwrap();
    output.lines().map(|s| s.to_owned()).collect()
}

pub fn lines_from_gzip_file<P: AsRef<Path>>(path: P) -> Vec<String> {
    trace!(message = "Reading gzip file.", path = %path.as_ref().display());
    let mut file = File::open(path).unwrap();
    let mut gzip_bytes = Vec::new();
    file.read_to_end(&mut gzip_bytes).unwrap();
    let mut output = String::new();
    MultiGzDecoder::new(&gzip_bytes[..])
        .read_to_string(&mut output)
        .unwrap();
    output.lines().map(|s| s.to_owned()).collect()
}

#[cfg(test)]
pub fn lines_from_zstd_file<P: AsRef<Path>>(path: P) -> Vec<String> {
    trace!(message = "Reading zstd file.", path = %path.as_ref().display());
    let file = File::open(path).unwrap();
    let mut output = String::new();
    ZstdDecoder::new(file)
        .unwrap()
        .read_to_string(&mut output)
        .unwrap();
    output.lines().map(|s| s.to_owned()).collect()
}

pub fn runtime() -> runtime::Runtime {
    runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
}

// Wait for a Future to resolve, or the duration to elapse (will panic)
pub async fn wait_for_duration<F, Fut>(mut f: F, duration: Duration)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool> + Send + 'static,
{
    let started = Instant::now();
    let mut delay = WAIT_FOR_MIN_MILLIS;
    while !f().await {
        sleep(Duration::from_millis(delay)).await;
        if started.elapsed() > duration {
            panic!("Timed out while waiting");
        }
        // quadratic backoff up to a maximum delay
        delay = (delay * 2).min(WAIT_FOR_MAX_MILLIS);
    }
}

// Wait for 5 seconds
pub async fn wait_for<F, Fut>(f: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool> + Send + 'static,
{
    wait_for_duration(f, Duration::from_secs(WAIT_FOR_SECS)).await
}

// Wait (for 5 secs) for a TCP socket to be reachable
pub async fn wait_for_tcp<A>(addr: A)
where
    A: ToSocketAddrs + Clone + Send + 'static,
{
    wait_for(move || {
        let addr = addr.clone();
        async move { TcpStream::connect(addr).await.is_ok() }
    })
    .await
}

// Allows specifying a custom duration to wait for a TCP socket to be reachable
pub async fn wait_for_tcp_duration(addr: SocketAddr, duration: Duration) {
    wait_for_duration(
        || async move { TcpStream::connect(addr).await.is_ok() },
        duration,
    )
    .await
}

pub async fn wait_for_atomic_usize<T, F>(value: T, unblock: F)
where
    T: AsRef<AtomicUsize>,
    F: Fn(usize) -> bool,
{
    let value = value.as_ref();
    wait_for(|| ready(unblock(value.load(Ordering::SeqCst)))).await
}

// Retries a func every `retry` duration until given an Ok(T); panics after `until` elapses
pub async fn retry_until<'a, F, Fut, T, E>(mut f: F, retry: Duration, until: Duration) -> T
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>> + Send + 'a,
{
    let started = Instant::now();
    while started.elapsed() < until {
        match f().await {
            Ok(res) => return res,
            Err(_) => tokio::time::sleep(retry).await,
        }
    }
    panic!("Timeout")
}

pub struct CountReceiver<T> {
    count: Arc<AtomicUsize>,
    trigger: Option<oneshot::Sender<()>>,
    connected: Option<oneshot::Receiver<()>>,
    handle: JoinHandle<Vec<T>>,
}

impl<T: Send + 'static> CountReceiver<T> {
    pub fn count(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }

    /// Succeeds once first connection has been made.
    pub async fn connected(&mut self) {
        if let Some(tripwire) = self.connected.take() {
            tripwire.await.unwrap();
        }
    }

    fn new<F, Fut>(make_fut: F) -> CountReceiver<T>
    where
        F: FnOnce(Arc<AtomicUsize>, oneshot::Receiver<()>, oneshot::Sender<()>) -> Fut,
        Fut: Future<Output = Vec<T>> + Send + 'static,
    {
        let count = Arc::new(AtomicUsize::new(0));
        let (trigger, tripwire) = oneshot::channel();
        let (trigger_connected, connected) = oneshot::channel();

        CountReceiver {
            count: Arc::clone(&count),
            trigger: Some(trigger),
            connected: Some(connected),
            handle: tokio::spawn(make_fut(count, tripwire, trigger_connected)),
        }
    }

    pub fn receive_items_stream<S, F, Fut>(make_stream: F) -> CountReceiver<T>
    where
        S: Stream<Item = T> + Send + 'static,
        F: FnOnce(oneshot::Receiver<()>, oneshot::Sender<()>) -> Fut + Send + 'static,
        Fut: Future<Output = S> + Send + 'static,
    {
        CountReceiver::new(|count, tripwire, connected| async move {
            let stream = make_stream(tripwire, connected).await;
            stream
                .inspect(move |_| {
                    count.fetch_add(1, Ordering::Relaxed);
                })
                .collect::<Vec<T>>()
                .await
        })
    }
}

impl<T> Future for CountReceiver<T> {
    type Output = Vec<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if let Some(trigger) = this.trigger.take() {
            _ = trigger.send(());
        }

        let result = ready!(this.handle.poll_unpin(cx));
        Poll::Ready(result.unwrap())
    }
}

impl CountReceiver<String> {
    pub fn receive_lines(addr: SocketAddr) -> CountReceiver<String> {
        CountReceiver::new(|count, tripwire, connected| async move {
            let listener = TcpListener::bind(addr).await.unwrap();
            CountReceiver::receive_lines_stream(
                TcpListenerStream::new(listener),
                count,
                tripwire,
                Some(connected),
            )
            .await
        })
    }

    #[cfg(unix)]
    pub fn receive_lines_unix<P>(path: P) -> CountReceiver<String>
    where
        P: AsRef<Path> + Send + 'static,
    {
        CountReceiver::new(|count, tripwire, connected| async move {
            let listener = tokio::net::UnixListener::bind(path).unwrap();
            CountReceiver::receive_lines_stream(
                UnixListenerStream::new(listener),
                count,
                tripwire,
                Some(connected),
            )
            .await
        })
    }

    async fn receive_lines_stream<S, T>(
        stream: S,
        count: Arc<AtomicUsize>,
        tripwire: oneshot::Receiver<()>,
        mut connected: Option<oneshot::Sender<()>>,
    ) -> Vec<String>
    where
        S: Stream<Item = IoResult<T>>,
        T: AsyncWrite + AsyncRead,
    {
        stream
            .take_until(tripwire)
            .map_ok(|socket| FramedRead::new(socket, LinesCodec::new()))
            .map(|x| {
                connected.take().map(|trigger| trigger.send(()));
                x.unwrap()
            })
            .flatten()
            .map(|x| x.unwrap())
            .inspect(move |_| {
                count.fetch_add(1, Ordering::Relaxed);
            })
            .collect::<Vec<String>>()
            .await
    }
}

impl CountReceiver<Event> {
    pub fn receive_events<S>(stream: S) -> CountReceiver<Event>
    where
        S: Stream<Item = Event> + Send + 'static,
    {
        CountReceiver::new(|count, tripwire, connected| async move {
            connected.send(()).unwrap();
            stream
                .take_until(tripwire)
                .inspect(move |_| {
                    count.fetch_add(1, Ordering::Relaxed);
                })
                .collect::<Vec<Event>>()
                .await
        })
    }
}

pub async fn start_topology(
    mut config: Config,
    require_healthy: impl Into<Option<bool>>,
) -> (RunningTopology, ShutdownErrorReceiver) {
    config.healthchecks.set_require_healthy(require_healthy);
    RunningTopology::start_init_validated(config, Default::default())
        .await
        .unwrap()
}

/// Collect the first `n` events from a stream while a future is spawned
/// in the background. This is used for tests where the collect has to
/// happen concurrent with the sending process (ie the stream is
/// handling finalization, which is required for the future to receive
/// an acknowledgement).
pub async fn spawn_collect_n<F, S>(future: F, stream: S, n: usize) -> Vec<Event>
where
    F: Future<Output = ()> + Send + 'static,
    S: Stream<Item = Event>,
{
    // TODO: Switch to using `select!` so that we can drive `future` to completion while also driving `collect_n`,
    // such that if `future` panics, we break out and don't continue driving `collect_n`. In most cases, `future`
    // completing successfully is what actually drives events into `stream`, so continuing to wait for all N events when
    // the catalyst has failed is.... almost never the desired behavior.
    let sender = tokio::spawn(future);
    let events = collect_n(stream, n).await;
    sender.await.expect("Failed to send data");
    events
}

/// Collect all the ready events from a stream after spawning a future
/// in the background and letting it run for a given interval. This is
/// used for tests where the collect has to happen concurrent with the
/// sending process (ie the stream is handling finalization, which is
/// required for the future to receive an acknowledgement).
pub async fn spawn_collect_ready<F, S>(future: F, stream: S, sleep: u64) -> Vec<Event>
where
    F: Future<Output = ()> + Send + 'static,
    S: Stream<Item = Event> + Unpin,
{
    let sender = tokio::spawn(future);
    tokio::time::sleep(Duration::from_secs(sleep)).await;
    let events = collect_ready(stream).await;
    sender.await.expect("Failed to send data");
    events
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, RwLock},
        time::Duration,
    };

    use super::retry_until;

    // helper which errors the first 3x, and succeeds on the 4th
    async fn retry_until_helper(count: Arc<RwLock<i32>>) -> Result<(), ()> {
        if *count.read().unwrap() < 3 {
            let mut c = count.write().unwrap();
            *c += 1;
            return Err(());
        }
        Ok(())
    }

    #[tokio::test]
    async fn retry_until_before_timeout() {
        let count = Arc::new(RwLock::new(0));
        let func = || {
            let count = Arc::clone(&count);
            retry_until_helper(count)
        };

        retry_until(func, Duration::from_millis(10), Duration::from_secs(1)).await;
    }
}

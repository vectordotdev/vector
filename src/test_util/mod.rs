use std::{
    collections::{BTreeSet, HashMap},
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
        Arc, Mutex,
    },
    task::{Context, Poll},
};

use async_trait::async_trait;
use flate2::read::MultiGzDecoder;
use futures::{
    ready, stream, task::noop_waker_ref, FutureExt, SinkExt, Stream, StreamExt, TryStreamExt,
};
use futures_util::{
    future::{err, ok},
    stream::BoxStream,
    Sink,
};
use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
use portpicker::pick_unused_port;
use rand::{thread_rng, Rng};
use rand_distr::Alphanumeric;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
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
use value::Value;
use vector_buffers::{
    topology::channel::{limited, LimitedReceiver},
    Acker,
};
use vector_core::{
    config::{AcknowledgementsConfig, DataType, Input, Output},
    event::{
        metric::{MetricData, Sample},
        BatchNotifier, Event, EventArray, EventContainer, LogEvent, MetricValue,
    },
    schema,
    sink::{StreamSink, VectorSink},
    source::Source,
    transform::{FunctionTransform, OutputBuffer, Transform, TransformConfig, TransformContext},
};

use crate::{
    config::{
        Config, ConfigDiff, GenerateConfig, SinkConfig, SinkContext, SourceConfig, SourceContext,
    },
    sinks::Healthcheck,
    topology::{self, RunningTopology},
    trace, SourceSender,
};

const WAIT_FOR_SECS: u64 = 5; // The default time to wait in `wait_for`
const WAIT_FOR_MIN_MILLIS: u64 = 5; // The minimum time to pause before retrying
const WAIT_FOR_MAX_MILLIS: u64 = 500; // The maximum time to pause before retrying

#[cfg(test)]
pub mod components;

#[cfg(test)]
pub mod http;

#[cfg(test)]
pub mod metrics;
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
            let mut event = crate::event::Event::Log(crate::event::LogEvent::default());
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
    let cfg = T::generate_config().to_string();
    toml::from_str::<T>(&cfg).expect("Invalid config generated");
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
    next_addr_for_ip(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)))
}

pub fn next_addr_v6() -> SocketAddr {
    next_addr_for_ip(IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)))
}

pub fn trace_init() {
    #[cfg(unix)]
    let color = atty::is(atty::Stream::Stdout);
    // Windows: ANSI colors are not supported by cmd.exe
    // Color is false for everything except unix.
    #[cfg(not(unix))]
    let color = false;

    let levels = std::env::var("TEST_LOG").unwrap_or_else(|_| "error".to_string());

    trace::init(color, false, &levels);
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
) -> Result<SocketAddr, Infallible> {
    let stream = TcpStream::connect(&addr).await.unwrap();

    let local_addr = stream.local_addr().unwrap();

    let mut connector = SslConnector::builder(SslMethod::tls()).unwrap();
    if let Some(ca) = ca.into() {
        connector.set_ca_file(ca).unwrap();
    } else {
        connector.set_verify(SslVerifyMode::NONE);
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
    let stream = map_batch_stream(stream::iter(lines.clone()).map(LogEvent::from), batch);
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

pub fn random_events_with_stream(
    len: usize,
    count: usize,
    batch: Option<BatchNotifier>,
) -> (Vec<Event>, impl Stream<Item = EventArray>) {
    let events = (0..count)
        .map(|_| Event::from(random_string(len)))
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
    F: Fn((usize, Event)) -> Event,
{
    let events = (0..count)
        .map(|_| Event::from(random_string(len)))
        .enumerate()
        .map(update_fn)
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
    S: Stream + Unpin,
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
            let _ = trigger.send(());
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
) -> (RunningTopology, tokio::sync::mpsc::UnboundedReceiver<()>) {
    config.healthchecks.set_require_healthy(require_healthy);
    let diff = ConfigDiff::initial(&config);
    let pieces = topology::build_or_log_errors(&config, &diff, HashMap::new())
        .await
        .unwrap();
    topology::start_validated(config, diff, pieces)
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
    S: Stream<Item = Event> + Unpin,
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

pub fn sink(channel_size: usize) -> (impl Stream<Item = EventArray>, MockSinkConfig) {
    let (tx, rx) = SourceSender::new_with_buffer(channel_size);
    let sink = MockSinkConfig::new(tx, true);
    (rx.into_stream(), sink)
}

pub fn sink_with_data(
    channel_size: usize,
    data: &str,
) -> (impl Stream<Item = EventArray>, MockSinkConfig) {
    let (tx, rx) = SourceSender::new_with_buffer(channel_size);
    let sink = MockSinkConfig::new_with_data(tx, true, data);
    (rx.into_stream(), sink)
}

pub fn sink_failing_healthcheck(
    channel_size: usize,
) -> (impl Stream<Item = EventArray>, MockSinkConfig) {
    let (tx, rx) = SourceSender::new_with_buffer(channel_size);
    let sink = MockSinkConfig::new(tx, false);
    (rx.into_stream(), sink)
}

pub fn source() -> (SourceSender, MockSourceConfig) {
    let (tx, rx) = SourceSender::new_with_buffer(1);
    let source = MockSourceConfig::new(rx);
    (tx, source)
}

pub fn source_with_data(data: &str) -> (SourceSender, MockSourceConfig) {
    let (tx, rx) = SourceSender::new_with_buffer(1);
    let source = MockSourceConfig::new_with_data(rx, data);
    (tx, source)
}

pub fn source_with_event_counter() -> (SourceSender, MockSourceConfig, Arc<AtomicUsize>) {
    let event_counter = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = SourceSender::new_with_buffer(1);
    let source = MockSourceConfig::new_with_event_counter(rx, Arc::clone(&event_counter));
    (tx, source, event_counter)
}

pub fn transform(suffix: &str, increase: f64) -> MockTransformConfig {
    MockTransformConfig::new(suffix.to_owned(), increase)
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct MockSourceConfig {
    #[serde(skip)]
    receiver: Arc<Mutex<Option<LimitedReceiver<EventArray>>>>,
    #[serde(skip)]
    event_counter: Option<Arc<AtomicUsize>>,
    #[serde(skip)]
    data_type: Option<DataType>,
    #[serde(skip)]
    force_shutdown: bool,
    // something for serde to use, so we can trigger rebuilds
    data: Option<String>,
}

impl Default for MockSourceConfig {
    fn default() -> Self {
        let (_, receiver) = limited(1000);
        Self {
            receiver: Arc::new(Mutex::new(Some(receiver))),
            event_counter: None,
            data_type: Some(DataType::all()),
            force_shutdown: false,
            data: None,
        }
    }
}

impl_generate_config_from_default!(MockSourceConfig);

impl MockSourceConfig {
    pub fn new(receiver: LimitedReceiver<EventArray>) -> Self {
        Self {
            receiver: Arc::new(Mutex::new(Some(receiver))),
            event_counter: None,
            data_type: Some(DataType::all()),
            force_shutdown: false,
            data: None,
        }
    }

    pub fn new_with_data(receiver: LimitedReceiver<EventArray>, data: &str) -> Self {
        Self {
            receiver: Arc::new(Mutex::new(Some(receiver))),
            event_counter: None,
            data_type: Some(DataType::all()),
            force_shutdown: false,
            data: Some(data.into()),
        }
    }

    pub fn new_with_event_counter(
        receiver: LimitedReceiver<EventArray>,
        event_counter: Arc<AtomicUsize>,
    ) -> Self {
        Self {
            receiver: Arc::new(Mutex::new(Some(receiver))),
            event_counter: Some(event_counter),
            data_type: Some(DataType::all()),
            force_shutdown: false,
            data: None,
        }
    }

    pub fn set_force_shutdown(&mut self, force_shutdown: bool) {
        self.force_shutdown = force_shutdown;
    }
}

#[async_trait]
#[typetag::serde(name = "mock_source")]
impl SourceConfig for MockSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<Source> {
        let wrapped = Arc::clone(&self.receiver);
        let event_counter = self.event_counter.clone();
        let mut recv = wrapped.lock().unwrap().take().unwrap();
        let shutdown1 = cx.shutdown.clone();
        let shutdown2 = cx.shutdown;
        let mut out = cx.out;
        let force_shutdown = self.force_shutdown;

        Ok(Box::pin(async move {
            tokio::pin!(shutdown1);
            tokio::pin!(shutdown2);

            loop {
                tokio::select! {
                    biased;

                    _ = &mut shutdown1, if force_shutdown => break,

                    Some(array) = recv.next() => {
                        if let Some(counter) = &event_counter {
                            counter.fetch_add(array.len(), Ordering::Relaxed);
                        }

                        if let Err(e) = out.send_event(array).await {
                            error!(message = "Error sending in sink..", %e);
                            return Err(())
                        }
                    },

                    _ = &mut shutdown2, if !force_shutdown => break,
                }
            }

            info!("Finished sending.");
            Ok(())
        }))
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(self.data_type.unwrap())]
    }

    fn source_type(&self) -> &'static str {
        "mock_source"
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug)]
pub struct MockTransform {
    suffix: String,
    increase: f64,
}

impl FunctionTransform for MockTransform {
    fn transform(&mut self, output: &mut OutputBuffer, mut event: Event) {
        match &mut event {
            Event::Log(log) => {
                let mut v = log
                    .get(crate::config::log_schema().message_key())
                    .unwrap()
                    .to_string_lossy();
                v.push_str(&self.suffix);
                log.insert(crate::config::log_schema().message_key(), Value::from(v));
            }
            Event::Metric(metric) => {
                let increment = match metric.value() {
                    MetricValue::Counter { .. } => Some(MetricValue::Counter {
                        value: self.increase,
                    }),
                    MetricValue::Gauge { .. } => Some(MetricValue::Gauge {
                        value: self.increase,
                    }),
                    MetricValue::Distribution { statistic, .. } => {
                        Some(MetricValue::Distribution {
                            samples: vec![Sample {
                                value: self.increase,
                                rate: 1,
                            }],
                            statistic: *statistic,
                        })
                    }
                    MetricValue::AggregatedHistogram { .. } => None,
                    MetricValue::AggregatedSummary { .. } => None,
                    MetricValue::Sketch { .. } => None,
                    MetricValue::Set { .. } => {
                        let mut values = BTreeSet::new();
                        values.insert(self.suffix.clone());
                        Some(MetricValue::Set { values })
                    }
                };
                if let Some(increment) = increment {
                    assert!(metric.add(&MetricData {
                        kind: metric.kind(),
                        timestamp: metric.timestamp(),
                        value: increment,
                    }));
                }
            }
            Event::Trace(trace) => {
                let mut v = trace
                    .get(crate::config::log_schema().message_key())
                    .unwrap()
                    .to_string_lossy();
                v.push_str(&self.suffix);
                trace.insert(crate::config::log_schema().message_key(), Value::from(v));
            }
        };
        output.push(event);
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct MockTransformConfig {
    suffix: String,
    increase: f64,
}

impl_generate_config_from_default!(MockTransformConfig);

impl MockTransformConfig {
    pub const fn new(suffix: String, increase: f64) -> Self {
        Self { suffix, increase }
    }
}

#[async_trait]
#[typetag::serde(name = "mock_transform")]
impl TransformConfig for MockTransformConfig {
    async fn build(&self, _globals: &TransformContext) -> crate::Result<Transform> {
        Ok(Transform::function(MockTransform {
            suffix: self.suffix.clone(),
            increase: self.increase,
        }))
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn outputs(&self, _: &schema::Definition) -> Vec<Output> {
        vec![Output::default(DataType::all())]
    }

    fn transform_type(&self) -> &'static str {
        "mock_transform"
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct MockSinkConfig {
    #[serde(skip)]
    sink: Mode,
    #[serde(skip)]
    healthy: bool,
    // something for serde to use, so we can trigger rebuilds
    data: Option<String>,
}

impl_generate_config_from_default!(MockSinkConfig);

#[derive(Debug, Clone)]
enum Mode {
    Normal(SourceSender),
    Dead,
}

impl Default for Mode {
    fn default() -> Self {
        Mode::Dead
    }
}

impl MockSinkConfig {
    pub const fn new(sink: SourceSender, healthy: bool) -> Self {
        Self {
            sink: Mode::Normal(sink),
            healthy,
            data: None,
        }
    }

    pub fn new_with_data(sink: SourceSender, healthy: bool, data: &str) -> Self {
        Self {
            sink: Mode::Normal(sink),
            healthy,
            data: Some(data.into()),
        }
    }
}

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("unhealthy"))]
    Unhealthy,
}

#[async_trait]
#[typetag::serde(name = "mock_sink")]
impl SinkConfig for MockSinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        // If this sink is set to not be healthy, just send the healthcheck error immediately over
        // the oneshot.. otherwise, pass the sender to the sink so it can send it only once it has
        // started running, so that tests can request the topology be healthy before proceeding.
        let (tx, rx) = oneshot::channel();

        let health_tx = if self.healthy {
            Some(tx)
        } else {
            let _ = tx.send(Err(HealthcheckError::Unhealthy.into()));
            None
        };

        let sink = MockSink {
            acker: cx.acker(),
            sink: self.sink.clone(),
            health_tx,
        };

        let healthcheck = async move { rx.await.unwrap() };

        Ok((VectorSink::from_event_streamsink(sink), healthcheck.boxed()))
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn sink_type(&self) -> &'static str {
        "mock_sink"
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        None
    }
}

struct MockSink {
    acker: Acker,
    sink: Mode,
    health_tx: Option<oneshot::Sender<crate::Result<()>>>,
}

#[async_trait]
impl StreamSink<Event> for MockSink {
    async fn run(mut self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        match self.sink {
            Mode::Normal(mut sink) => {
                if let Some(tx) = self.health_tx.take() {
                    let _ = tx.send(Ok(()));
                }

                // We have an inner sink, so forward the input normally
                while let Some(event) = input.next().await {
                    if let Err(error) = sink.send_event(event).await {
                        error!(message = "Ingesting an event failed at mock sink.", %error);
                    }

                    self.acker.ack(1);
                }
            }
            Mode::Dead => {
                // Simulate a dead sink and never poll the input
                futures::future::pending::<()>().await;
            }
        }

        Ok(())
    }
}

/// A source that immediately panics.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct PanicSourceConfig {
    dummy: Option<String>,
}

impl_generate_config_from_default!(PanicSourceConfig);

#[async_trait]
#[typetag::serde(name = "panic_source")]
impl SourceConfig for PanicSourceConfig {
    async fn build(&self, _cx: SourceContext) -> crate::Result<Source> {
        Ok(Box::pin(async { panic!() }))
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(DataType::Log)]
    }

    fn source_type(&self) -> &'static str {
        "panic_source"
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

/// A source that immediately returns an error.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct ErrorSourceConfig {
    dummy: Option<String>,
}

impl_generate_config_from_default!(ErrorSourceConfig);

#[async_trait]
#[typetag::serde(name = "error_source")]
impl SourceConfig for ErrorSourceConfig {
    async fn build(&self, _cx: SourceContext) -> crate::Result<Source> {
        Ok(err(()).boxed())
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(DataType::Log)]
    }

    fn source_type(&self) -> &'static str {
        "error_source"
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct PanicSinkConfig {
    dummy: Option<String>,
}

impl_generate_config_from_default!(PanicSinkConfig);

#[async_trait]
#[typetag::serde(name = "panic_sink")]
impl SinkConfig for PanicSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        Ok((VectorSink::from_event_sink(PanicSink), ok(()).boxed()))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn sink_type(&self) -> &'static str {
        "panic_sink"
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        None
    }
}

struct PanicSink;

impl Sink<Event> for PanicSink {
    type Error = ();

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        panic!()
    }

    fn start_send(self: Pin<&mut Self>, _item: Event) -> Result<(), Self::Error> {
        panic!()
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        panic!()
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        panic!()
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct ErrorSinkConfig {
    dummy: Option<String>,
}

impl_generate_config_from_default!(ErrorSinkConfig);

#[async_trait]
#[typetag::serde(name = "error_sink")]
impl SinkConfig for ErrorSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        Ok((VectorSink::from_event_sink(ErrorSink), ok(()).boxed()))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn sink_type(&self) -> &'static str {
        "panic"
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        None
    }
}

struct ErrorSink;

impl Sink<Event> for ErrorSink {
    type Error = ();

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Err(()))
    }

    fn start_send(self: Pin<&mut Self>, _item: Event) -> Result<(), Self::Error> {
        Err(())
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Err(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Err(()))
    }
}

#[cfg(test)]
mod register {
    use crate::config::{SinkDescription, SourceDescription, TransformDescription};

    use super::{
        ErrorSinkConfig, ErrorSourceConfig, MockSinkConfig, MockSourceConfig, MockTransformConfig,
        PanicSinkConfig, PanicSourceConfig,
    };

    inventory::submit! {
        SourceDescription::new::<MockSourceConfig>("mock_source")
    }

    inventory::submit! {
        SourceDescription::new::<PanicSourceConfig>("panic_source")
    }

    inventory::submit! {
        SourceDescription::new::<ErrorSourceConfig>("error_source")
    }

    inventory::submit! {
        TransformDescription::new::<MockTransformConfig>("mock_transform")
    }

    inventory::submit! {
        SinkDescription::new::<MockSinkConfig>("mock_sink")
    }

    inventory::submit! {
        SinkDescription::new::<PanicSinkConfig>("panic_sink")
    }

    inventory::submit! {
        SinkDescription::new::<ErrorSinkConfig>("error_sink")
    }
}

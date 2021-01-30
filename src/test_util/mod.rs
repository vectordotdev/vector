use crate::{
    config::{Config, ConfigDiff, GenerateConfig},
    topology::{self, RunningTopology},
    trace, Event,
};
use flate2::read::GzDecoder;
use futures::{
    ready, stream, task::noop_waker_ref, FutureExt, SinkExt, Stream, StreamExt, TryStreamExt,
};
use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
use portpicker::pick_unused_port;
use rand::{thread_rng, Rng};
use rand_distr::Alphanumeric;
use std::{
    collections::HashMap,
    convert::Infallible,
    fs::File,
    future::{ready, Future},
    io::Read,
    iter,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, Result as IoResult},
    net::{TcpListener, TcpStream},
    runtime,
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time::{delay_for, Duration, Instant},
};
use tokio_util::codec::{Encoder, FramedRead, FramedWrite, LinesCodec};

const WAIT_FOR_SECS: u64 = 5; // The default time to wait in `wait_for`
const WAIT_FOR_MIN_MILLIS: u64 = 5; // The minimum time to pause before retrying
const WAIT_FOR_MAX_MILLIS: u64 = 500; // The maximum time to pause before retrying

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

pub fn next_addr() -> SocketAddr {
    let port = pick_unused_port();
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port)
}

pub fn next_addr_v6() -> SocketAddr {
    let port = pick_unused_port();
    SocketAddr::new(IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)), port)
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
) -> Result<(), Infallible> {
    send_encodable(addr, LinesCodec::new(), lines).await
}

pub async fn send_encodable<I, E: From<std::io::Error> + std::fmt::Debug>(
    addr: SocketAddr,
    encoder: impl Encoder<I, Error = E>,
    lines: impl IntoIterator<Item = I>,
) -> Result<(), Infallible> {
    let stream = TcpStream::connect(&addr).await.unwrap();
    let mut sink = FramedWrite::new(stream, encoder);

    let mut lines = stream::iter(lines.into_iter()).map(Ok);
    sink.send_all(&mut lines).await.unwrap();

    let stream = sink.get_mut();
    stream.shutdown(Shutdown::Both).unwrap();

    Ok(())
}

pub async fn send_lines_tls(
    addr: SocketAddr,
    host: String,
    lines: impl Iterator<Item = String>,
    ca: impl Into<Option<&Path>>,
) -> Result<(), Infallible> {
    let stream = TcpStream::connect(&addr).await.unwrap();

    let mut connector = SslConnector::builder(SslMethod::tls()).unwrap();
    if let Some(ca) = ca.into() {
        connector.set_ca_file(ca).unwrap();
    } else {
        connector.set_verify(SslVerifyMode::NONE);
    }

    let config = connector.build().configure().unwrap();

    let stream = tokio_openssl::connect(config, &host, stream).await.unwrap();
    let mut sink = FramedWrite::new(stream, LinesCodec::new());

    let mut lines = stream::iter(lines).map(Ok);
    sink.send_all(&mut lines).await.unwrap();

    let stream = sink.get_mut().get_mut();
    stream.shutdown(Shutdown::Both).unwrap();

    Ok(())
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

pub fn random_lines_with_stream(
    len: usize,
    count: usize,
) -> (Vec<String>, impl Stream<Item = Event>) {
    let lines = (0..count).map(|_| random_string(len)).collect::<Vec<_>>();
    let stream = stream::iter(lines.clone()).map(Event::from);
    (lines, stream)
}

fn random_events_with_stream_generic<F>(
    count: usize,
    generator: F,
) -> (Vec<Event>, impl Stream<Item = Event>)
where
    F: Fn() -> Event,
{
    let events = (0..count).map(|_| generator()).collect::<Vec<_>>();
    let stream = stream::iter(events.clone());
    (events, stream)
}

pub fn random_events_with_stream(
    len: usize,
    count: usize,
) -> (Vec<Event>, impl Stream<Item = Event>) {
    random_events_with_stream_generic(count, move || Event::from(random_string(len)))
}

pub fn random_string(len: usize) -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect::<String>()
}

pub fn random_lines(len: usize) -> impl Iterator<Item = String> {
    std::iter::repeat(()).map(move |_| random_string(len))
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
    iter::repeat(()).map(move |_| random_map(max_size, field_len))
}

pub async fn collect_n<T>(rx: mpsc::Receiver<T>, n: usize) -> Vec<T> {
    rx.take(n).collect().await
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
    GzDecoder::new(&gzip_bytes[..])
        .read_to_string(&mut output)
        .unwrap();
    output.lines().map(|s| s.to_owned()).collect()
}

pub fn runtime() -> runtime::Runtime {
    runtime::Builder::new()
        .threaded_scheduler()
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
        delay_for(Duration::from_millis(delay)).await;
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
pub async fn wait_for_tcp(addr: SocketAddr) {
    wait_for(|| async move { TcpStream::connect(addr).await.is_ok() }).await
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
            Err(_) => tokio::time::delay_for(retry).await,
        }
    }
    panic!("Timeout")
}

#[cfg(test)]
mod tests {
    use super::retry_until;
    use std::{
        sync::{Arc, RwLock},
        time::Duration,
    };

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
            let mut listener = TcpListener::bind(addr).await.unwrap();
            CountReceiver::receive_lines_stream(
                listener.incoming(),
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
            let mut listener = tokio::net::UnixListener::bind(path).unwrap();
            CountReceiver::receive_lines_stream(
                listener.incoming(),
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

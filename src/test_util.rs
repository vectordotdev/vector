use futures::{Async, Future, Poll, Sink, Stream};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::codec::{FramedRead, FramedWrite, LinesCodec};
use tokio::net::{TcpListener, TcpStream};
use tokio::util::FutureExt;

static NEXT_PORT: AtomicUsize = AtomicUsize::new(1234);
pub fn next_addr() -> SocketAddr {
    use std::net::{IpAddr, Ipv4Addr};

    let port = NEXT_PORT.fetch_add(1, Ordering::AcqRel) as u16;
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port)
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

pub fn random_string(len: usize) -> String {
    use rand::distributions::Alphanumeric;
    use rand::{thread_rng, Rng};

    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .collect::<String>()
}

pub fn random_lines(len: usize) -> impl Iterator<Item = String> {
    use rand::distributions::Alphanumeric;
    use rand::{rngs::SmallRng, thread_rng, Rng, SeedableRng};

    let mut rng = SmallRng::from_rng(thread_rng()).unwrap();

    std::iter::repeat(()).map(move |_| rng.sample_iter(&Alphanumeric).take(len).collect::<String>())
}

pub fn receive_lines(
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

pub fn receive_lines_with_count(
    addr: &SocketAddr,
    executor: &tokio::runtime::TaskExecutor,
) -> (
    impl Future<Item = Vec<String>, Error = ()>,
    Arc<AtomicUsize>,
) {
    let listener = TcpListener::bind(addr).unwrap();

    let count = Arc::new(AtomicUsize::new(0));
    let count_clone = Arc::clone(&count);

    let lines = listener
        .incoming()
        .take(1)
        .map(|socket| FramedRead::new(socket, LinesCodec::new()))
        .flatten()
        .inspect(move |_| {
            count_clone.fetch_add(1, Ordering::Relaxed);
        })
        .map_err(|e| panic!("{:?}", e))
        .collect();

    (futures::sync::oneshot::spawn(lines, executor), count)
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
    let mut rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(future)
}

pub fn wait_for_tcp(addr: SocketAddr) {
    wait_for(|| std::net::TcpStream::connect(addr).is_ok())
}

pub fn shutdown_on_idle(runtime: tokio::runtime::Runtime) {
    block_on(
        runtime
            .shutdown_on_idle()
            .timeout(std::time::Duration::from_secs(5)),
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

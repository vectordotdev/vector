use futures::{Future, Sink, Stream};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::codec::{FramedWrite, LinesCodec};
use tokio::net::TcpStream;
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
    let wait = std::time::Duration::from_millis(5);
    let limit = std::time::Duration::from_secs(5);
    let mut attempts = 0;
    while let Err(_) = std::net::TcpStream::connect(addr) {
        std::thread::sleep(wait.clone());
        attempts += 1;
        if attempts * wait > limit {
            panic!("timed out waiting for tcp on {:?}", addr);
        }
    }
}

pub fn shutdown_on_idle(runtime: tokio::runtime::Runtime) {
    block_on(
        runtime
            .shutdown_on_idle()
            .timeout(std::time::Duration::from_secs(5)),
    )
    .unwrap()
}

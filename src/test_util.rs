use futures::{Future, Sink, Stream};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::codec::{FramedWrite, LinesCodec};
use tokio::net::TcpStream;

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
                .map(|(_source, sink)| sink)
                .and_then(|sink| {
                    let socket = sink.into_inner().into_inner();
                    tokio::io::shutdown(socket)
                        .map(|_| ())
                        .map_err(|e| panic!("{:}", e))
                })
        })
}

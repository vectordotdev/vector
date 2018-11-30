use std::net::SocketAddr;

use futures::{future, Future, Sink};
use tokio::codec::{FramedWrite, LinesCodec};
use tokio::net::TcpStream;

use Record;

pub struct SplunkSink;

impl super::SinkFactory for SplunkSink {
    type Config = SocketAddr;

    fn build(addr: SocketAddr) -> super::RouterSinkFuture {
        // lazy so that we don't actually try to connect until the future is polled
        Box::new(future::lazy(move || {
            TcpStream::connect(&addr).map(|socket| -> super::RouterSink {
                Box::new(
                    FramedWrite::new(socket, LinesCodec::new())
                        .with(|record: Record| Ok(record.line)),
                )
            })
        }))
    }
}

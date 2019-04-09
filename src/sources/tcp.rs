use crate::record::Record;
use bytes::Bytes;
use codec::BytesDelimitedCodec;
use futures::{future, sync::mpsc, Future, Sink, Stream};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::{self, codec::FramedRead, net::TcpListener};
use tokio_trace::field;
use tokio_trace_futures::Instrument;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct TcpConfig {
    pub address: std::net::SocketAddr,
    #[serde(default = "default_max_length")]
    pub max_length: usize,
}

fn default_max_length() -> usize {
    100 * 1024
}

impl TcpConfig {
    pub fn new(addr: std::net::SocketAddr) -> Self {
        Self {
            address: addr,
            max_length: default_max_length(),
        }
    }
}

#[typetag::serde(name = "tcp")]
impl crate::topology::config::SourceConfig for TcpConfig {
    fn build(&self, out: mpsc::Sender<Record>) -> Result<super::Source, String> {
        Ok(tcp(self.address, self.max_length, out))
    }
}

pub fn tcp(addr: SocketAddr, max_length: usize, out: mpsc::Sender<Record>) -> super::Source {
    let out = out.sink_map_err(|e| error!("error sending line: {:?}", e));

    Box::new(future::lazy(move || {
        let listener = match TcpListener::bind(&addr) {
            Ok(listener) => listener,
            Err(err) => {
                error!("Failed to bind to listener socket: {}", err);
                return future::Either::B(future::err(()));
            }
        };

        info!(message = "listening.", addr = field::display(&addr));

        let future = listener
            .incoming()
            .map_err(|e| error!("failed to accept socket; error = {}", e))
            .for_each(move |socket| {
                let peer_addr = socket.peer_addr().ok().map(|s| s.ip().to_string());

                let span = if let Some(addr) = &peer_addr {
                    info_span!("connection", peer_addr = field::display(addr))
                } else {
                    info_span!("connection")
                };

                let host = peer_addr.map(Bytes::from);

                let inner_span = span.clone();
                span.enter(|| {
                    debug!("accepted a new socket.");

                    let out = out.clone();

                    let lines_in = FramedRead::new(
                        socket,
                        BytesDelimitedCodec::new_with_max_length(b'\n', max_length),
                    )
                    .map(Record::from)
                    .map(move |mut record| {
                        record.host = host.clone();

                        trace!(
                            message = "Received one line.",
                            record = field::debug(&record)
                        );
                        record
                    })
                    .map_err(|e| error!("error reading line: {:?}", e));

                    let handler = lines_in.forward(out).map(|_| debug!("connection closed"));

                    tokio::spawn(handler.instrument(inner_span))
                })
            });
        future::Either::A(future)
    }))
}

#[cfg(test)]
mod test {
    use crate::test_util::{next_addr, send_lines, wait_for_tcp};
    use bytes::Bytes;
    use futures::sync::mpsc;
    use futures::Stream;

    #[test]
    fn tcp_it_includes_host() {
        let (tx, rx) = mpsc::channel(1);

        let addr = next_addr();

        let server = super::tcp(addr, super::default_max_length(), tx);
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        rt.spawn(server);
        wait_for_tcp(addr);

        rt.block_on(send_lines(addr, vec!["test".to_owned()].into_iter()))
            .unwrap();

        let record = rx.wait().next().unwrap().unwrap();
        assert_eq!(record.host, Some(Bytes::from("127.0.0.1")));
    }

    #[test]
    fn tcp_it_defaults_max_length() {
        let with: super::TcpConfig = toml::from_str(
            r#"
            address = "127.0.0.1:1234"
            max_length = 19
            "#,
        )
        .unwrap();

        let without: super::TcpConfig = toml::from_str(
            r#"
            address = "127.0.0.1:1234"
            "#,
        )
        .unwrap();

        assert_eq!(with.max_length, 19);
        assert_eq!(without.max_length, super::default_max_length());
    }
}

use super::util::StreamExt as _;
use crate::record::{self, Record};
use bytes::Bytes;
use codec::{self, BytesDelimitedCodec};
use futures::{future, sync::mpsc, Future, Sink, Stream};
use serde::{Deserialize, Serialize};
use std::net::{SocketAddr, ToSocketAddrs};
use std::time::{Duration, Instant};
use stream_cancel::{StreamExt, Tripwire};
use tokio::{codec::FramedRead, net::TcpListener, timer};
use tokio_trace::field;
use tokio_trace_futures::Instrument;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct TcpConfig {
    pub address: String,
    #[serde(default = "default_max_length")]
    pub max_length: usize,
    #[serde(default = "default_shutdown_timeout_secs")]
    pub shutdown_timeout_secs: u64,
}

fn default_max_length() -> usize {
    100 * 1024
}

fn default_shutdown_timeout_secs() -> u64 {
    30
}

impl TcpConfig {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            address: addr.to_string(),
            max_length: default_max_length(),
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
        }
    }
}

#[typetag::serde(name = "tcp")]
impl crate::topology::config::SourceConfig for TcpConfig {
    fn build(&self, out: mpsc::Sender<Record>) -> Result<super::Source, String> {
        let tcp = tcp(self.clone(), out)?;
        Ok(tcp)
    }
}

pub fn tcp(config: TcpConfig, out: mpsc::Sender<Record>) -> Result<super::Source, String> {
    let out = out.sink_map_err(|e| error!("error sending line: {:?}", e));

    let TcpConfig {
        address,
        max_length,
        shutdown_timeout_secs,
    } = config;

    let addr = address
        .to_socket_addrs()
        .map_err(|e| format!("IO Error: {}", e))?
        .next()
        .ok_or_else(|| "Unable to resolve DNS for provided address".to_string())?;

    let source = future::lazy(move || {
        let listener = match TcpListener::bind(&addr) {
            Ok(listener) => listener,
            Err(err) => {
                error!("Failed to bind to listener socket: {}", err);
                return future::Either::B(future::err(()));
            }
        };

        info!(message = "listening.", addr = field::display(&addr));

        let (trigger, tripwire) = Tripwire::new();
        let tripwire = tripwire
            .and_then(move |_| {
                timer::Delay::new(Instant::now() + Duration::from_secs(shutdown_timeout_secs))
                    .map_err(|err| panic!("Timer error: {:?}", err))
            })
            .shared();

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
                let tripwire = tripwire
                    .clone()
                    .map(move |_| {
                        info!(
                            "Resetting connection (still open after {} seconds).",
                            shutdown_timeout_secs
                        )
                    })
                    .map_err(|_| ());

                span.enter(|| {
                    debug!("accepted a new socket.");

                    let out = out.clone();

                    let lines_in = FramedRead::new(
                        socket,
                        BytesDelimitedCodec::new_with_max_length(b'\n', max_length),
                    )
                    .take_until(tripwire)
                    .map(Record::from)
                    .map(move |mut record| {
                        if let Some(host) = &host {
                            record
                                .structured
                                .insert(record::HOST.clone(), host.clone().into());
                        }

                        trace!(
                            message = "Received one line.",
                            record = field::debug(&record)
                        );
                        record
                    })
                    .filter_map_err(|err| match err {
                        codec::Error::MaxLimitExceeded => {
                            warn!("Received line longer than max_length. Discarding.");
                            None
                        }
                        codec::Error::Io(io) => Some(io),
                    })
                    .map_err(|e| warn!("connection error: {:?}", e));

                    let handler = lines_in.forward(out).map(|_| debug!("connection closed"));

                    tokio::spawn(handler.instrument(inner_span))
                })
            })
            .inspect(|_| trigger.cancel());
        future::Either::A(future)
    });

    Ok(Box::new(source))
}

#[cfg(test)]
mod test {
    use super::TcpConfig;
    use crate::record;
    use crate::test_util::{block_on, next_addr, send_lines, wait_for_tcp};
    use futures::sync::mpsc;
    use futures::Stream;

    #[test]
    fn tcp_it_includes_host() {
        let (tx, rx) = mpsc::channel(1);

        let addr = next_addr();

        let server = super::tcp(TcpConfig::new(addr), tx).unwrap();
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        rt.spawn(server);
        wait_for_tcp(addr);

        rt.block_on(send_lines(addr, vec!["test".to_owned()].into_iter()))
            .unwrap();

        let record = rx.wait().next().unwrap().unwrap();
        assert_eq!(record.structured[&record::HOST], "127.0.0.1".into());
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

    #[test]
    fn tcp_continue_after_long_line() {
        let (tx, rx) = mpsc::channel(10);

        let addr = next_addr();

        let mut config = TcpConfig::new(addr);
        config.max_length = 10;

        let server = super::tcp(config, tx).unwrap();
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        rt.spawn(server);
        wait_for_tcp(addr);

        let lines = vec![
            "short".to_owned(),
            "this is too long".to_owned(),
            "more short".to_owned(),
        ];

        rt.block_on(send_lines(addr, lines.into_iter())).unwrap();

        let (record, rx) = block_on(rx.into_future()).unwrap();
        assert_eq!(record.unwrap().structured[&record::MESSAGE], "short".into());

        let (record, _rx) = block_on(rx.into_future()).unwrap();
        assert_eq!(
            record.unwrap().structured[&record::MESSAGE],
            "more short".into()
        );
    }
}

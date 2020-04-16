use crate::{
    internal_events::TcpConnectionError,
    shutdown::ShutdownSignal,
    stream::StreamExt,
    tls::{MaybeTlsListener, MaybeTlsSettings},
    Event,
};
use bytes::Bytes;
use futures01::{future, sync::mpsc, Future, Sink, Stream};
use listenfd::ListenFd;
use serde::{de, Deserialize, Deserializer, Serialize};
use std::{
    fmt, io,
    net::SocketAddr,
    time::{Duration, Instant},
};
use stream_cancel::Tripwire;
use tokio01::{
    codec::{Decoder, FramedRead},
    net::TcpListener,
    prelude::AsyncRead,
    reactor::Handle,
    timer,
};
use tracing::{field, Span};
use tracing_futures::Instrument;

fn make_listener(
    addr: SocketListenAddr,
    mut listenfd: ListenFd,
    tls: &MaybeTlsSettings,
) -> Option<MaybeTlsListener> {
    match addr {
        SocketListenAddr::SocketAddr(addr) => match tls.bind(&addr) {
            Ok(listener) => Some(listener),
            Err(err) => {
                error!("Failed to bind to listener socket: {}", err);
                None
            }
        },
        SocketListenAddr::SystemdFd(offset) => match listenfd.take_tcp_listener(offset) {
            Ok(Some(listener)) => match TcpListener::from_std(listener, &Handle::default()) {
                Ok(listener) => Some(listener.into()),
                Err(err) => {
                    error!("Failed to bind to listener socket: {}", err);
                    None
                }
            },
            Ok(None) => {
                error!("Failed to take listen FD, not open or already taken");
                None
            }
            Err(err) => {
                error!("Failed to take listen FD: {}", err);
                None
            }
        },
    }
}

pub trait TcpSource: Clone + Send + 'static {
    type Decoder: Decoder<Error = io::Error> + Send + 'static;

    fn decoder(&self) -> Self::Decoder;

    fn build_event(
        &self,
        frame: <Self::Decoder as tokio01::codec::Decoder>::Item,
        host: Bytes,
    ) -> Option<Event>;

    fn run(
        self,
        addr: SocketListenAddr,
        shutdown_timeout_secs: u64,
        tls: MaybeTlsSettings,
        shutdown: ShutdownSignal,
        out: mpsc::Sender<Event>,
    ) -> crate::Result<crate::sources::Source> {
        let out = out.sink_map_err(|e| error!("error sending event: {:?}", e));

        let listenfd = ListenFd::from_env();

        let source = future::lazy(move || {
            let listener = match make_listener(addr, listenfd, &tls) {
                None => return future::Either::B(future::err(())),
                Some(listener) => listener,
            };

            info!(
                message = "listening.",
                addr = field::display(
                    listener
                        .local_addr()
                        .map(|addr| SocketListenAddr::SocketAddr(addr))
                        .unwrap_or(addr)
                )
            );

            let (trigger, tripwire) = Tripwire::new();
            let tripwire = tripwire.select2(shutdown.clone());
            let tripwire = tripwire
                .and_then(move |_| {
                    timer::Delay::new(Instant::now() + Duration::from_secs(shutdown_timeout_secs))
                        .map_err(|err| panic!("Timer error: {:?}", err))
                })
                .shared();

            let future = listener
                .incoming()
                .take_until(shutdown)
                .map_err(|error| {
                    error!(
                        message = "failed to accept socket",
                        %error
                    )
                })
                .for_each(move |socket| {
                    let peer_addr = socket.peer_addr().ip().to_string();

                    let span = info_span!("connection", %peer_addr);

                    let host = Bytes::from(peer_addr);

                    let tripwire = tripwire
                        .clone()
                        .map(move |_| {
                            info!(
                                "Resetting connection (still open after {} seconds).",
                                shutdown_timeout_secs
                            )
                        })
                        .map_err(|_| ());

                    let source = self.clone();
                    span.in_scope(|| {
                        let peer_addr = socket.peer_addr();
                        debug!(message = "accepted a new connection", %peer_addr);
                        handle_stream(span.clone(), socket, source, tripwire, host, out.clone())
                    });
                    Ok(())
                })
                .inspect(|_| trigger.cancel());
            future::Either::A(future)
        });

        Ok(Box::new(source))
    }
}

fn handle_stream(
    span: Span,
    socket: impl AsyncRead + Send + 'static,
    source: impl TcpSource,
    tripwire: impl Future<Item = (), Error = ()> + Send + 'static,
    host: Bytes,
    out: impl Sink<SinkItem = Event, SinkError = ()> + Send + 'static,
) {
    let handler = FramedRead::new(socket, source.decoder())
        .take_until(tripwire)
        .filter_map(move |frame| {
            let host = host.clone();
            source.build_event(frame, host)
        })
        .map_err(|error| {
            emit!(TcpConnectionError { error });
        })
        .forward(out)
        .map(|_| debug!("connection closed."))
        .map_err(|_| warn!("Error received while processing TCP source"));
    tokio01::spawn(handler.instrument(span));
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub enum SocketListenAddr {
    SocketAddr(SocketAddr),
    #[serde(deserialize_with = "parse_systemd_fd")]
    SystemdFd(usize),
}

impl fmt::Display for SocketListenAddr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::SocketAddr(ref addr) => addr.fmt(f),
            Self::SystemdFd(offset) => write!(f, "systemd socket #{}", offset),
        }
    }
}

impl From<SocketAddr> for SocketListenAddr {
    fn from(addr: SocketAddr) -> Self {
        Self::SocketAddr(addr)
    }
}

fn parse_systemd_fd<'de, D>(des: D) -> Result<usize, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &'de str = Deserialize::deserialize(des)?;
    match s {
        "systemd" => Ok(0),
        s if s.starts_with("systemd#") => {
            Ok(s[8..].parse::<usize>().map_err(de::Error::custom)? - 1)
        }
        _ => Err(de::Error::custom("must start with \"systemd\"")),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde::Deserialize;
    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

    #[derive(Debug, Deserialize)]
    struct Config {
        addr: SocketListenAddr,
    }

    #[test]
    fn parse_socket_listen_addr() {
        let test: Config = toml::from_str(r#"addr="127.1.2.3:1234""#).unwrap();
        assert_eq!(
            test.addr,
            SocketListenAddr::SocketAddr(SocketAddr::V4(SocketAddrV4::new(
                Ipv4Addr::new(127, 1, 2, 3),
                1234,
            )))
        );
        let test: Config = toml::from_str(r#"addr="systemd""#).unwrap();
        assert_eq!(test.addr, SocketListenAddr::SystemdFd(0));
        let test: Config = toml::from_str(r#"addr="systemd#3""#).unwrap();
        assert_eq!(test.addr, SocketListenAddr::SystemdFd(2));
    }
}

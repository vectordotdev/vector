mod request_limiter;

use bytes::Bytes;
use futures::{future::BoxFuture, FutureExt, StreamExt};
use listenfd::ListenFd;
use serde::{de, Deserialize, Deserializer, Serialize};
use smallvec::SmallVec;
use socket2::SockRef;
use vector_core::ByteSizeOf;

use std::net::SocketAddr;

use std::{fmt, io, mem::drop, sync::Arc, time::Duration};

use codecs::StreamDecodingError;
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
    time::sleep,
};
use tokio_util::codec::{Decoder, FramedRead};
use tracing_futures::Instrument;

use super::AfterReadExt as _;
use crate::sources::util::tcp::request_limiter::RequestLimiter;
use crate::{
    codecs::ReadyFrames,
    config::{AcknowledgementsConfig, Resource, SourceContext},
    event::{BatchNotifier, BatchStatus, Event},
    internal_events::{
        ConnectionOpen, OpenGauge, SocketEventsReceived, SocketMode, StreamClosedError,
        TcpBytesReceived, TcpSendAckError, TcpSocketTlsConnectionError,
    },
    shutdown::ShutdownSignal,
    tcp::TcpKeepaliveConfig,
    tls::{MaybeTlsIncomingStream, MaybeTlsListener, MaybeTlsSettings},
    SourceSender,
};

const MAX_IN_FLIGHT_EVENTS_TARGET: usize = 100_000;

async fn make_listener(
    addr: SocketListenAddr,
    mut listenfd: ListenFd,
    tls: &MaybeTlsSettings,
) -> Option<MaybeTlsListener> {
    match addr {
        SocketListenAddr::SocketAddr(addr) => match tls.bind(&addr).await {
            Ok(listener) => Some(listener),
            Err(error) => {
                error!(message = "Failed to bind to listener socket.", %error);
                None
            }
        },
        SocketListenAddr::SystemdFd(offset) => match listenfd.take_tcp_listener(offset) {
            Ok(Some(listener)) => match TcpListener::from_std(listener) {
                Ok(listener) => Some(listener.into()),
                Err(error) => {
                    error!(message = "Failed to bind to listener socket.", %error);
                    None
                }
            },
            Ok(None) => {
                error!("Failed to take listen FD, not open or already taken.");
                None
            }
            Err(error) => {
                error!(message = "Failed to take listen FD.", %error);
                None
            }
        },
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum TcpSourceAck {
    Ack,
    Error,
    Reject,
}

pub trait TcpSourceAcker {
    fn build_ack(self, ack: TcpSourceAck) -> Option<Bytes>;
}

pub struct TcpNullAcker;

impl TcpSourceAcker for TcpNullAcker {
    // This function builds an acknowledgement from the source data in
    // the acker and the given acknowledgement code.
    fn build_ack(self, _ack: TcpSourceAck) -> Option<Bytes> {
        None
    }
}

pub trait TcpSource: Clone + Send + Sync + 'static
where
    <<Self as TcpSource>::Decoder as tokio_util::codec::Decoder>::Item: std::marker::Send,
{
    // Should be default: `std::io::Error`.
    // Right now this is unstable: https://github.com/rust-lang/rust/issues/29661
    type Error: From<io::Error>
        + StreamDecodingError
        + std::fmt::Debug
        + std::fmt::Display
        + Send
        + Unpin;
    type Item: Into<SmallVec<[Event; 1]>> + Send + Unpin;
    type Decoder: Decoder<Item = (Self::Item, usize), Error = Self::Error> + Send + 'static;
    type Acker: TcpSourceAcker + Send;

    fn decoder(&self) -> Self::Decoder;

    fn handle_events(&self, _events: &mut [Event], _host: std::net::SocketAddr) {}

    fn build_acker(&self, item: &[Self::Item]) -> Self::Acker;

    #[allow(clippy::too_many_arguments)]
    fn run(
        self,
        addr: SocketListenAddr,
        keepalive: Option<TcpKeepaliveConfig>,
        shutdown_timeout_secs: u64,
        tls: MaybeTlsSettings,
        receive_buffer_bytes: Option<usize>,
        cx: SourceContext,
        acknowledgements: AcknowledgementsConfig,
        max_connections: Option<u32>,
    ) -> crate::Result<crate::sources::Source> {
        let acknowledgements = cx.do_acknowledgements(&acknowledgements);

        let listenfd = ListenFd::from_env();

        Ok(Box::pin(async move {
            let listener = match make_listener(addr, listenfd, &tls).await {
                None => return Err(()),
                Some(listener) => listener,
            };

            info!(
                message = "Listening.",
                addr = %listener
                    .local_addr()
                    .map(SocketListenAddr::SocketAddr)
                    .unwrap_or(addr)
            );

            let tripwire = cx.shutdown.clone();
            let tripwire = async move {
                let _ = tripwire.await;
                sleep(Duration::from_secs(shutdown_timeout_secs)).await;
            }
            .shared();

            let connection_gauge = OpenGauge::new();
            let shutdown_clone = cx.shutdown.clone();

            let request_limiter = RequestLimiter::new(MAX_IN_FLIGHT_EVENTS_TARGET, num_cpus::get());

            listener
                .accept_stream_limited(max_connections)
                .take_until(shutdown_clone)
                .for_each(move |(connection, tcp_connection_permit)| {
                    let shutdown_signal = cx.shutdown.clone();
                    let tripwire = tripwire.clone();
                    let source = self.clone();
                    let out = cx.out.clone();
                    let connection_gauge = connection_gauge.clone();
                    let request_limiter = request_limiter.clone();

                    async move {
                        let socket = match connection {
                            Ok(socket) => socket,
                            Err(error) => {
                                error!(
                                    message = "Failed to accept socket.",
                                    %error
                                );
                                return;
                            }
                        };

                        let peer_addr = socket.peer_addr();
                        let span = info_span!("connection", %peer_addr);

                        let tripwire = tripwire
                            .map(move |_| {
                                info!(
                                    message = "Resetting connection (still open after seconds).",
                                    seconds = ?shutdown_timeout_secs
                                );
                            })
                            .boxed();

                        span.clone().in_scope(|| {
                            debug!(message = "Accepted a new connection.", peer_addr = %peer_addr);

                            let open_token =
                                connection_gauge.open(|count| emit!(ConnectionOpen { count }));

                            let fut = handle_stream(
                                shutdown_signal,
                                socket,
                                keepalive,
                                receive_buffer_bytes,
                                source,
                                tripwire,
                                peer_addr,
                                out,
                                acknowledgements,
                                request_limiter,
                            );

                            tokio::spawn(
                                fut.map(move |()| {
                                    drop(open_token);
                                    drop(tcp_connection_permit);
                                })
                                .instrument(span.or_current()),
                            );
                        });
                    }
                })
                .map(Ok)
                .await
        }))
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_stream<T>(
    mut shutdown_signal: ShutdownSignal,
    mut socket: MaybeTlsIncomingStream<TcpStream>,
    keepalive: Option<TcpKeepaliveConfig>,
    receive_buffer_bytes: Option<usize>,
    source: T,
    mut tripwire: BoxFuture<'static, ()>,
    peer_addr: SocketAddr,
    mut out: SourceSender,
    acknowledgements: bool,
    request_limiter: RequestLimiter,
) where
    <<T as TcpSource>::Decoder as tokio_util::codec::Decoder>::Item: std::marker::Send,
    T: TcpSource,
{
    tokio::select! {
        result = socket.handshake() => {
            if let Err(error) = result {
                emit!(TcpSocketTlsConnectionError { error });
                return;
            }
        },
        _ = &mut shutdown_signal => {
            return;
        }
    };

    if let Some(keepalive) = keepalive {
        if let Err(error) = socket.set_keepalive(keepalive) {
            warn!(message = "Failed configuring TCP keepalive.", %error);
        }
    }

    if let Some(receive_buffer_bytes) = receive_buffer_bytes {
        if let Err(error) = socket.set_receive_buffer_bytes(receive_buffer_bytes) {
            warn!(message = "Failed configuring receive buffer size on TCP socket.", %error);
        }
    }

    let socket = socket.after_read(move |byte_size| {
        emit!(TcpBytesReceived {
            byte_size,
            peer_addr
        });
    });
    let reader = FramedRead::new(socket, source.decoder());
    let mut reader = ReadyFrames::new(reader);

    loop {
        let mut permit = tokio::select! {
            _ = &mut tripwire => break,
            _ = &mut shutdown_signal => {
                if close_socket(reader.get_ref().get_ref().get_ref()) {
                    break;
                }
                None
            },
            permit = request_limiter.acquire() => {
                Some(permit)
            }
            else => break,
        };

        let timeout = tokio::time::sleep(Duration::from_millis(10));
        tokio::pin!(timeout);

        tokio::select! {
            _ = &mut tripwire => break,
            _ = &mut shutdown_signal => {
                if close_socket(reader.get_ref().get_ref().get_ref()) {
                    break;
                }
            },
            _ = &mut timeout => {
                // This connection is currently holding a permit, but has not received data for some time. Release
                // the permit to let another connection try
                continue;
            }
            res = reader.next() => {
                match res {
                    Some(Ok((frames, _byte_size))) => {
                        let _num_frames = frames.len();
                        let acker = source.build_acker(&frames);
                        let (batch, receiver) = BatchNotifier::maybe_new_with_receiver(acknowledgements);


                        let mut events = frames.into_iter().flat_map(Into::into).collect::<Vec<Event>>();
                        let count = events.len();

                        emit!(SocketEventsReceived {
                            mode: SocketMode::Tcp,
                            byte_size: events.size_of(),
                            count,
                        });

                        if let Some(permit) = &mut permit {
                            // Note that this is intentionally not the "number of events in a single request", but rather
                            // the "number of events currently available". This may contain events from multiple events,
                            // but it should always contain all events from each request.
                            permit.decoding_finished(events.len());
                        }

                        if let Some(batch) = batch {
                            for event in &mut events {
                                event.add_batch_notifier(Arc::clone(&batch));
                            }
                        }

                        source.handle_events(&mut events, peer_addr);
                        match out.send_batch(events).await {
                            Ok(_) => {
                                let ack = match receiver {
                                    None => TcpSourceAck::Ack,
                                    Some(receiver) =>
                                        match receiver.await {
                                            BatchStatus::Delivered => TcpSourceAck::Ack,
                                            BatchStatus::Errored => {
                                                warn!(message = "Error delivering events to sink.",
                                                      internal_log_rate_secs = 5);
                                                TcpSourceAck::Error
                                            }
                                            BatchStatus::Rejected => {
                                                warn!(message = "Failed to deliver events to sink.",
                                                      internal_log_rate_secs = 5);
                                                TcpSourceAck::Reject
                                            }
                                        }
                                };
                                if let Some(ack_bytes) = acker.build_ack(ack){
                                    let stream = reader.get_mut().get_mut();
                                    if let Err(error) = stream.write_all(&ack_bytes).await {
                                        emit!(TcpSendAckError{ error });
                                        break;
                                    }
                                }
                                if ack != TcpSourceAck::Ack {
                                    break;
                                }
                            }
                            Err(error) => {
                                emit!(StreamClosedError { error, count });
                                break;
                            }
                        }
                    }
                    Some(Err(error)) => {
                        if !<<T as TcpSource>::Error as StreamDecodingError>::can_continue(&error) {
                            warn!(message = "Failed to read data from TCP source.", %error);
                            break;
                        }
                    }
                    None => {
                        debug!("Connection closed.");
                        break
                    },
                }
            }
            else => break,
        }

        drop(permit);
    }
}

fn close_socket(socket: &MaybeTlsIncomingStream<TcpStream>) -> bool {
    debug!("Start graceful shutdown.");
    // Close our write part of TCP socket to signal the other side
    // that it should stop writing and close the channel.
    if let Some(stream) = socket.get_ref() {
        let socket = SockRef::from(stream);
        if let Err(error) = socket.shutdown(std::net::Shutdown::Write) {
            warn!(message = "Failed in signalling to the other side to close the TCP channel.", %error);
        }
        false
    } else {
        // Connection hasn't yet been established so we are done here.
        debug!("Closing connection that hasn't yet been fully established.");
        true
    }
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

impl From<SocketListenAddr> for Resource {
    fn from(addr: SocketListenAddr) -> Resource {
        match addr {
            SocketListenAddr::SocketAddr(addr) => Resource::tcp(addr),
            SocketListenAddr::SystemdFd(offset) => Self::SystemFdOffset(offset),
        }
    }
}

fn parse_systemd_fd<'de, D>(des: D) -> Result<usize, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &'de str = Deserialize::deserialize(des)?;
    match s {
        "systemd" => Ok(0),
        s if s.starts_with("systemd#") => s[8..]
            .parse::<usize>()
            .map_err(de::Error::custom)?
            .checked_sub(1)
            .ok_or_else(|| de::Error::custom("systemd indices start from 1, found 0")),
        _ => Err(de::Error::custom("must start with \"systemd\"")),
    }
}

#[cfg(test)]
mod test {
    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

    use serde::Deserialize;

    use super::*;

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

pub(super) mod request_limiter;

use std::{io, mem::drop, net::SocketAddr, num::NonZeroU64, time::Duration};

use bytes::Bytes;
use futures::{FutureExt, StreamExt, future::BoxFuture};
use futures_util::future::OptionFuture;
use ipnet::IpNet;
use listenfd::ListenFd;
use smallvec::SmallVec;
use socket2::SockRef;
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
    time::sleep,
};
use tokio_util::codec::Decoder;
use tracing::Instrument;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::{ReadyFrames, StreamDecodingError, internal_events::DecoderFramingError},
    config::{LegacyKey, LogNamespace, SourceAcknowledgementsConfig},
    event::{BatchNotifier, BatchStatus, Event},
    finalization::AddBatchNotifier,
    internal_event::emit,
    lookup::{OwnedValuePath, path},
    shutdown::ShutdownSignal,
    source_sender::SourceSender,
    tcp::TcpKeepaliveConfig,
    tls::{
        CertificateMetadata, MaybeTlsIncomingStream, MaybeTlsListener, MaybeTlsSettings,
        TlsAcceptorReloader,
    },
};
use vrl::value::ObjectMap;

use self::request_limiter::RequestLimiter;
use super::SocketListenAddr;
use crate::{
    config::SourceContext,
    internal_events::{
        ConnectionOpen, OpenGauge, SocketBindError, SocketEventsReceived, SocketMode,
        SocketReceiveError, StreamClosedError, TcpBytesReceived, TcpSendAckError,
        TcpSocketTlsConnectionError, TcpSocketTlsHandshakeTimeout, TcpSourceConnectionClosed,
    },
    net::is_graceful_tls_shutdown,
    sources::SourceError,
    sources::util::{AfterReadExt, LenientFramedRead},
};

pub const MAX_IN_FLIGHT_EVENTS_TARGET: usize = 100_000;

///
/// # Errors
///
/// Returns an error if the address cannot be bound or the inherited listener cannot be configured.
pub async fn try_bind_tcp_listener(
    addr: SocketListenAddr,
    mut listenfd: ListenFd,
    tls: &MaybeTlsSettings,
    tls_reloader: Option<TlsAcceptorReloader>,
    allowlist: Option<Vec<IpNet>>,
) -> crate::Result<MaybeTlsListener> {
    match addr {
        SocketListenAddr::SocketAddr(addr) => tls
            .bind_reloadable(&addr, tls_reloader)
            .await
            .map_err(Into::into),
        SocketListenAddr::SystemdFd(offset) => match listenfd.take_tcp_listener(offset)? {
            Some(listener) => {
                listener.set_nonblocking(true)?;
                TcpListener::from_std(listener)
                    .map(Into::into)
                    .map_err(Into::into)
            }
            None => {
                Err(io::Error::new(io::ErrorKind::AddrInUse, "systemd fd already consumed").into())
            }
        },
    }
    .map(|listener| listener.with_allowlist(allowlist))
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

    ///
    /// # Errors
    ///
    /// Returns an error if TLS or the source decoder cannot be configured.
    #[allow(clippy::too_many_arguments)]
    fn run(
        self,
        addr: SocketListenAddr,
        keepalive: Option<TcpKeepaliveConfig>,
        shutdown_timeout_secs: Duration,
        tls: MaybeTlsSettings,
        tls_reloader: Option<TlsAcceptorReloader>,
        tls_client_metadata_key: Option<OwnedValuePath>,
        receive_buffer_bytes: Option<usize>,
        max_connection_duration_secs: Option<u64>,
        tls_handshake_timeout_secs: Option<NonZeroU64>,
        cx: SourceContext,
        acknowledgements: SourceAcknowledgementsConfig,
        max_connections: Option<u32>,
        allowlist: Option<Vec<IpNet>>,
        source_name: &'static str,
        log_namespace: LogNamespace,
    ) -> crate::Result<crate::sources::Source> {
        let acknowledgements = cx.do_acknowledgements(acknowledgements);

        Ok(Box::pin(async move {
            let listenfd = ListenFd::from_env();
            let listener = try_bind_tcp_listener(addr, listenfd, &tls, tls_reloader, allowlist)
                .await
                .map_err(|error| {
                    emit(SocketBindError {
                        mode: SocketMode::Tcp,
                        error: &error,
                    });
                    SourceError::from(error)
                })?;

            info!(
                message = "Listening.",
                addr = %listener
                    .local_addr()
                    .map_or(addr, SocketListenAddr::SocketAddr)
            );

            let tripwire = cx.shutdown.clone();
            let tripwire = async move {
                _ = tripwire.await;
                sleep(shutdown_timeout_secs).await;
            }
            .shared();

            let connection_gauge = OpenGauge::new();
            let shutdown_clone = cx.shutdown.clone();

            let request_limiter =
                RequestLimiter::new(MAX_IN_FLIGHT_EVENTS_TARGET, vector_lib::num_threads());

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
                    let tls_client_metadata_key = tls_client_metadata_key.clone();

                    async move {
                        let socket = match connection {
                            Ok(socket) => socket,
                            Err(error) => {
                                emit(SocketReceiveError {
                                    mode: SocketMode::Tcp,
                                    error: &error,
                                });
                                return;
                            }
                        };

                        let peer_addr = socket.peer_addr();
                        let span = info_span!("connection", %peer_addr);

                        let tripwire = tripwire
                            .map(move |()| {
                                info!(
                                    message = "Resetting connection (still open after seconds).",
                                    seconds = ?shutdown_timeout_secs
                                );
                            })
                            .boxed();

                        span.clone().in_scope(|| {
                            debug!(message = "Accepted a new connection.", peer_addr = %peer_addr);

                            let open_token =
                                connection_gauge.open(|count| emit(ConnectionOpen { count }));

                            let fut = handle_stream(
                                shutdown_signal,
                                socket,
                                keepalive,
                                receive_buffer_bytes,
                                max_connection_duration_secs,
                                tls_handshake_timeout_secs,
                                source,
                                tripwire,
                                peer_addr,
                                out,
                                acknowledgements,
                                request_limiter,
                                tls_client_metadata_key.clone(),
                                source_name,
                                log_namespace,
                            );

                            tokio::spawn(
                                fut.map(move |()| {
                                    drop(open_token);
                                    // Paired with the ConnectionOpen emit above:
                                    // fires exactly once per accepted connection,
                                    // including paths that return early from
                                    // handle_stream (TLS handshake failure,
                                    // shutdown during handshake).
                                    emit(TcpSourceConnectionClosed);
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

#[allow(
    clippy::too_many_lines,
    reason = "Splitting this source routine is deferred to keep the lint cleanup mechanical. https://github.com/vectordotdev/vector/issues/23659"
)]
#[allow(clippy::too_many_arguments)]
async fn handle_stream<T>(
    mut shutdown_signal: ShutdownSignal,
    mut socket: MaybeTlsIncomingStream<TcpStream>,
    keepalive: Option<TcpKeepaliveConfig>,
    receive_buffer_bytes: Option<usize>,
    max_connection_duration_secs: Option<u64>,
    tls_handshake_timeout_secs: Option<NonZeroU64>,
    source: T,
    mut tripwire: BoxFuture<'static, ()>,
    peer_addr: SocketAddr,
    mut out: SourceSender,
    acknowledgements: bool,
    request_limiter: RequestLimiter,
    tls_client_metadata_key: Option<OwnedValuePath>,
    source_name: &'static str,
    log_namespace: LogNamespace,
) where
    <<T as TcpSource>::Decoder as tokio_util::codec::Decoder>::Item: std::marker::Send,
    T: TcpSource,
{
    let handshake_timeout = OptionFuture::from(
        tls_handshake_timeout_secs.map(|secs| tokio::time::sleep(Duration::from_secs(secs.get()))),
    );
    tokio::pin!(handshake_timeout);

    tokio::select! {
        result = socket.handshake() => {
            if let Err(error) = result {
                emit(TcpSocketTlsConnectionError { error, peer_addr });
                return;
            }
        },
        Some(()) = &mut handshake_timeout => {
            emit(TcpSocketTlsHandshakeTimeout {
                peer_addr,
                timeout: Duration::from_secs(
                    tls_handshake_timeout_secs.map_or(0, NonZeroU64::get),
                ),
            });
            return;
        },
        _ = &mut shutdown_signal => {
            return;
        }
    };

    if let Some(keepalive) = keepalive
        && let Err(error) = socket.set_keepalive(keepalive)
    {
        warn!(message = "Failed configuring TCP keepalive.", %error);
    }

    if let Some(receive_buffer_bytes) = receive_buffer_bytes
        && let Err(error) = socket.set_receive_buffer_bytes(receive_buffer_bytes)
    {
        warn!(message = "Failed configuring receive buffer size on TCP socket.", %error);
    }

    let socket = socket.after_read(move |byte_size| {
        emit(TcpBytesReceived {
            byte_size,
            peer_addr,
        });
    });

    let certificate_metadata = socket
        .get_ref()
        .ssl_stream()
        .and_then(|stream| stream.ssl().peer_certificate())
        .map(CertificateMetadata::from);

    let reader = LenientFramedRead::new(socket, source.decoder());

    let mut reader = ReadyFrames::new(reader);

    let connection_close_timeout = OptionFuture::from(
        max_connection_duration_secs
            .map(|timeout_secs| tokio::time::sleep(Duration::from_secs(timeout_secs))),
    );

    tokio::pin!(connection_close_timeout);

    // Without TLS, nothing is buffered before the first read. With TLS, the first read does not
    // wait, in case the TLS layer kept data from the handshake.
    let mut before_first_read = reader.get_ref().get_ref().get_ref().ssl_stream().is_none();

    loop {
        // If the last read found no more data, wait until the socket has data before taking a
        // permit. Otherwise idle connections take turns holding the permits and starve the
        // connections that have data to read.
        let idle_socket = if before_first_read || reader.is_inner_pending() {
            reader.get_ref().get_ref().get_ref().get_ref()
        } else {
            None
        };
        let first_read = std::mem::take(&mut before_first_read);

        let mut permit = tokio::select! {
            () = &mut tripwire => break,
            Some(()) = &mut connection_close_timeout  => {
                if close_socket(reader.get_ref().get_ref().get_ref()) {
                    break;
                }
                None
            },
            _ = &mut shutdown_signal => {
                if close_socket(reader.get_ref().get_ref().get_ref()) {
                    break;
                }
                None
            },
            permit = async {
                if let Some(socket) = idle_socket {
                    if first_read {
                        // Tokio marks a newly accepted socket as readable before any data arrives,
                        // so the first wait uses `peek`, which returns once data or the end of the
                        // stream arrives and consumes no data.
                        _ = socket.peek(&mut [0; 1]).await;
                    } else {
                        // Later waits do not use `peek`, because it would clear a pending error
                        // (such as a reset) that the next read must see. An error from `readable`
                        // is also returned by the next read.
                        _ = socket.readable().await;
                    }
                }
                request_limiter.acquire().await
            } => {
                Some(permit)
            }
            else => break,
        };

        let timeout = tokio::time::sleep(Duration::from_millis(10));
        tokio::pin!(timeout);

        tokio::select! {
            () = &mut tripwire => break,
            _ = &mut shutdown_signal => {
                if close_socket(reader.get_ref().get_ref().get_ref()) {
                    break;
                }
            },
            () = &mut timeout => {
                // This connection is currently holding a permit, but has not received data for some time. Release
                // the permit to let another connection try. The next iteration waits for new data before it takes a
                // permit again.
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

                        emit(SocketEventsReceived {
                            mode: SocketMode::Tcp,
                            byte_size: events.estimated_json_encoded_size_of(),
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
                                event.add_batch_notifier(batch.clone());
                            }
                        }


                        if let Some(certificate_metadata) = &certificate_metadata {
                            let mut metadata = ObjectMap::new();
                            metadata.insert("subject".into(), certificate_metadata.subject().into());
                            for event in &mut events {
                                let log = event.as_mut_log();

                                log_namespace.insert_source_metadata(
                                    source_name,
                                    log,
                                    tls_client_metadata_key.as_ref().map(LegacyKey::Overwrite),
                                    path!("tls_client_metadata"),
                                    metadata.clone()
                                );
                            }
                        }

                        source.handle_events(&mut events, peer_addr);
                        if out.send_batch(events).await.is_ok() {
                            let ack = match receiver {
                                None => TcpSourceAck::Ack,
                                Some(receiver) =>
                                    match receiver.await {
                                        BatchStatus::Delivered => TcpSourceAck::Ack,
                                        BatchStatus::Errored => {TcpSourceAck::Error},
                                        BatchStatus::Rejected => {
                                            // Sinks are responsible for emitting ComponentEventsDropped.
                                            TcpSourceAck::Reject
                                        }
                                    }
                            };
                            if let Some(ack_bytes) = acker.build_ack(ack){
                                let stream = reader.get_mut().get_mut();
                                if let Err(error) = stream.write_all(&ack_bytes).await {
                                    // Per spec, `*Error` events MUST only be
                                    // emitted on real errors. A peer-initiated
                                    // graceful TLS shutdown during the ack
                                    // write is a lifecycle event, not an error
                                    // — log at warn and skip the emit.
                                    if is_graceful_tls_shutdown(&error) {
                                        warn!(
                                            message = "Connection closed by peer before acknowledgement could be sent.",
                                            error = %error,
                                        );
                                    } else {
                                        emit(TcpSendAckError { error, peer_addr });
                                    }
                                    break;
                                }
                            }
                            if ack != TcpSourceAck::Ack {
                                break;
                            }
                        } else {
                            emit(StreamClosedError { count });
                            break;
                        }
                    }
                    Some(Err(error)) => {
                        if !<<T as TcpSource>::Error as StreamDecodingError>::can_continue(&error) {
                            emit(DecoderFramingError { error });
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

#[cfg(test)]
mod tests {
    use std::{
        net::SocketAddr,
        pin::Pin,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use bytes::{Bytes, BytesMut};
    use futures::StreamExt;
    use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
    use smallvec::SmallVec;
    use tokio::{
        io::{AsyncReadExt, AsyncWrite, AsyncWriteExt},
        net::TcpStream,
        task::JoinHandle,
        time::{Instant, sleep, timeout},
    };
    use tokio_util::codec::Decoder;
    use vector_lib::{
        config::{LogNamespace, SourceAcknowledgementsConfig},
        event::{Event, EventStatus},
        source_sender::SourceSender,
        tls::{MaybeTlsSettings, TlsEnableableConfig},
    };

    use super::{SocketListenAddr, TcpSource, TcpSourceAck, TcpSourceAcker};
    use crate::{
        config::{ComponentKey, SourceContext},
        sources::SourceError,
        test_util::{addr::next_addr, wait_for_tcp},
    };

    /// A newline-delimited TCP source that counts the frames it decodes.
    #[derive(Clone, Default)]
    struct CountingSource {
        decoded: Arc<AtomicUsize>,
        send_acks: bool,
    }

    #[derive(Clone)]
    struct CountingDecoder {
        inner: vector_lib::codecs::Decoder,
        decoded: Arc<AtomicUsize>,
    }

    impl CountingDecoder {
        fn count<T>(&self, frame: Option<T>) -> Option<T> {
            if frame.is_some() {
                self.decoded.fetch_add(1, Ordering::Relaxed);
            }
            frame
        }
    }

    impl Decoder for CountingDecoder {
        type Item = <vector_lib::codecs::Decoder as Decoder>::Item;
        type Error = <vector_lib::codecs::Decoder as Decoder>::Error;

        fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
            let frame = self.inner.decode(src)?;
            Ok(self.count(frame))
        }

        fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
            let frame = self.inner.decode_eof(src)?;
            Ok(self.count(frame))
        }
    }

    /// Answers each delivered batch with `ack\n` when enabled, like the `fluent` and `logstash`
    /// protocols answer each request.
    struct LineAcker(bool);

    impl TcpSourceAcker for LineAcker {
        fn build_ack(self, ack: TcpSourceAck) -> Option<Bytes> {
            (self.0 && ack == TcpSourceAck::Ack).then(|| Bytes::from_static(b"ack\n"))
        }
    }

    impl TcpSource for CountingSource {
        type Error = vector_lib::codecs::decoding::Error;
        type Item = SmallVec<[Event; 1]>;
        type Decoder = CountingDecoder;
        type Acker = LineAcker;

        fn decoder(&self) -> Self::Decoder {
            CountingDecoder {
                inner: vector_lib::codecs::Decoder::default(),
                decoded: Arc::clone(&self.decoded),
            }
        }

        fn build_acker(&self, _item: &[Self::Item]) -> Self::Acker {
            LineAcker(self.send_acks)
        }
    }

    async fn start_source(
        source: CountingSource,
        tls: bool,
        cx: SourceContext,
    ) -> (SocketAddr, JoinHandle<Result<(), SourceError>>) {
        let tls = if tls {
            MaybeTlsSettings::from_config(Some(&TlsEnableableConfig::test_config()), true).unwrap()
        } else {
            MaybeTlsSettings::Raw(())
        };
        let acknowledgements = SourceAcknowledgementsConfig::from(source.send_acks);
        let (guard, addr) = next_addr();
        let server = source
            .run(
                SocketListenAddr::SocketAddr(addr),
                None,
                Duration::from_secs(1),
                tls,
                None,
                None,
                None,
                None,
                None,
                cx,
                acknowledgements,
                None,
                None,
                "test",
                LogNamespace::Legacy,
            )
            .unwrap();
        let handle = tokio::spawn(server);
        wait_for_tcp(addr).await;
        drop(guard);
        (addr, handle)
    }

    type Client = Pin<Box<dyn AsyncWrite + Send>>;

    async fn connect(addr: SocketAddr, tls: bool) -> Client {
        let stream = TcpStream::connect(addr).await.unwrap();
        if !tls {
            return Box::pin(stream);
        }
        let mut connector = SslConnector::builder(SslMethod::tls()).unwrap();
        connector.set_verify(SslVerifyMode::NONE);
        let ssl = connector
            .build()
            .configure()
            .unwrap()
            .into_ssl("localhost")
            .unwrap();
        let mut stream = tokio_openssl::SslStream::new(ssl, stream).unwrap();
        Pin::new(&mut stream).connect().await.unwrap();
        Box::pin(stream)
    }

    async fn assert_idle_connections_do_not_starve_busy_connection(tls: bool) {
        const IDLE_CONNECTIONS: usize = 64;
        const LINES: usize = 30;

        let (tx, mut rx) = SourceSender::new_test();
        let (addr, _source) = start_source(
            CountingSource::default(),
            tls,
            SourceContext::new_test(tx, None),
        )
        .await;

        let mut idle = Vec::with_capacity(IDLE_CONNECTIONS);
        for _ in 0..IDLE_CONNECTIONS {
            idle.push(connect(addr, tls).await);
        }

        let mut busy = connect(addr, tls).await;
        busy.write_all(b"first\n").await.unwrap();
        timeout(Duration::from_secs(10), rx.next())
            .await
            .expect("first line was not received")
            .unwrap();

        // Each line is sent after the previous event arrived, so each line needs a new permit.
        timeout(Duration::from_secs(1), async {
            for i in 0..LINES {
                busy.write_all(format!("line {i}\n").as_bytes())
                    .await
                    .unwrap();
                rx.next().await.unwrap();
            }
        })
        .await
        .expect("idle connections starved the busy connection");
    }

    #[tokio::test]
    async fn idle_connections_do_not_starve_busy_connection() {
        assert_idle_connections_do_not_starve_busy_connection(false).await;
    }

    #[tokio::test]
    async fn idle_tls_connections_do_not_starve_busy_connection() {
        assert_idle_connections_do_not_starve_busy_connection(true).await;
    }

    #[tokio::test]
    async fn new_idle_connections_do_not_delay_busy_connection() {
        const IDLE_CONNECTIONS: usize = 64;

        let (tx, mut rx) = SourceSender::new_test();
        let (addr, _source) = start_source(
            CountingSource::default(),
            false,
            SourceContext::new_test(tx, None),
        )
        .await;

        // These connections were just opened and send nothing.
        let mut idle = Vec::with_capacity(IDLE_CONNECTIONS);
        for _ in 0..IDLE_CONNECTIONS {
            idle.push(TcpStream::connect(addr).await.unwrap());
        }

        let mut busy = TcpStream::connect(addr).await.unwrap();
        busy.write_all(b"line\n").await.unwrap();
        timeout(Duration::from_millis(150), rx.next())
            .await
            .expect("new idle connections delayed the busy connection")
            .unwrap();
    }

    async fn assert_reads_buffered_frames(tls: bool) {
        // More frames than fit in one `ReadyFrames` batch, in a single small write, so that one
        // read from the socket returns several batches.
        const LINES: usize = 3_000;

        let (tx, rx) = SourceSender::new_test();
        let (addr, _source) = start_source(
            CountingSource::default(),
            tls,
            SourceContext::new_test(tx, None),
        )
        .await;

        let mut client = connect(addr, tls).await;
        client.write_all(&b"x\n".repeat(LINES)).await.unwrap();
        client.flush().await.unwrap();

        // The client keeps the connection open and sends nothing else, so the socket does not
        // become readable again. Frames left in the read buffers must still be read.
        let events = timeout(Duration::from_secs(10), rx.take(LINES).collect::<Vec<_>>())
            .await
            .expect("buffered frames were not read");
        assert_eq!(events.len(), LINES);
        drop(client);
    }

    #[tokio::test]
    async fn reads_buffered_frames_without_new_data() {
        assert_reads_buffered_frames(false).await;
    }

    #[tokio::test]
    async fn reads_buffered_tls_frames_without_new_data() {
        assert_reads_buffered_frames(true).await;
    }

    #[tokio::test]
    async fn decoding_stays_limited_under_backpressure() {
        const OUTPUT_BUFFER: usize = 1;

        // The limiter never has fewer than two permits.
        let max_permits = vector_lib::num_threads().max(2);
        let connections = OUTPUT_BUFFER + max_permits + 16;

        let source = CountingSource::default();
        let decoded = Arc::clone(&source.decoded);
        // The output is never read, so the source stays under backpressure.
        let (tx, _rx) = SourceSender::new_test_sender_with_options(OUTPUT_BUFFER, None);
        let (addr, _source) = start_source(source, false, SourceContext::new_test(tx, None)).await;

        let mut clients = Vec::with_capacity(connections);
        for _ in 0..connections {
            let mut client = connect(addr, false).await;
            client.write_all(b"event\n").await.unwrap();
            clients.push(client);
        }
        sleep(Duration::from_secs(1)).await;

        // Only connections that hold a permit may decode, so the decoded frames fit in the
        // output buffer plus one frame per permit.
        let decoded = decoded.load(Ordering::Relaxed);
        assert!(decoded > 0, "no frame was decoded");
        assert!(
            decoded <= OUTPUT_BUFFER + max_permits,
            "decoded {decoded} frames from {connections} connections with at most {max_permits} permits"
        );
    }

    #[tokio::test]
    async fn acknowledges_batches_after_connection_was_idle() {
        let source = CountingSource {
            send_acks: true,
            ..Default::default()
        };
        let (tx, mut rx) = SourceSender::new_test_finalize(EventStatus::Delivered);
        tokio::spawn(async move { while rx.next().await.is_some() {} });
        let (addr, _source) = start_source(source, false, SourceContext::new_test(tx, None)).await;

        let mut stream = TcpStream::connect(addr).await.unwrap();
        for i in 0..3 {
            // Let the connection go idle before each line.
            sleep(Duration::from_millis(50)).await;
            stream
                .write_all(format!("line {i}\n").as_bytes())
                .await
                .unwrap();
            let mut ack = [0; 4];
            timeout(Duration::from_secs(5), stream.read_exact(&mut ack))
                .await
                .expect("batch was not acknowledged")
                .unwrap();
            assert_eq!(&ack, b"ack\n");
        }
    }

    #[tokio::test]
    async fn closes_idle_connection_when_client_closes() {
        let (tx, _rx) = SourceSender::new_test();
        let (addr, _source) = start_source(
            CountingSource::default(),
            false,
            SourceContext::new_test(tx, None),
        )
        .await;

        let mut stream = TcpStream::connect(addr).await.unwrap();
        // Let the connection go idle.
        sleep(Duration::from_millis(100)).await;
        stream.shutdown().await.unwrap();

        // The source reads the end of the stream and closes the connection.
        let read = timeout(Duration::from_secs(5), stream.read(&mut [0; 1]))
            .await
            .expect("connection was not closed");
        assert_eq!(read.unwrap(), 0);
    }

    #[tokio::test]
    async fn drops_partial_frame_when_idle_connection_is_reset() {
        let (tx, mut rx) = SourceSender::new_test();
        let (addr, _source) = start_source(
            CountingSource::default(),
            false,
            SourceContext::new_test(tx, None),
        )
        .await;

        let mut stream = TcpStream::connect(addr).await.unwrap();
        stream.write_all(b"partial").await.unwrap();
        // Let the source read the partial frame and go idle, then reset the connection.
        sleep(Duration::from_millis(100)).await;
        socket2::SockRef::from(&stream)
            .set_linger(Some(Duration::ZERO))
            .unwrap();
        drop(stream);

        // The reset is reported as an error, so the partial frame is not decoded as an event.
        assert!(
            timeout(Duration::from_millis(500), rx.next())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn shuts_down_with_idle_connections() {
        let key = ComponentKey::from("shuts_down_with_idle_connections");
        let (tx, _rx) = SourceSender::new_test();
        let (cx, mut shutdown) = SourceContext::new_shutdown(&key, tx);
        let (addr, source) = start_source(CountingSource::default(), false, cx).await;

        let mut idle = Vec::new();
        for _ in 0..8 {
            idle.push(TcpStream::connect(addr).await.unwrap());
        }
        // Let the connections go idle.
        sleep(Duration::from_millis(100)).await;

        let deadline = Instant::now() + Duration::from_secs(10);
        let shutdown_complete = shutdown.shutdown_source(&key, deadline);

        // On shutdown the source closes the write half of each connection right away, well
        // before `shutdown_timeout_secs` (one second here) forces the connections closed.
        for mut stream in idle {
            let read = timeout(Duration::from_millis(500), stream.read(&mut [0; 1]))
                .await
                .expect("connection was not closed on shutdown");
            assert_eq!(read.unwrap(), 0);
        }
        assert!(shutdown_complete.await);
        source.await.unwrap().unwrap();
    }
}

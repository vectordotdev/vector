#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
    convert::TryInto,
    fs,
    marker::{Send, Sync},
    net::SocketAddr,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use bytes::{Buf, Bytes, BytesMut};
use futures::{
    executor::block_on,
    future::{self, OptionFuture},
    sink::{Sink, SinkExt},
    stream::{self, StreamExt, TryStreamExt},
};
use futures_util::{Future, FutureExt, future::BoxFuture};
use ipnet::IpNet;
use listenfd::ListenFd;
use tokio::{
    self,
    io::{AsyncRead, AsyncWrite},
    net::{TcpStream, UnixListener},
    task::JoinHandle,
    time::sleep,
};
use tokio_stream::wrappers::UnixListenerStream;
use tokio_util::codec::{Framed, length_delimited};
use tracing::{Instrument, Span, field};
use vector_lib::{
    lookup::OwnedValuePath,
    tcp::TcpKeepaliveConfig,
    tls::{CertificateMetadata, MaybeTlsIncomingStream, MaybeTlsSettings},
};

use super::net::{RequestLimiter, SocketListenAddr};
use crate::{
    SourceSender,
    event::Event,
    internal_events::{
        ConnectionOpen, OpenGauge, SocketBindError, SocketMode, SocketReceiveError,
        TcpBytesReceived, TcpSocketError, TcpSocketTlsConnectionError, UnixSocketError,
        UnixSocketFileDeleteError,
    },
    shutdown::ShutdownSignal,
    sources::{
        Source,
        util::{
            AfterReadExt,
            net::{MAX_IN_FLIGHT_EVENTS_TARGET, try_bind_tcp_listener},
        },
    },
};

const FSTRM_CONTROL_FRAME_LENGTH_MAX: usize = 512;
const FSTRM_CONTROL_FIELD_CONTENT_TYPE_LENGTH_MAX: usize = 256;

/// If a connection does not receive any data during this short timeout,
/// it should release its permit (and try to obtain a new one) allowing other connections to read.
/// It is very short because any incoming data will avoid this timeout,
/// so it mainly prevents holding permits without consuming any data
const PERMIT_HOLD_TIMEOUT_MS: u64 = 10;

pub type FrameStreamSink = Box<dyn Sink<Bytes, Error = std::io::Error> + Send + Unpin>;

pub struct FrameStreamReader {
    response_sink: Mutex<FrameStreamSink>,
    expected_content_type: String,
    state: FrameStreamState,
}

struct FrameStreamState {
    expect_control_frame: bool,
    control_state: ControlState,
    is_bidirectional: bool,
}
impl FrameStreamState {
    const fn new() -> Self {
        FrameStreamState {
            expect_control_frame: false,
            //first control frame should be READY (if bidirectional -- if unidirectional first will be START)
            control_state: ControlState::Initial,
            is_bidirectional: true, //assume
        }
    }
}

#[derive(PartialEq, Debug)]
enum ControlState {
    Initial,
    GotReady,
    ReadingData,
    Stopped,
}

#[derive(Copy, Clone)]
enum ControlHeader {
    Accept,
    Start,
    Stop,
    Ready,
    Finish,
}

impl ControlHeader {
    fn from_u32(val: u32) -> Result<Self, ()> {
        match val {
            0x01 => Ok(ControlHeader::Accept),
            0x02 => Ok(ControlHeader::Start),
            0x03 => Ok(ControlHeader::Stop),
            0x04 => Ok(ControlHeader::Ready),
            0x05 => Ok(ControlHeader::Finish),
            _ => {
                error!("Don't know header value {} (expected 0x01 - 0x05).", val);
                Err(())
            }
        }
    }

    const fn to_u32(self) -> u32 {
        match self {
            ControlHeader::Accept => 0x01,
            ControlHeader::Start => 0x02,
            ControlHeader::Stop => 0x03,
            ControlHeader::Ready => 0x04,
            ControlHeader::Finish => 0x05,
        }
    }
}

enum ControlField {
    ContentType,
}

impl ControlField {
    fn from_u32(val: u32) -> Result<Self, ()> {
        match val {
            0x01 => Ok(ControlField::ContentType),
            _ => {
                error!("Don't know field type {} (expected 0x01).", val);
                Err(())
            }
        }
    }
    const fn to_u32(&self) -> u32 {
        match self {
            ControlField::ContentType => 0x01,
        }
    }
}

fn advance_u32(b: &mut Bytes) -> Result<u32, ()> {
    if b.len() < 4 {
        error!("Malformed frame.");
        return Err(());
    }
    let a = b.split_to(4);
    Ok(u32::from_be_bytes(a[..].try_into().unwrap()))
}

impl FrameStreamReader {
    pub fn new(response_sink: FrameStreamSink, expected_content_type: String) -> Self {
        FrameStreamReader {
            response_sink: Mutex::new(response_sink),
            expected_content_type,
            state: FrameStreamState::new(),
        }
    }

    pub fn handle_frame(&mut self, frame: Bytes) -> Option<Bytes> {
        if frame.is_empty() {
            //frame length of zero means the next frame is a control frame
            self.state.expect_control_frame = true;
            None
        } else if self.state.expect_control_frame {
            self.state.expect_control_frame = false;
            _ = self.handle_control_frame(frame);
            None
        } else {
            //data frame
            if self.state.control_state == ControlState::ReadingData {
                Some(frame) //return data frame
            } else {
                error!(
                    "Received a data frame while in state {:?}.",
                    self.state.control_state
                );
                None
            }
        }
    }

    fn handle_control_frame(&mut self, mut frame: Bytes) -> Result<(), ()> {
        //enforce maximum control frame size
        if frame.len() > FSTRM_CONTROL_FRAME_LENGTH_MAX {
            error!("Control frame is too long.");
        }

        let header = ControlHeader::from_u32(advance_u32(&mut frame)?)?;

        //match current state to received header
        match self.state.control_state {
            ControlState::Initial => {
                match header {
                    ControlHeader::Ready => {
                        let content_type = self.process_fields(header, &mut frame)?.unwrap();

                        self.send_control_frame(Self::make_frame(
                            ControlHeader::Accept,
                            Some(content_type),
                        ));
                        self.state.control_state = ControlState::GotReady; //waiting for a START control frame
                    }
                    ControlHeader::Start => {
                        //check for content type
                        _ = self.process_fields(header, &mut frame)?;
                        //if didn't error, then we are ok to change state
                        self.state.control_state = ControlState::ReadingData;
                        self.state.is_bidirectional = false; //if first message was START then we are unidirectional (no responses)
                    }
                    _ => error!("Got wrong control frame, expected READY."),
                }
            }
            ControlState::GotReady => {
                match header {
                    ControlHeader::Start => {
                        //check for content type
                        _ = self.process_fields(header, &mut frame)?;
                        //if didn't error, then we are ok to change state
                        self.state.control_state = ControlState::ReadingData;
                    }
                    _ => error!("Got wrong control frame, expected START."),
                }
            }
            ControlState::ReadingData => {
                match header {
                    ControlHeader::Stop => {
                        //check there aren't any fields
                        _ = self.process_fields(header, &mut frame)?;
                        if self.state.is_bidirectional {
                            //send FINISH frame -- but only if we are bidirectional
                            self.send_control_frame(Self::make_frame(ControlHeader::Finish, None));
                        }
                        self.state.control_state = ControlState::Stopped; //stream is now done
                    }
                    _ => error!("Got wrong control frame, expected STOP."),
                }
            }
            ControlState::Stopped => error!("Unexpected control frame, current state is STOPPED."),
        };
        Ok(())
    }

    fn process_fields(
        &mut self,
        header: ControlHeader,
        frame: &mut Bytes,
    ) -> Result<Option<String>, ()> {
        match header {
            ControlHeader::Ready => {
                //should provide 1+ content types
                //should match expected content type
                let is_start_frame = false;
                let content_type = self.process_content_type(frame, is_start_frame)?;
                Ok(Some(content_type))
            }
            ControlHeader::Start => {
                //can take one or zero content types
                if frame.is_empty() {
                    Ok(None)
                } else {
                    //should match expected content type
                    let is_start_frame = true;
                    let content_type = self.process_content_type(frame, is_start_frame)?;
                    Ok(Some(content_type))
                }
            }
            ControlHeader::Stop => {
                //check that there are no fields
                if !frame.is_empty() {
                    error!("Unexpected fields in STOP header.");
                    Err(())
                } else {
                    Ok(None)
                }
            }
            _ => {
                error!("Unexpected control header value {:?}.", header.to_u32());
                Err(())
            }
        }
    }

    fn process_content_type(&self, frame: &mut Bytes, is_start_frame: bool) -> Result<String, ()> {
        if frame.is_empty() {
            error!("No fields in control frame.");
            return Err(());
        }

        let mut content_types = vec![];
        while !frame.is_empty() {
            //4 bytes of ControlField
            let field_val = advance_u32(frame)?;
            let field_type = ControlField::from_u32(field_val)?;
            match field_type {
                ControlField::ContentType => {
                    //4 bytes giving length of content type
                    let field_len = advance_u32(frame)? as usize;

                    //enforce limit on content type string
                    if field_len > FSTRM_CONTROL_FIELD_CONTENT_TYPE_LENGTH_MAX {
                        error!("Content-Type string is too long.");
                        return Err(());
                    }

                    let content_type = std::str::from_utf8(&frame[..field_len]).unwrap();
                    content_types.push(content_type.to_string());
                    frame.advance(field_len);
                }
            }
        }

        if is_start_frame && content_types.len() > 1 {
            error!(
                "START control frame can only have one content-type provided (got {}).",
                content_types.len()
            );
            return Err(());
        }

        for content_type in &content_types {
            if *content_type == self.expected_content_type {
                return Ok(content_type.clone());
            }
        }

        error!(
            "Content types did not match up. Expected {} got {:?}.",
            self.expected_content_type, content_types
        );
        Err(())
    }

    fn make_frame(header: ControlHeader, content_type: Option<String>) -> Bytes {
        let mut frame = BytesMut::new();
        frame.extend(header.to_u32().to_be_bytes());
        if let Some(s) = content_type {
            frame.extend(ControlField::ContentType.to_u32().to_be_bytes()); //field type: ContentType
            frame.extend((s.len() as u32).to_be_bytes()); //length of type
            frame.extend(s.as_bytes());
        }
        Bytes::from(frame)
    }

    fn send_control_frame(&mut self, frame: Bytes) {
        let empty_frame = Bytes::from(&b""[..]); //send empty frame to say we are control frame
        let mut stream = stream::iter(vec![Ok(empty_frame), Ok(frame)]);

        if let Err(e) = block_on(self.response_sink.lock().unwrap().send_all(&mut stream)) {
            error!("Encountered error '{:#?}' while sending control frame.", e);
        }
    }
}

pub trait FrameHandler {
    fn content_type(&self) -> String;
    fn max_frame_length(&self) -> usize;
    fn handle_event(&self, received_from: Option<Bytes>, frame: Bytes) -> Option<Event>;
    fn multithreaded(&self) -> bool;
    fn max_frame_handling_tasks(&self) -> usize;
    fn host_key(&self) -> &Option<OwnedValuePath>;
    fn timestamp_key(&self) -> Option<&OwnedValuePath>;
    fn source_type_key(&self) -> Option<&OwnedValuePath>;
}

pub trait UnixFrameHandler: FrameHandler {
    fn socket_path(&self) -> PathBuf;
    fn socket_file_mode(&self) -> Option<u32>;
    fn socket_receive_buffer_size(&self) -> Option<usize>;
    fn socket_send_buffer_size(&self) -> Option<usize>;
}

pub trait TcpFrameHandler: FrameHandler {
    fn address(&self) -> SocketListenAddr;
    fn keepalive(&self) -> Option<TcpKeepaliveConfig>;
    fn shutdown_timeout_secs(&self) -> Duration;
    fn tls(&self) -> MaybeTlsSettings;
    fn tls_client_metadata_key(&self) -> Option<OwnedValuePath>;
    fn receive_buffer_bytes(&self) -> Option<usize>;
    fn max_connection_duration_secs(&self) -> Option<u64>;
    fn max_connections(&self) -> Option<u32>;
    fn allowed_origins(&self) -> Option<&[IpNet]>;
    fn insert_tls_client_metadata(&mut self, metadata: Option<CertificateMetadata>);
}

/**
 * Based off of the build_framestream_unix_source function.
 * Functions similarly, just uses TCP socket instead of unix socket
 **/
pub fn build_framestream_tcp_source(
    frame_handler: impl TcpFrameHandler + Send + Sync + Clone + 'static,
    shutdown: ShutdownSignal,
    out: SourceSender,
) -> crate::Result<Source> {
    let addr = frame_handler.address();
    let tls = frame_handler.tls();
    let shutdown = shutdown.clone();
    let out = out.clone();

    Ok(Box::pin(async move {
        let listenfd = ListenFd::from_env();
        let listener = try_bind_tcp_listener(
            addr,
            listenfd,
            &tls,
            None, // tls_reloader: not wired for this source
            frame_handler
                .allowed_origins()
                .map(|origins| origins.to_vec()),
        )
        .await
        .map_err(|error| {
            emit!(SocketBindError {
                mode: SocketMode::Tcp,
                error: &error,
            })
        })?;

        info!(
            message = "Listening.",
            addr = %listener
                .local_addr()
                .map(SocketListenAddr::SocketAddr)
                .unwrap_or(addr)
        );

        let tripwire = shutdown.clone();
        let shutdown_timeout_secs = frame_handler.shutdown_timeout_secs();
        let tripwire = async move {
            _ = tripwire.await;
            sleep(shutdown_timeout_secs).await;
        }
        .shared();

        let connection_gauge = OpenGauge::new();
        let shutdown_clone = shutdown.clone();

        let request_limiter = RequestLimiter::new(
            MAX_IN_FLIGHT_EVENTS_TARGET,
            frame_handler.max_frame_handling_tasks(),
        );

        listener
            .accept_stream_limited(frame_handler.max_connections())
            .take_until(shutdown_clone)
            .for_each(move |(connection, tcp_connection_permit)| {
                let shutdown_signal = shutdown.clone();
                let tripwire = tripwire.clone();
                let out = out.clone();
                let connection_gauge = connection_gauge.clone();
                let request_limiter = request_limiter.clone();
                let frame_handler_clone = frame_handler.clone();

                async move {
                    let socket = match connection {
                        Ok(socket) => socket,
                        Err(error) => {
                            emit!(SocketReceiveError {
                                mode: SocketMode::Tcp,
                                error: &error
                            });
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
                            frame_handler_clone,
                            shutdown_signal,
                            socket,
                            tripwire,
                            peer_addr,
                            out,
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

#[allow(clippy::too_many_arguments)]
async fn handle_stream(
    mut frame_handler: impl TcpFrameHandler + Send + Sync + Clone + 'static,
    mut shutdown_signal: ShutdownSignal,
    mut socket: MaybeTlsIncomingStream<TcpStream>,
    mut tripwire: BoxFuture<'static, ()>,
    peer_addr: SocketAddr,
    out: SourceSender,
    request_limiter: RequestLimiter,
) {
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

    if let Some(keepalive) = frame_handler.keepalive()
        && let Err(error) = socket.set_keepalive(keepalive)
    {
        warn!(message = "Failed configuring TCP keepalive.", %error);
    }

    if let Some(receive_buffer_bytes) = frame_handler.receive_buffer_bytes()
        && let Err(error) = socket.set_receive_buffer_bytes(receive_buffer_bytes)
    {
        warn!(message = "Failed configuring receive buffer size on TCP socket.", %error);
    }

    let socket = socket.after_read(move |byte_size| {
        emit!(TcpBytesReceived {
            byte_size,
            peer_addr
        });
    });

    let certificate_metadata = socket
        .get_ref()
        .ssl_stream()
        .and_then(|stream| stream.ssl().peer_certificate())
        .map(CertificateMetadata::from);

    frame_handler.insert_tls_client_metadata(certificate_metadata);

    let span = info_span!("connection");
    span.record("peer_addr", field::debug(&peer_addr));
    let received_from: Option<Bytes> = Some(peer_addr.to_string().into());

    let connection_close_timeout = OptionFuture::from(
        frame_handler
            .max_connection_duration_secs()
            .map(|timeout_secs| tokio::time::sleep(Duration::from_secs(timeout_secs))),
    );
    tokio::pin!(connection_close_timeout);

    let content_type = frame_handler.content_type();
    let mut event_sink = out.clone();
    let (sock_sink, sock_stream) = Framed::new(
        socket,
        length_delimited::Builder::new()
            .max_frame_length(frame_handler.max_frame_length())
            .new_codec(),
    )
    .split();
    let mut reader = FrameStreamReader::new(Box::new(sock_sink), content_type);
    let mut frames = sock_stream
        .map_err(move |error| {
            emit!(TcpSocketError {
                error: &error,
                peer_addr,
            });
        })
        .filter_map(move |frame| {
            future::ready(match frame {
                Ok(f) => reader.handle_frame(Bytes::from(f)),
                Err(_) => None,
            })
        });

    let active_parsing_task_nums = Arc::new(AtomicUsize::new(0));
    loop {
        let mut permit = tokio::select! {
            _ = &mut tripwire => break,
            Some(_) = &mut connection_close_timeout  => {
                break;
            },
            _ = &mut shutdown_signal => {
                break;
            },
            permit = request_limiter.acquire() => {
                Some(permit)
            }
            else => break,
        };

        let timeout = tokio::time::sleep(Duration::from_millis(PERMIT_HOLD_TIMEOUT_MS));
        tokio::pin!(timeout);

        tokio::select! {
            _ = &mut tripwire => break,
            _ = &mut shutdown_signal => break,
            _ = &mut timeout => {
                // This connection is currently holding a permit, but has not received data for some time. Release
                // the permit to let another connection try
                continue;
            }
            res = frames.next() => {
                match res {
                    Some(frame) => {
                        if let Some(permit) = &mut permit {
                            // Note that this is intentionally not the "number of events in a single request", but rather
                            // the "number of events currently available". This may contain events from multiple events,
                            // but it should always contain all events from each request.
                            permit.decoding_finished(1);
                        };
                        handle_tcp_frame(&mut frame_handler, frame, &mut event_sink, received_from.clone(), Arc::clone(&active_parsing_task_nums)).await;
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

async fn handle_tcp_frame<T>(
    frame_handler: &mut T,
    frame: Bytes,
    event_sink: &mut SourceSender,
    received_from: Option<Bytes>,
    active_parsing_task_nums: Arc<AtomicUsize>,
) where
    T: TcpFrameHandler + Send + Sync + Clone + 'static,
{
    if frame_handler.multithreaded() {
        spawn_event_handling_tasks(
            frame,
            frame_handler.clone(),
            event_sink.clone(),
            received_from,
            active_parsing_task_nums,
            frame_handler.max_frame_handling_tasks(),
        )
        .await;
    } else if let Some(event) = frame_handler.handle_event(received_from, frame)
        && let Err(e) = event_sink.send_event(event).await
    {
        error!("Error sending event: {e:?}.");
    }
}

/**
 * Based off of the build_unix_source function.
 * Functions similarly, but uses the FrameStreamReader to deal with
 * framestream control packets, and responds appropriately.
 **/
pub fn build_framestream_unix_source(
    frame_handler: impl UnixFrameHandler + Send + Sync + Clone + 'static,
    shutdown: ShutdownSignal,
    out: SourceSender,
) -> crate::Result<Source> {
    let path = frame_handler.socket_path();

    //check if the path already exists (and try to delete it)
    match fs::metadata(&path) {
        Ok(_) => {
            //exists, so try to delete it
            info!(message = "Deleting file.", ?path);
            fs::remove_file(&path)?;
        }
        Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => {} //doesn't exist, do nothing
        Err(e) => {
            error!("Unable to get socket information; error = {:?}.", e);
            return Err(Box::new(e));
        }
    };

    let listener = UnixListener::bind(&path)?;

    // system's 'net.core.rmem_max' might have to be changed if socket receive buffer is not updated properly
    if let Some(socket_receive_buffer_size) = frame_handler.socket_receive_buffer_size() {
        _ = nix::sys::socket::setsockopt(
            &listener,
            nix::sys::socket::sockopt::RcvBuf,
            &(socket_receive_buffer_size),
        );
        let rcv_buf_size =
            nix::sys::socket::getsockopt(&listener, nix::sys::socket::sockopt::RcvBuf);
        info!(
            "Unix socket receive buffer size modified to {}.",
            rcv_buf_size.unwrap()
        );
    }

    // system's 'net.core.wmem_max' might have to be changed if socket send buffer is not updated properly
    if let Some(socket_send_buffer_size) = frame_handler.socket_send_buffer_size() {
        _ = nix::sys::socket::setsockopt(
            &listener,
            nix::sys::socket::sockopt::SndBuf,
            &(socket_send_buffer_size),
        );
        let snd_buf_size =
            nix::sys::socket::getsockopt(&listener, nix::sys::socket::sockopt::SndBuf);
        info!(
            "Unix socket buffer send size modified to {}.",
            snd_buf_size.unwrap()
        );
    }

    // the permissions to unix socket are restricted from 0o700 to 0o777, which are 448 and 511 in decimal
    if let Some(socket_permission) = frame_handler.socket_file_mode() {
        if !(448..=511).contains(&socket_permission) {
            return Err(format!(
                "Invalid Socket permission {socket_permission:#o}. Must between 0o700 and 0o777."
            )
            .into());
        }
        match fs::set_permissions(&path, fs::Permissions::from_mode(socket_permission)) {
            Ok(_) => {
                info!("Socket permissions updated to {:#o}.", socket_permission);
            }
            Err(e) => {
                error!(
                    "Failed to update listener socket permissions; error = {:?}.",
                    e
                );
                return Err(Box::new(e));
            }
        };
    };

    let fut = async move {
        let active_parsing_task_nums = Arc::new(AtomicUsize::new(0));

        info!(message = "Listening...", ?path, r#type = "unix");

        let mut stream = UnixListenerStream::new(listener).take_until(shutdown.clone());
        while let Some(socket) = stream.next().await {
            let socket = match socket {
                Err(e) => {
                    error!("Failed to accept socket; error = {:?}.", e);
                    continue;
                }
                Ok(s) => s,
            };
            let peer_addr = socket.peer_addr().ok();
            let listen_path = path.clone();
            let active_task_nums_ = Arc::clone(&active_parsing_task_nums);

            let span = info_span!("connection");
            let path = if let Some(addr) = peer_addr {
                if let Some(path) = addr.as_pathname().map(|e| e.to_owned()) {
                    span.record("peer_path", field::debug(&path));
                    Some(path)
                } else {
                    None
                }
            } else {
                None
            };
            let received_from: Option<Bytes> =
                path.map(|p| p.to_string_lossy().into_owned().into());

            build_framestream_source(
                frame_handler.clone(),
                socket,
                received_from,
                out.clone(),
                shutdown.clone(),
                span,
                active_task_nums_,
                move |error| {
                    emit!(UnixSocketError {
                        error: &error,
                        path: &listen_path,
                    });
                },
            );
        }

        // Cleanup
        drop(stream);

        // Delete socket file
        if let Err(error) = fs::remove_file(&path) {
            emit!(UnixSocketFileDeleteError { path: &path, error });
        }

        Ok(())
    };

    Ok(Box::pin(fut))
}

#[allow(clippy::too_many_arguments)]
fn build_framestream_source<T: Send + 'static>(
    frame_handler: impl FrameHandler + Send + Sync + Clone + 'static,
    socket: impl AsyncRead + AsyncWrite + Send + 'static,
    received_from: Option<Bytes>,
    out: SourceSender,
    shutdown: impl Future<Output = T> + Unpin + Send + 'static,
    span: Span,
    active_task_nums: Arc<AtomicUsize>,
    error_mapper: impl FnMut(std::io::Error) + Send + 'static,
) {
    let content_type = frame_handler.content_type();
    let mut event_sink = out.clone();
    let (sock_sink, sock_stream) = Framed::new(
        socket,
        length_delimited::Builder::new()
            .max_frame_length(frame_handler.max_frame_length())
            .new_codec(),
    )
    .split();
    let mut fs_reader = FrameStreamReader::new(Box::new(sock_sink), content_type);
    let frame_handler_copy = frame_handler.clone();
    let frames = sock_stream
        .take_until(shutdown)
        .map_err(error_mapper)
        .filter_map(move |frame| {
            future::ready(match frame {
                Ok(f) => fs_reader.handle_frame(Bytes::from(f)),
                Err(_) => None,
            })
        });
    if !frame_handler.multithreaded() {
        let mut events = frames.filter_map(move |f| {
            future::ready(frame_handler_copy.handle_event(received_from.clone(), f))
        });

        let handler = async move {
            if let Err(e) = event_sink.send_event_stream(&mut events).await {
                error!("Error sending event: {:?}.", e);
            }

            info!("Finished sending.");
        };
        tokio::spawn(handler.instrument(span.or_current()));
    } else {
        let handler = async move {
            frames
                .for_each(move |f| {
                    let max_frame_handling_tasks = frame_handler_copy.max_frame_handling_tasks();
                    let f_handler = frame_handler_copy.clone();
                    let received_from_copy = received_from.clone();
                    let event_sink_copy = event_sink.clone();
                    let active_task_nums_copy = Arc::clone(&active_task_nums);

                    async move {
                        spawn_event_handling_tasks(
                            f,
                            f_handler,
                            event_sink_copy,
                            received_from_copy,
                            active_task_nums_copy,
                            max_frame_handling_tasks,
                        )
                        .await;
                    }
                })
                .await;
            info!("Finished sending.");
        };
        tokio::spawn(handler.instrument(span.or_current()));
    }
}

async fn spawn_event_handling_tasks(
    event_data: Bytes,
    event_handler: impl FrameHandler + Send + Sync + 'static,
    mut event_sink: SourceSender,
    received_from: Option<Bytes>,
    active_task_nums: Arc<AtomicUsize>,
    max_frame_handling_tasks: usize,
) -> JoinHandle<()> {
    wait_for_task_quota(&active_task_nums, max_frame_handling_tasks).await;

    crate::spawn_in_current_span(async move {
        future::ready({
            if let Some(evt) = event_handler.handle_event(received_from, event_data)
                && event_sink.send_event(evt).await.is_err()
            {
                error!("Encountered error while sending event.");
            }
            active_task_nums.fetch_sub(1, Ordering::AcqRel);
        })
        .await;
    })
}

async fn wait_for_task_quota(active_task_nums: &Arc<AtomicUsize>, max_tasks: usize) {
    while max_tasks > 0 && max_tasks < active_task_nums.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(3)).await;
    }
    active_task_nums.fetch_add(1, Ordering::AcqRel);
}

#[cfg(test)]
mod test;

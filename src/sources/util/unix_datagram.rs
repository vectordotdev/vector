use std::{
    fs::{remove_file, symlink_metadata},
    io,
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::{Path, PathBuf},
};

use bytes::{Bytes, BytesMut};
use futures::StreamExt;
use smallvec::SmallVec;
use tokio::net::UnixDatagram;
use vector_lib::internal_event::{emit, register};
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::{
        DecoderFramedRead, StreamDecodingError,
        decoding::format::{BoxedDeserializer, Deserializer as FormatDeserializer},
    },
    config::LogNamespace,
    internal_event::{
        ByteSize, BytesReceived, ComponentEventsDropped, InternalEventHandle as _, Protocol,
        UNINTENTIONAL,
    },
    source_sender::chunk_size_events,
};

#[cfg(target_os = "linux")]
use {
    nix::libc,
    std::{
        ffi::OsStr,
        mem,
        os::{fd::AsRawFd, unix::ffi::OsStrExt, unix::net::UnixDatagram as StdUnixDatagram},
        ptr, slice,
    },
    tokio::io::unix::AsyncFd,
};

use crate::{
    SourceSender,
    codecs::Decoder,
    event::Event,
    internal_events::{
        SocketBindError, SocketEventsReceived, SocketMode, SocketOversizedDatagram,
        SocketReceiveError, StreamClosedError, UnixSocketFileDeleteError,
    },
    shutdown::ShutdownSignal,
    sources::{
        Source,
        util::{change_socket_permissions, unix::UNNAMED_SOCKET_HOST},
    },
};

const UNIX_DATAGRAM_BATCH_SIZE: usize = 256;
const DEFAULT_UNIX_DATAGRAM_MAX_BATCH_BYTES: usize = 16 * 1024 * 1024;

#[cfg(target_os = "linux")]
struct UnixDatagramBatch<'a> {
    receiver: &'a UnixDatagramBatchReceiver,
    received: usize,
}

#[cfg(target_os = "linux")]
impl UnixDatagramBatch<'_> {
    const fn len(&self) -> usize {
        self.received
    }

    fn payload(&self, index: usize) -> &[u8] {
        &self.receiver.buffers[index][..(self.receiver.headers[index].msg_len as usize)
            .min(self.receiver.buffers[index].len())]
    }

    fn received_size(&self, index: usize) -> usize {
        self.receiver.headers[index].msg_len as usize
    }

    fn accepted_payload_size(&self) -> usize {
        (0..self.len())
            .filter(|&index| !self.truncated(index))
            .map(|index| self.payload(index).len())
            .sum()
    }

    fn truncated(&self, index: usize) -> bool {
        let header = &self.receiver.headers[index];
        header.msg_len as usize > self.receiver.buffers[index].len()
            || (header.msg_hdr.msg_flags & libc::MSG_TRUNC) != 0
    }

    const fn captures_peer_address(&self) -> bool {
        self.receiver.capture_peer_address
    }

    fn received_from(&self, index: usize) -> io::Result<Option<String>> {
        unix_peer_from_sockaddr(
            &self.receiver.addresses[index],
            self.receiver.headers[index].msg_hdr.msg_namelen,
        )
    }
}

#[cfg(not(target_os = "linux"))]
struct UnixDatagramBatch<'a> {
    payload: &'a [u8],
    received_from: Option<String>,
    truncated: bool,
    capture_peer_address: bool,
    received_size: usize,
}

#[cfg(not(target_os = "linux"))]
impl UnixDatagramBatch<'_> {
    const fn len(&self) -> usize {
        1
    }

    fn payload(&self, index: usize) -> &[u8] {
        assert_eq!(index, 0);
        self.payload
    }

    fn received_size(&self, index: usize) -> usize {
        assert_eq!(index, 0);
        self.received_size
    }

    fn accepted_payload_size(&self) -> usize {
        if self.truncated {
            0
        } else {
            self.payload.len()
        }
    }

    fn truncated(&self, index: usize) -> bool {
        assert_eq!(index, 0);
        self.truncated
    }

    const fn captures_peer_address(&self) -> bool {
        self.capture_peer_address
    }

    fn received_from(&self, index: usize) -> io::Result<Option<String>> {
        assert_eq!(index, 0);
        Ok(self.received_from.clone())
    }
}

#[cfg(target_os = "linux")]
struct UnixDatagramBatchReceiver {
    socket: AsyncFd<StdUnixDatagram>,
    addresses: Box<[libc::sockaddr_storage; UNIX_DATAGRAM_BATCH_SIZE]>,
    buffers: Vec<Box<[u8]>>,
    _iovecs: Box<[libc::iovec; UNIX_DATAGRAM_BATCH_SIZE]>,
    headers: Box<[libc::mmsghdr; UNIX_DATAGRAM_BATCH_SIZE]>,
    batch_size: usize,
    last_received: usize,
    capture_peer_address: bool,
}

// SAFETY: The raw pointers in `headers` only refer to the receiver's stable heap
// allocations (`addresses`, `buffers`, and `_iovecs`). The receiver is owned
// by one source task and is never accessed concurrently.
#[cfg(target_os = "linux")]
unsafe impl Send for UnixDatagramBatchReceiver {}
// SAFETY: shared access only reads completed `recvmmsg` output. Receiving the
// next batch requires an exclusive borrow, so those reads cannot race with the
// kernel mutating the pointed-to buffers and headers.
#[cfg(target_os = "linux")]
unsafe impl Sync for UnixDatagramBatchReceiver {}

#[cfg(target_os = "linux")]
impl UnixDatagramBatchReceiver {
    #[allow(clippy::cast_possible_truncation)]
    fn new(
        socket: UnixDatagram,
        max_length: usize,
        capture_peer_address: bool,
        max_batch_bytes: Option<usize>,
    ) -> io::Result<Self> {
        validate_max_length(max_length)?;
        let max_batch_bytes = max_batch_bytes
            .unwrap_or_else(|| DEFAULT_UNIX_DATAGRAM_MAX_BATCH_BYTES.max(max_length));
        if max_batch_bytes < max_length {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Unix datagram max_batch_bytes must be at least max_length",
            ));
        }
        let batch_size = (max_batch_bytes / max_length).min(UNIX_DATAGRAM_BATCH_SIZE);
        let socket = socket.into_std()?;
        socket.set_nonblocking(true)?;

        let mut addresses: Box<[libc::sockaddr_storage; UNIX_DATAGRAM_BATCH_SIZE]> =
            Box::new(std::array::from_fn(|_| {
                // SAFETY: `sockaddr_storage` is an output buffer that the kernel initializes.
                unsafe { mem::zeroed() }
            }));
        let mut buffers = (0..batch_size)
            .map(|_| vec![0; max_length].into_boxed_slice())
            .collect::<Vec<_>>();
        let mut iovecs: Box<[libc::iovec; UNIX_DATAGRAM_BATCH_SIZE]> =
            Box::new(std::array::from_fn(|_| {
                // SAFETY: every field is initialized below before the kernel reads it.
                unsafe { mem::zeroed() }
            }));
        let mut headers: Box<[libc::mmsghdr; UNIX_DATAGRAM_BATCH_SIZE]> =
            Box::new(std::array::from_fn(|_| {
                // SAFETY: every field read by `recvmmsg` is initialized below.
                unsafe { mem::zeroed() }
            }));

        for index in 0..batch_size {
            iovecs[index] = libc::iovec {
                iov_base: buffers[index].as_mut_ptr().cast(),
                iov_len: buffers[index].len(),
            };
            headers[index].msg_hdr = libc::msghdr {
                msg_name: if capture_peer_address {
                    (&raw mut addresses[index]).cast()
                } else {
                    ptr::null_mut()
                },
                msg_namelen: if capture_peer_address {
                    mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t
                } else {
                    0
                },
                msg_iov: &raw mut iovecs[index],
                msg_iovlen: 1,
                msg_control: ptr::null_mut(),
                msg_controllen: 0,
                msg_flags: 0,
            };
        }

        // SAFETY: `StdUnixDatagram` owns a stable, valid descriptor for its entire lifetime.
        let socket = unsafe { AsyncFd::register(socket) }?;

        Ok(Self {
            socket,
            addresses,
            buffers,
            _iovecs: iovecs,
            headers,
            batch_size,
            last_received: 0,
            capture_peer_address,
        })
    }

    async fn recv_batch(&mut self) -> io::Result<UnixDatagramBatch<'_>> {
        let last_received = self.last_received;
        let received = loop {
            let mut guard = self.socket.readable().await?;
            let headers = &mut self.headers;
            let batch_size = self.batch_size;
            match guard.try_io(|socket| {
                try_recvmmsg_unix(socket.get_ref(), headers, last_received, batch_size)
            }) {
                Ok(result) => break result?,
                Err(_would_block) => {}
            }
        };
        self.last_received = received;

        Ok(UnixDatagramBatch {
            receiver: self,
            received,
        })
    }
}

#[cfg(target_os = "linux")]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn try_recvmmsg_unix(
    socket: &StdUnixDatagram,
    headers: &mut [libc::mmsghdr; UNIX_DATAGRAM_BATCH_SIZE],
    reset_count: usize,
    batch_size: usize,
) -> io::Result<usize> {
    if !headers[0].msg_hdr.msg_name.is_null() {
        for header in headers.iter_mut().take(reset_count) {
            header.msg_hdr.msg_namelen =
                mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;
        }
    }

    // SAFETY: the receiver owns every buffer referenced by `headers`; the pointers and lengths
    // were initialized together and remain stable for this nonblocking call.
    let count = unsafe {
        libc::recvmmsg(
            socket.as_raw_fd(),
            headers.as_mut_ptr(),
            batch_size as libc::c_uint,
            libc::MSG_DONTWAIT | libc::MSG_TRUNC,
            ptr::null_mut(),
        )
    };

    if count < 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(count as usize)
}

#[cfg(target_os = "linux")]
fn unix_peer_from_sockaddr(
    storage: &libc::sockaddr_storage,
    len: libc::socklen_t,
) -> io::Result<Option<String>> {
    let path_offset = mem::offset_of!(libc::sockaddr_un, sun_path);
    let len = len as usize;
    if len <= path_offset {
        return Ok(Some(unnamed_socket_host()));
    }
    if libc::c_int::from(storage.ss_family) != libc::AF_UNIX {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "received a non-Unix address from a Unix datagram socket",
        ));
    }

    // SAFETY: the checked family and kernel-provided length establish a Unix socket address;
    // unaligned reads are permitted explicitly.
    let address =
        unsafe { ptr::read_unaligned(ptr::from_ref(storage).cast::<libc::sockaddr_un>()) };
    let path_len = (len - path_offset).min(address.sun_path.len());
    // SAFETY: `path_len` is bounded by `sun_path`, and the copied address remains live here.
    let path = unsafe { slice::from_raw_parts(address.sun_path.as_ptr().cast::<u8>(), path_len) };

    // Linux abstract namespace sockets have a leading NUL and no pathname. Tokio's
    // SocketAddr also exposes these as neither unnamed nor pathname addresses.
    if path.first() == Some(&0) {
        return Ok(None);
    }

    let path_end = path
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(path.len());
    let path = OsStr::from_bytes(&path[..path_end]);
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[cfg(not(target_os = "linux"))]
struct UnixDatagramBatchReceiver {
    socket: UnixDatagram,
    buffer: Vec<u8>,
    max_length: usize,
    capture_peer_address: bool,
}

#[cfg(not(target_os = "linux"))]
impl UnixDatagramBatchReceiver {
    fn new(
        socket: UnixDatagram,
        max_length: usize,
        capture_peer_address: bool,
        _max_batch_bytes: Option<usize>,
    ) -> io::Result<Self> {
        validate_max_length(max_length)?;
        Ok(Self {
            socket,
            buffer: vec![0; max_length.saturating_add(1)],
            max_length,
            capture_peer_address,
        })
    }

    async fn recv_batch(&mut self) -> io::Result<UnixDatagramBatch<'_>> {
        let (byte_size, received_from) = if self.capture_peer_address {
            let (byte_size, address) = self.socket.recv_from(&mut self.buffer).await?;
            let received_from = if address.is_unnamed() {
                Some(unnamed_socket_host())
            } else {
                address
                    .as_pathname()
                    .map(|path| path.to_string_lossy().into_owned())
            };
            (byte_size, received_from)
        } else {
            (self.socket.recv(&mut self.buffer).await?, None)
        };
        Ok(UnixDatagramBatch {
            payload: &self.buffer[..byte_size],
            received_from,
            truncated: byte_size > self.max_length,
            capture_peer_address: self.capture_peer_address,
            received_size: byte_size,
        })
    }
}

fn validate_max_length(max_length: usize) -> io::Result<()> {
    if max_length == 0 {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Unix datagram size must be greater than zero",
        ))
    } else {
        Ok(())
    }
}

fn unnamed_socket_host() -> String {
    UNNAMED_SOCKET_HOST.to_owned()
}

enum DatagramDecoder {
    Framed(Box<Decoder>),
    Message(BoxedDeserializer, LogNamespace),
}

/// Returns a `Source` object corresponding to a Unix domain datagram socket.
/// Passing in different functions for `decoder` and `handle_events` can allow
/// for different source-specific logic (such as decoding syslog messages in the
/// syslog source).
///
/// # Errors
///
/// The returned source reports an error if processing the datagram stream fails.
///
/// # Panics
///
/// The returned source panics if binding the socket or setting its permissions fails.
pub fn build_unix_datagram_source(
    listen_path: PathBuf,
    socket_file_mode: Option<u32>,
    max_length: usize,
    decoder: Decoder,
    handle_events: impl Fn(&mut [Event], Option<String>) + Clone + Send + Sync + 'static,
    shutdown: ShutdownSignal,
    out: SourceSender,
) -> crate::Result<Source> {
    // Framing limits exclude the trailing delimiter, so retain one additional byte for it.
    Ok(build_unix_datagram_source_inner(
        listen_path,
        socket_file_mode,
        max_length.saturating_add(1),
        None,
        None,
        DatagramDecoder::Framed(Box::new(decoder)),
        true,
        handle_events,
        shutdown,
        out,
    ))
}

/// Returns a Unix domain datagram source for message-oriented deserializers.
///
/// Unlike [`build_unix_datagram_source`], this skips stream framing and decodes each datagram as
/// one message. Datagram-oriented protocols such as syslog can use this to retain receive batching
/// without constructing a framed stream per packet.
///
/// # Errors
///
/// The returned source reports an error if processing the datagram stream fails.
#[allow(clippy::too_many_arguments)]
pub fn build_unix_datagram_source_with_message_decoder(
    listen_path: PathBuf,
    socket_file_mode: Option<u32>,
    max_length: usize,
    receive_buffer_bytes: Option<usize>,
    max_batch_bytes: Option<usize>,
    decoder: impl FormatDeserializer + 'static,
    log_namespace: LogNamespace,
    capture_peer_address: bool,
    handle_events: impl Fn(&mut [Event], Option<String>) + Clone + Send + Sync + 'static,
    shutdown: ShutdownSignal,
    out: SourceSender,
) -> crate::Result<Source> {
    Ok(build_unix_datagram_source_inner(
        listen_path,
        socket_file_mode,
        max_length,
        receive_buffer_bytes,
        max_batch_bytes,
        DatagramDecoder::Message(Box::new(decoder), log_namespace),
        capture_peer_address,
        handle_events,
        shutdown,
        out,
    ))
}

#[allow(clippy::too_many_arguments)]
fn build_unix_datagram_source_inner(
    listen_path: PathBuf,
    socket_file_mode: Option<u32>,
    max_length: usize,
    receive_buffer_bytes: Option<usize>,
    max_batch_bytes: Option<usize>,
    decoder: DatagramDecoder,
    capture_peer_address: bool,
    handle_events: impl Fn(&mut [Event], Option<String>) + Clone + Send + Sync + 'static,
    shutdown: ShutdownSignal,
    out: SourceSender,
) -> Source {
    Box::pin(async move {
        let socket = match bind_unix_datagram(&listen_path) {
            Ok(socket) => socket,
            Err(error) => {
                emit(SocketBindError {
                    mode: SocketMode::Unix,
                    error: &error,
                });
                return Err(crate::sources::SourceError::from(error));
            }
        };
        if let Some(receive_buffer_bytes) = receive_buffer_bytes {
            crate::net::set_receive_buffer_size(&socket, receive_buffer_bytes)
                .expect("Failed to set Unix datagram receive buffer size");
        }
        info!(message = "Listening.", path = ?listen_path, r#type = "unix_datagram");

        change_socket_permissions(&listen_path, socket_file_mode)
            .expect("Failed to set socket permissions");

        let result = listen(
            socket,
            max_length,
            max_batch_bytes,
            decoder,
            capture_peer_address,
            shutdown,
            handle_events,
            out,
        )
        .await;

        // Delete socket file.
        if let Err(error) = remove_file(&listen_path) {
            emit(UnixSocketFileDeleteError {
                path: &listen_path,
                error,
            });
        }

        result
    })
}

fn bind_unix_datagram(listen_path: &Path) -> io::Result<UnixDatagram> {
    let bind_error = match UnixDatagram::bind(listen_path) {
        Ok(socket) => return Ok(socket),
        Err(error) if error.kind() == io::ErrorKind::AddrInUse => error,
        Err(error) => return Err(error),
    };
    let metadata = symlink_metadata(listen_path)?;
    if !metadata.file_type().is_socket() {
        return Err(bind_error);
    }
    let probe = std::os::unix::net::UnixDatagram::unbound()?;
    probe.set_nonblocking(true)?;
    match probe.connect(listen_path) {
        Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
            let current = symlink_metadata(listen_path)?;
            if current.dev() != metadata.dev() || current.ino() != metadata.ino() {
                return Err(bind_error);
            }
            remove_file(listen_path)?;
            UnixDatagram::bind(listen_path)
        }
        _ => Err(bind_error),
    }
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_lines)]
async fn listen(
    socket: UnixDatagram,
    max_length: usize,
    max_batch_bytes: Option<usize>,
    decoder: DatagramDecoder,
    capture_peer_address: bool,
    mut shutdown: ShutdownSignal,
    handle_events: impl Fn(&mut [Event], Option<String>) + Clone + Send + Sync + 'static,
    mut out: SourceSender,
) -> Result<(), crate::sources::SourceError> {
    let mut receiver =
        UnixDatagramBatchReceiver::new(socket, max_length, capture_peer_address, max_batch_bytes)
            .map_err(|error| {
            emit(SocketReceiveError {
                mode: SocketMode::Unix,
                error: &error,
            });
            crate::sources::SourceError::from(error)
        })?;
    let bytes_received = register(BytesReceived::from(Protocol::UNIX));
    let send_batch_size = chunk_size_events().max(1);
    loop {
        let batch = tokio::select! {
            batch_result = receiver.recv_batch() => batch_result.map_err(|error| {
                emit(SocketReceiveError {
                    mode: SocketMode::Unix,
                    error: &error,
                });
                crate::sources::SourceError::from(error)
            })?,
            _ = &mut shutdown => return Ok(()),
        };

        let received_byte_size = (0..batch.len())
            .map(|index| batch.received_size(index))
            .sum();
        bytes_received.emit(ByteSize(received_byte_size));
        for index in 0..batch.len() {
            if batch.truncated(index) {
                emit(SocketOversizedDatagram {
                    mode: SocketMode::Unix,
                });
                warn!(
                    message = "Discarding Unix datagram larger than max_length.",
                    max_length
                );
                emit(ComponentEventsDropped::<UNINTENTIONAL> {
                    count: 1,
                    reason: "Unix datagram exceeded max_length.",
                });
            }
        }
        let received_from = if batch.captures_peer_address() {
            let mut addresses = SmallVec::<[Option<String>; UNIX_DATAGRAM_BATCH_SIZE]>::new();
            for index in 0..batch.len() {
                match batch.received_from(index) {
                    Ok(address) => addresses.push(address),
                    Err(error) => {
                        emit(SocketReceiveError {
                            mode: SocketMode::Unix,
                            error: &error,
                        });
                        addresses.push(None);
                    }
                }
            }
            Some(addresses)
        } else {
            None
        };

        let mut output = Vec::with_capacity(batch.len().min(send_batch_size));
        match &decoder {
            DatagramDecoder::Framed(decoder) => {
                for index in 0..batch.len() {
                    if batch.truncated(index) {
                        continue;
                    }
                    let mut stream =
                        DecoderFramedRead::new(batch.payload(index), decoder.as_ref().clone());
                    while let Some(result) = stream.next().await {
                        match result {
                            Ok((events, _byte_size)) => {
                                append_events(
                                    events,
                                    received_from
                                        .as_ref()
                                        .and_then(|values| values[index].clone()),
                                    &handle_events,
                                    &mut output,
                                );
                                while output.len() >= send_batch_size {
                                    if !send_buffered_events(
                                        &mut out,
                                        &mut shutdown,
                                        &mut output,
                                        send_batch_size,
                                    )
                                    .await
                                    {
                                        return Ok(());
                                    }
                                }
                            }
                            Err(error) => {
                                emit(SocketReceiveError {
                                    mode: SocketMode::Unix,
                                    error: &error,
                                });
                                if !error.can_continue() {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            DatagramDecoder::Message(parser, log_namespace) => {
                if received_from
                    .as_ref()
                    .is_none_or(|values| values.iter().all(Option::is_none))
                {
                    // Keep the batch in one reference-counted allocation. The deferred syslog
                    // fast path retains every frame until the Vector sink encodes the batch, so
                    // allocating one `Bytes` backing store per datagram creates substantial
                    // allocator/free pressure at high packet rates.
                    // `received_byte_size` includes the original lengths reported by
                    // `MSG_TRUNC`. Reserve only bytes that will actually be retained so a
                    // batch of rejected oversized datagrams cannot amplify memory use.
                    let mut storage = BytesMut::with_capacity(batch.accepted_payload_size());
                    let mut ranges = SmallVec::<[_; UNIX_DATAGRAM_BATCH_SIZE]>::new();
                    for index in 0..batch.len() {
                        if batch.truncated(index) {
                            continue;
                        }
                        let start = storage.len();
                        storage.extend_from_slice(batch.payload(index));
                        ranges.push(start..storage.len());
                    }
                    let storage = storage.freeze();
                    let frames: SmallVec<[Bytes; UNIX_DATAGRAM_BATCH_SIZE]> = ranges
                        .into_iter()
                        .map(|range| storage.slice(range))
                        .collect();
                    for frame in frames {
                        match parser.parse(frame, *log_namespace) {
                            Ok(events) => {
                                append_events(events, None, &handle_events, &mut output);
                                while output.len() >= send_batch_size {
                                    if !send_buffered_events(
                                        &mut out,
                                        &mut shutdown,
                                        &mut output,
                                        send_batch_size,
                                    )
                                    .await
                                    {
                                        return Ok(());
                                    }
                                }
                            }
                            Err(error) => emit(SocketReceiveError {
                                mode: SocketMode::Unix,
                                error: &error,
                            }),
                        }
                    }
                } else {
                    for index in 0..batch.len() {
                        if batch.truncated(index) {
                            continue;
                        }
                        match parser
                            .parse(Bytes::copy_from_slice(batch.payload(index)), *log_namespace)
                        {
                            Ok(events) => {
                                append_events(
                                    events,
                                    received_from
                                        .as_ref()
                                        .and_then(|values| values[index].clone()),
                                    &handle_events,
                                    &mut output,
                                );
                                while output.len() >= send_batch_size {
                                    if !send_buffered_events(
                                        &mut out,
                                        &mut shutdown,
                                        &mut output,
                                        send_batch_size,
                                    )
                                    .await
                                    {
                                        return Ok(());
                                    }
                                }
                            }
                            Err(error) => {
                                emit(SocketReceiveError {
                                    mode: SocketMode::Unix,
                                    error: &error,
                                });
                            }
                        }
                    }
                }
            }
        }

        while !output.is_empty() {
            if !send_buffered_events(&mut out, &mut shutdown, &mut output, send_batch_size).await {
                return Ok(());
            }
        }
    }
}

fn append_events(
    mut events: impl AsMut<[Event]> + IntoIterator<Item = Event>,
    received_from: Option<String>,
    handle_events: &impl Fn(&mut [Event], Option<String>),
    output: &mut impl Extend<Event>,
) {
    handle_events(events.as_mut(), received_from);
    output.extend(events);
}

async fn send_buffered_events(
    out: &mut SourceSender,
    shutdown: &mut ShutdownSignal,
    output: &mut Vec<Event>,
    max_events: usize,
) -> bool {
    let split_at = output.len().min(max_events);
    let remainder = output.split_off(split_at);
    let events = std::mem::replace(output, remainder);
    let count = events.len();
    debug_assert!(count > 0);

    emit(SocketEventsReceived {
        mode: SocketMode::Unix,
        byte_size: events.estimated_json_encoded_size_of(),
        count,
    });
    tokio::select! {
        result = out.send_batch(events) => {
            if result.is_err() {
                emit(StreamClosedError { count });
                false
            } else {
                true
            }
        }
        _ = shutdown => false,
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use tokio::net::UnixDatagram;

    use super::{UnixDatagramBatchReceiver, bind_unix_datagram, unnamed_socket_host};

    #[tokio::test]
    async fn rebinds_abandoned_unix_datagram_socket() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("socket");
        drop(UnixDatagram::bind(&path).unwrap());
        let receiver = bind_unix_datagram(&path).unwrap();
        let sender = UnixDatagram::unbound().unwrap();
        sender.send_to(b"resumed", &path).await.unwrap();
        let mut buffer = [0; 32];
        let received = receiver.recv(&mut buffer).await.unwrap();
        assert_eq!(&buffer[..received], b"resumed");
    }

    #[tokio::test]
    async fn preserves_live_unix_datagram_socket() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("socket");
        let receiver = UnixDatagram::bind(&path).unwrap();
        assert_eq!(
            bind_unix_datagram(&path).unwrap_err().kind(),
            std::io::ErrorKind::AddrInUse
        );
        let sender = UnixDatagram::unbound().unwrap();
        sender.send_to(b"original", &path).await.unwrap();
        let mut buffer = [0; 32];
        let received = receiver.recv(&mut buffer).await.unwrap();
        assert_eq!(&buffer[..received], b"original");
    }

    #[tokio::test]
    async fn preserves_non_socket_paths_and_symlinks() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("file");
        std::fs::write(&path, b"preserved").unwrap();
        assert!(bind_unix_datagram(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"preserved");
        let target = directory.path().join("abandoned");
        drop(UnixDatagram::bind(&target).unwrap());
        let link = directory.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(bind_unix_datagram(&link).is_err());
        assert!(link.is_symlink());
        assert!(target.exists());
    }

    #[tokio::test]
    async fn recvmmsg_receives_multiple_unix_datagrams_and_peer_paths() {
        let directory = tempfile::tempdir().unwrap();
        let receiver_path = directory.path().join("receiver");
        let sender_path = directory.path().join("sender");
        let socket = UnixDatagram::bind(&receiver_path).unwrap();
        let mut receiver = UnixDatagramBatchReceiver::new(socket, 64, true, None).unwrap();
        let sender = UnixDatagram::bind(&sender_path).unwrap();
        sender.connect(&receiver_path).unwrap();

        for message in ["one", "two", "three"] {
            sender.send(message.as_bytes()).await.unwrap();
        }

        let batch = receiver.recv_batch().await.unwrap();
        assert_eq!(batch.len(), 3);
        assert_eq!(
            (0..batch.len())
                .map(|index| batch.payload(index))
                .collect::<Vec<_>>(),
            [b"one".as_slice(), b"two".as_slice(), b"three".as_slice()]
        );
        let expected_peer = sender_path.to_string_lossy().into_owned();
        assert!(
            (0..batch.len())
                .all(|index| batch.received_from(index).unwrap().as_ref() == Some(&expected_peer))
        );

        for message in ["four", "five"] {
            sender.send(message.as_bytes()).await.unwrap();
        }
        let batch = receiver.recv_batch().await.unwrap();
        assert_eq!(
            (0..batch.len())
                .map(|index| batch.payload(index))
                .collect::<Vec<_>>(),
            [b"four".as_slice(), b"five".as_slice()]
        );
    }

    #[tokio::test]
    async fn recvmmsg_preserves_unnamed_unix_peer() {
        let directory = tempfile::tempdir().unwrap();
        let receiver_path = directory.path().join("receiver");
        let socket = UnixDatagram::bind(&receiver_path).unwrap();
        let mut receiver = UnixDatagramBatchReceiver::new(socket, 64, true, None).unwrap();
        let sender = UnixDatagram::unbound().unwrap();
        sender.send_to(b"hello", &receiver_path).await.unwrap();

        let batch = receiver.recv_batch().await.unwrap();
        assert_eq!(batch.payload(0), b"hello");
        assert_eq!(batch.received_from(0).unwrap(), Some(unnamed_socket_host()));
    }

    #[tokio::test]
    async fn recvmmsg_reports_truncated_unix_datagrams() {
        let directory = tempfile::tempdir().unwrap();
        let receiver_path = directory.path().join("receiver");
        let socket = UnixDatagram::bind(&receiver_path).unwrap();
        let mut receiver = UnixDatagramBatchReceiver::new(socket, 5, false, None).unwrap();
        let sender = UnixDatagram::unbound().unwrap();

        sender.send_to(b"payload", &receiver_path).await.unwrap();
        let batch = receiver.recv_batch().await.unwrap();
        assert!(batch.truncated(0));
        assert_eq!(batch.payload(0), b"paylo");
        assert_eq!(batch.received_size(0), 7);
        assert_eq!(batch.accepted_payload_size(), 0);

        sender.send_to(b"exact", &receiver_path).await.unwrap();
        let batch = receiver.recv_batch().await.unwrap();
        assert!(!batch.truncated(0));
        assert_eq!(batch.payload(0), b"exact");
        assert_eq!(batch.accepted_payload_size(), 5);
    }

    #[tokio::test]
    async fn receive_batch_memory_is_bounded() {
        let directory = tempfile::tempdir().unwrap();
        let receiver_path = directory.path().join("receiver");
        let socket = UnixDatagram::bind(&receiver_path).unwrap();
        let receiver = UnixDatagramBatchReceiver::new(socket, 1024, false, Some(4 * 1024)).unwrap();

        assert_eq!(receiver.batch_size, 4);
        assert_eq!(receiver.buffers.len(), 4);
        assert_eq!(
            receiver
                .buffers
                .iter()
                .map(|buffer| buffer.len())
                .sum::<usize>(),
            4096
        );
    }
    #[tokio::test]
    async fn receive_batch_budget_must_fit_one_datagram() {
        let directory = tempfile::tempdir().unwrap();
        let receiver_path = directory.path().join("receiver");
        let socket = UnixDatagram::bind(&receiver_path).unwrap();
        let result = UnixDatagramBatchReceiver::new(socket, 4096, false, Some(1024));
        assert!(matches!(
            result,
            Err(error) if error.kind() == std::io::ErrorKind::InvalidInput
        ));
    }
}

use std::net::SocketAddr;
#[cfg(unix)]
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};

use bytes::{Bytes, BytesMut, buf::Buf};
use futures::{
    future,
    sink::{Sink, SinkExt},
    stream::{self, StreamExt},
};
use futures_util::Stream;
use ipnet::IpNet;
use tokio::{
    self,
    net::{TcpStream, UnixStream},
    task::JoinHandle,
    time::{Duration, Instant},
};
use tokio_util::codec::{Framed, length_delimited};
use vector_lib::{
    config::{LegacyKey, LogNamespace},
    lookup::{OwnedValuePath, owned_value_path, path},
    tcp::TcpKeepaliveConfig,
    tls::{CertificateMetadata, MaybeTls, MaybeTlsSettings},
};

use super::{
    ControlField, ControlHeader, FrameHandler, TcpFrameHandler, UnixFrameHandler,
    build_framestream_tcp_source, build_framestream_unix_source, spawn_event_handling_tasks,
};
use crate::{
    SourceSender,
    config::{ComponentKey, log_schema},
    event::{Event, LogEvent},
    shutdown::SourceShutdownCoordinator,
    sources::util::net::SocketListenAddr,
    test_util::{addr::next_addr, collect_n, collect_n_stream},
};

#[derive(Clone)]
struct MockFrameHandler<F: Send + Sync + Clone + FnOnce() + 'static> {
    content_type: String,
    max_frame_length: usize,
    multithreaded: bool,
    max_frame_handling_tasks: usize,
    extra_task_handling_routine: F,
    host_key: Option<OwnedValuePath>,
    timestamp_key: Option<OwnedValuePath>,
    source_type_key: Option<OwnedValuePath>,
    log_namespace: LogNamespace,
}

#[derive(Clone)]
struct MockUnixFrameHandler<F: Send + Sync + Clone + FnOnce() + 'static> {
    frame_handler: MockFrameHandler<F>,
    socket_path: PathBuf,
    socket_file_mode: Option<u32>,
    socket_receive_buffer_size: Option<usize>,
    socket_send_buffer_size: Option<usize>,
}

#[derive(Clone)]
struct MockTcpFrameHandler<F: Send + Sync + Clone + FnOnce() + 'static> {
    frame_handler: MockFrameHandler<F>,
    address: SocketListenAddr,
    keepalive: Option<TcpKeepaliveConfig>,
    shutdown_timeout_secs: Duration,
    tls: MaybeTlsSettings,
    tls_client_metadata_key: Option<OwnedValuePath>,
    receive_buffer_bytes: Option<usize>,
    max_connection_duration_secs: Option<u64>,
    max_connections: Option<u32>,
    permit_origin: Option<Vec<IpNet>>,
}

impl<F: Send + Sync + Clone + FnOnce() + 'static> MockTcpFrameHandler<F> {
    pub fn new(
        addr: SocketAddr,
        content_type: String,
        multithreaded: bool,
        extra_routine: F,
        permit_origin: Option<Vec<IpNet>>,
    ) -> Self {
        Self {
            frame_handler: MockFrameHandler::new(content_type, multithreaded, extra_routine),
            address: addr.into(),
            keepalive: None,
            shutdown_timeout_secs: Duration::from_secs(30),
            tls: MaybeTls::Raw(()),
            tls_client_metadata_key: None,
            receive_buffer_bytes: None,
            max_connection_duration_secs: None,
            max_connections: None,
            permit_origin,
        }
    }
}

impl<F: Send + Sync + Clone + FnOnce() + 'static> MockUnixFrameHandler<F> {
    pub fn new(content_type: String, multithreaded: bool, extra_routine: F) -> Self {
        Self {
            frame_handler: MockFrameHandler::new(content_type, multithreaded, extra_routine),
            socket_path: tempfile::tempdir().unwrap().keep().join("unix_test"),
            socket_file_mode: None,
            socket_receive_buffer_size: None,
            socket_send_buffer_size: None,
        }
    }
}

impl<F: Send + Sync + Clone + FnOnce() + 'static> MockFrameHandler<F> {
    pub fn new(content_type: String, multithreaded: bool, extra_routine: F) -> Self {
        Self {
            content_type,
            max_frame_length: bytesize::kib(100u64) as usize,
            multithreaded,
            max_frame_handling_tasks: 0,
            extra_task_handling_routine: extra_routine,
            host_key: Some(owned_value_path!("test_framestream")),
            timestamp_key: Some(owned_value_path!("my_timestamp")),
            source_type_key: Some(owned_value_path!("source_type")),
            log_namespace: LogNamespace::Legacy,
        }
    }
}

impl<F: Send + Sync + Clone + FnOnce() + 'static> FrameHandler for MockFrameHandler<F> {
    fn content_type(&self) -> String {
        self.content_type.clone()
    }
    fn max_frame_length(&self) -> usize {
        self.max_frame_length
    }

    fn handle_event(&self, received_from: Option<Bytes>, frame: Bytes) -> Option<Event> {
        let mut log_event = LogEvent::from(frame);

        log_event.insert(
            log_schema().source_type_key_target_path().unwrap(),
            "framestream",
        );
        if let Some(host) = received_from {
            self.log_namespace.insert_source_metadata(
                "framestream",
                &mut log_event,
                self.host_key.as_ref().map(LegacyKey::Overwrite),
                path!("host"),
                host,
            )
        }

        (self.extra_task_handling_routine.clone())();

        Some(log_event.into())
    }

    fn multithreaded(&self) -> bool {
        self.multithreaded
    }
    fn max_frame_handling_tasks(&self) -> usize {
        self.max_frame_handling_tasks
    }

    fn host_key(&self) -> &Option<OwnedValuePath> {
        &self.host_key
    }

    fn timestamp_key(&self) -> Option<&OwnedValuePath> {
        self.timestamp_key.as_ref()
    }

    fn source_type_key(&self) -> Option<&OwnedValuePath> {
        self.source_type_key.as_ref()
    }
}

impl<F: Send + Sync + Clone + FnOnce() + 'static> FrameHandler for MockUnixFrameHandler<F> {
    fn content_type(&self) -> String {
        self.frame_handler.content_type()
    }

    fn max_frame_length(&self) -> usize {
        self.frame_handler.max_frame_length()
    }

    fn handle_event(&self, received_from: Option<Bytes>, frame: Bytes) -> Option<Event> {
        self.frame_handler.handle_event(received_from, frame)
    }

    fn multithreaded(&self) -> bool {
        self.frame_handler.multithreaded()
    }

    fn max_frame_handling_tasks(&self) -> usize {
        self.frame_handler.max_frame_handling_tasks()
    }

    fn host_key(&self) -> &Option<OwnedValuePath> {
        self.frame_handler.host_key()
    }

    fn timestamp_key(&self) -> Option<&OwnedValuePath> {
        self.frame_handler.timestamp_key()
    }

    fn source_type_key(&self) -> Option<&OwnedValuePath> {
        self.frame_handler.source_type_key()
    }
}

impl<F: Send + Sync + Clone + FnOnce() + 'static> UnixFrameHandler for MockUnixFrameHandler<F> {
    fn socket_path(&self) -> PathBuf {
        self.socket_path.clone()
    }

    fn socket_file_mode(&self) -> Option<u32> {
        self.socket_file_mode
    }

    fn socket_receive_buffer_size(&self) -> Option<usize> {
        self.socket_receive_buffer_size
    }

    fn socket_send_buffer_size(&self) -> Option<usize> {
        self.socket_send_buffer_size
    }
}

impl<F: Send + Sync + Clone + FnOnce() + 'static> FrameHandler for MockTcpFrameHandler<F> {
    fn content_type(&self) -> String {
        self.frame_handler.content_type()
    }

    fn max_frame_length(&self) -> usize {
        self.frame_handler.max_frame_length()
    }

    fn handle_event(&self, received_from: Option<Bytes>, frame: Bytes) -> Option<Event> {
        self.frame_handler.handle_event(received_from, frame)
    }

    fn multithreaded(&self) -> bool {
        self.frame_handler.multithreaded()
    }

    fn max_frame_handling_tasks(&self) -> usize {
        self.frame_handler.max_frame_handling_tasks()
    }

    fn host_key(&self) -> &Option<OwnedValuePath> {
        self.frame_handler.host_key()
    }

    fn timestamp_key(&self) -> Option<&OwnedValuePath> {
        self.frame_handler.timestamp_key()
    }

    fn source_type_key(&self) -> Option<&OwnedValuePath> {
        self.frame_handler.source_type_key()
    }
}

impl<F: Send + Sync + Clone + FnOnce() + 'static> TcpFrameHandler for MockTcpFrameHandler<F> {
    fn address(&self) -> SocketListenAddr {
        self.address
    }

    fn keepalive(&self) -> Option<TcpKeepaliveConfig> {
        self.keepalive
    }

    fn shutdown_timeout_secs(&self) -> Duration {
        self.shutdown_timeout_secs
    }

    fn tls(&self) -> MaybeTlsSettings {
        self.tls.clone()
    }

    fn tls_client_metadata_key(&self) -> Option<OwnedValuePath> {
        self.tls_client_metadata_key.clone()
    }

    fn receive_buffer_bytes(&self) -> Option<usize> {
        self.receive_buffer_bytes
    }

    fn max_connection_duration_secs(&self) -> Option<u64> {
        self.max_connection_duration_secs
    }

    fn max_connections(&self) -> Option<u32> {
        self.max_connections
    }

    fn insert_tls_client_metadata(&mut self, _: Option<CertificateMetadata>) {}

    fn allowed_origins(&self) -> Option<&[IpNet]> {
        self.permit_origin.as_deref()
    }
}

fn init_framestream_tcp(
    source_id: &str,
    addr: &SocketAddr,
    frame_handler: impl TcpFrameHandler + Send + Sync + Clone + 'static,
    pipeline: SourceSender,
) -> (JoinHandle<Result<(), ()>>, SourceShutdownCoordinator) {
    let source_id = ComponentKey::from(source_id);
    let mut shutdown = SourceShutdownCoordinator::default();
    let (shutdown_signal, _) = shutdown.register_source(&source_id, false);
    let server = build_framestream_tcp_source(frame_handler, shutdown_signal, pipeline)
        .expect("Failed to build framestream tcp source.");

    let join_handle = tokio::spawn(server);

    while std::net::TcpStream::connect(addr).is_err() {
        thread::sleep(Duration::from_millis(2));
    }

    (join_handle, shutdown)
}

fn init_framestream_unix(
    source_id: &str,
    frame_handler: impl UnixFrameHandler + Send + Sync + Clone + 'static,
    pipeline: SourceSender,
) -> (
    PathBuf,
    JoinHandle<Result<(), ()>>,
    SourceShutdownCoordinator,
) {
    let source_id = ComponentKey::from(source_id);
    let socket_path = frame_handler.socket_path();
    let mut shutdown = SourceShutdownCoordinator::default();
    let (shutdown_signal, _) = shutdown.register_source(&source_id, false);
    let server = build_framestream_unix_source(frame_handler, shutdown_signal, pipeline)
        .expect("Failed to build framestream unix source.");

    let join_handle = tokio::spawn(server);

    // Wait for server to accept traffic
    while std::os::unix::net::UnixStream::connect(&socket_path).is_err() {
        thread::sleep(Duration::from_millis(2));
    }

    (socket_path, join_handle, shutdown)
}

async fn make_tcp_stream(
    addr: SocketAddr,
) -> Framed<TcpStream, length_delimited::LengthDelimitedCodec> {
    let socket = TcpStream::connect(&addr).await.unwrap();
    Framed::new(socket, length_delimited::Builder::new().new_codec())
}

async fn make_unix_stream(
    path: PathBuf,
) -> Framed<UnixStream, length_delimited::LengthDelimitedCodec> {
    let socket = UnixStream::connect(&path).await.unwrap();
    Framed::new(socket, length_delimited::Builder::new().new_codec())
}

async fn send_data_frames<S: Sink<Bytes, Error = std::io::Error> + Unpin>(
    sock_sink: &mut S,
    frames: Vec<Result<Bytes, std::io::Error>>,
) {
    let mut stream = stream::iter(frames);
    //send and send_all consume the sink
    _ = sock_sink.send_all(&mut stream).await;
}

async fn send_control_frame<S: Sink<Bytes, Error = std::io::Error> + Unpin>(
    sock_sink: &mut S,
    frame: Bytes,
) {
    send_data_frames(sock_sink, vec![Ok(Bytes::new()), Ok(frame)]).await; //send empty frame to say we are control frame
}

fn create_control_frame(header: ControlHeader) -> Bytes {
    Bytes::from(header.to_u32().to_be_bytes().to_vec())
}

fn create_control_frame_with_content(header: ControlHeader, content_types: Vec<Bytes>) -> Bytes {
    let mut frame = BytesMut::from(&header.to_u32().to_be_bytes()[..]);
    for content_type in content_types {
        frame.extend(ControlField::ContentType.to_u32().to_be_bytes());
        frame.extend((content_type.len() as u32).to_be_bytes());
        frame.extend(content_type.clone());
    }
    Bytes::from(frame)
}

fn assert_accept_frame(frame: &mut BytesMut, expected_content_type: Bytes) {
    //frame should start with 4 bytes saying ACCEPT

    assert_eq!(&frame[..4], &ControlHeader::Accept.to_u32().to_be_bytes(),);
    frame.advance(4);
    //next should be content type field
    assert_eq!(
        &frame[..4],
        &ControlField::ContentType.to_u32().to_be_bytes(),
    );
    frame.advance(4);
    //next should be length of content_type
    assert_eq!(
        &frame[..4],
        &(expected_content_type.len() as u32).to_be_bytes(),
    );
    frame.advance(4);
    //rest should be content type
    assert_eq!(&frame[..], &expected_content_type[..]);
}

fn create_frame_handler(multithreaded: bool) -> impl UnixFrameHandler + Send + Sync + Clone {
    MockUnixFrameHandler::new("test_content".to_string(), multithreaded, move || {})
}

fn create_tcp_frame_handler(
    addr: SocketAddr,
    multithreaded: bool,
    permit_origin: Option<Vec<IpNet>>,
) -> impl TcpFrameHandler + Send + Sync + Clone {
    MockTcpFrameHandler::new(
        addr,
        "test_content".to_string(),
        multithreaded,
        move || {},
        permit_origin,
    )
}

async fn signal_shutdown(source_name: &str, shutdown: &mut SourceShutdownCoordinator) {
    // Now signal to the Source to shut down.
    let deadline = Instant::now() + Duration::from_secs(10);
    let id = ComponentKey::from(source_name);
    let shutdown_complete = shutdown.shutdown_source(&id, deadline);
    let shutdown_success = shutdown_complete.await;
    assert!(shutdown_success);
}

async fn test_normal_framestream<
    T: Sink<Bytes, Error = std::io::Error> + Unpin,
    U: Stream<Item = Result<BytesMut, std::io::Error>> + Unpin,
    V: Stream<Item = Event> + Unpin,
>(
    source_name: &str,
    mut sock_sink: T,
    mut sock_stream: U,
    rx: V,
    mut shutdown: SourceShutdownCoordinator,
    source_handle: JoinHandle<Result<(), ()>>,
) {
    //1 - send READY frame (with content_type)
    let content_type = Bytes::from(&b"test_content"[..]);
    let ready_msg =
        create_control_frame_with_content(ControlHeader::Ready, vec![content_type.clone()]);
    send_control_frame(&mut sock_sink, ready_msg).await;

    //2 - wait for ACCEPT frame
    let mut frame_vec = collect_n_stream(&mut sock_stream, 2).await;
    //take second element, because first will be empty (signifying control frame)
    assert_eq!(frame_vec[0].as_ref().unwrap().len(), 0);
    assert_accept_frame(frame_vec[1].as_mut().unwrap(), content_type);

    //3 - send START frame
    send_control_frame(&mut sock_sink, create_control_frame(ControlHeader::Start)).await;

    //4 - send data
    send_data_frames(
        &mut sock_sink,
        vec![Ok(Bytes::from("hello")), Ok(Bytes::from("world"))],
    )
    .await;
    let events = collect_n(rx, 2).await;

    //5 - send STOP frame
    send_control_frame(&mut sock_sink, create_control_frame(ControlHeader::Stop)).await;

    let message_key = log_schema().message_key().unwrap().to_string();
    assert!(
        events
            .iter()
            .any(|e| e.as_log()[&message_key] == "hello".into())
    );
    assert!(
        events
            .iter()
            .any(|e| e.as_log()[&message_key] == "world".into())
    );

    drop(sock_stream); //explicitly drop the stream so we don't get warnings about not using it

    // Ensure source actually shut down successfully.
    signal_shutdown(source_name, &mut shutdown).await;
    _ = source_handle.await.unwrap();
}

async fn test_multiple_content_types<
    T: Sink<Bytes, Error = std::io::Error> + Unpin,
    U: Stream<Item = Result<BytesMut, std::io::Error>> + Unpin,
>(
    source_name: &str,
    mut sock_sink: T,
    mut sock_stream: U,
    mut shutdown: SourceShutdownCoordinator,
    source_handle: JoinHandle<Result<(), ()>>,
) {
    //1 - send READY frame (with content_type)
    let content_type = Bytes::from(&b"test_content"[..]);
    let ready_msg = create_control_frame_with_content(
        ControlHeader::Ready,
        vec![Bytes::from(&b"test_content2"[..]), content_type.clone()],
    ); //can have multiple content types
    send_control_frame(&mut sock_sink, ready_msg).await;

    //2 - wait for ACCEPT frame
    let mut frame_vec = collect_n_stream(&mut sock_stream, 2).await;

    //take second element, because first will be empty (signifying control frame)
    assert_eq!(frame_vec[0].as_ref().unwrap().len(), 0);
    assert_accept_frame(frame_vec[1].as_mut().unwrap(), content_type);

    drop(sock_stream); //explicitly drop the stream so we don't get warnings about not using it

    // Ensure source actually shut down successfully.
    signal_shutdown(source_name, &mut shutdown).await;
    _ = source_handle.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[should_panic]
async fn blocked_framestream_tcp() {
    let source_name = "test_source";
    let (tx, rx) = SourceSender::new_test();
    let (_guard, addr) = next_addr();
    let (source_handle, shutdown) = init_framestream_tcp(
        source_name,
        &addr,
        create_tcp_frame_handler(addr, false, Some(vec![])),
        tx,
    );
    let (sock_sink, sock_stream) = make_tcp_stream(addr).await.split();

    test_normal_framestream(
        source_name,
        sock_sink,
        sock_stream,
        rx,
        shutdown,
        source_handle,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn normal_framestream_singlethreaded_tcp() {
    let source_name = "test_source";
    let (tx, rx) = SourceSender::new_test();
    let (_guard, addr) = next_addr();
    let (source_handle, shutdown) = init_framestream_tcp(
        source_name,
        &addr,
        create_tcp_frame_handler(addr, false, None),
        tx,
    );
    let (sock_sink, sock_stream) = make_tcp_stream(addr).await.split();

    test_normal_framestream(
        source_name,
        sock_sink,
        sock_stream,
        rx,
        shutdown,
        source_handle,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn normal_framestream_singlethreaded_unix() {
    let source_name = "test_source";
    let (tx, rx) = SourceSender::new_test();
    let (path, source_handle, shutdown) =
        init_framestream_unix(source_name, create_frame_handler(false), tx);
    let (sock_sink, sock_stream) = make_unix_stream(path).await.split();

    test_normal_framestream(
        source_name,
        sock_sink,
        sock_stream,
        rx,
        shutdown,
        source_handle,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn normal_framestream_multithreaded_tcp() {
    let source_name = "test_source";
    let (tx, rx) = SourceSender::new_test();
    let (_guard, addr) = next_addr();
    let (source_handle, shutdown) = init_framestream_tcp(
        source_name,
        &addr,
        create_tcp_frame_handler(addr, true, None),
        tx,
    );
    let (sock_sink, sock_stream) = make_tcp_stream(addr).await.split();

    test_normal_framestream(
        source_name,
        sock_sink,
        sock_stream,
        rx,
        shutdown,
        source_handle,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn normal_framestream_multithreaded_unix() {
    let source_name = "test_source";
    let (tx, rx) = SourceSender::new_test();
    let (path, source_handle, shutdown) =
        init_framestream_unix(source_name, create_frame_handler(true), tx);
    let (sock_sink, sock_stream) = make_unix_stream(path).await.split();

    test_normal_framestream(
        source_name,
        sock_sink,
        sock_stream,
        rx,
        shutdown,
        source_handle,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn multiple_content_types_tcp() {
    let source_name = "test_source";
    let (tx, _) = SourceSender::new_test();
    let (_guard, addr) = next_addr();
    let (source_handle, shutdown) = init_framestream_tcp(
        source_name,
        &addr,
        create_tcp_frame_handler(addr, false, None),
        tx,
    );
    let (sock_sink, sock_stream) = make_tcp_stream(addr).await.split();

    test_multiple_content_types(source_name, sock_sink, sock_stream, shutdown, source_handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn multiple_content_types_unix() {
    let source_name = "test_source";
    let (tx, _) = SourceSender::new_test();
    let (path, source_handle, shutdown) =
        init_framestream_unix(source_name, create_frame_handler(false), tx);
    let (sock_sink, sock_stream) = make_unix_stream(path).await.split();

    test_multiple_content_types(source_name, sock_sink, sock_stream, shutdown, source_handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn wrong_content_type() {
    let source_name = "test_source";
    let (tx, _) = SourceSender::new_test();
    let (path, source_handle, mut shutdown) =
        init_framestream_unix(source_name, create_frame_handler(false), tx);
    let (mut sock_sink, mut sock_stream) = make_unix_stream(path).await.split();

    //1 - send READY frame (with WRONG content_type)
    let ready_msg = create_control_frame_with_content(
        ControlHeader::Ready,
        vec![Bytes::from(&b"test_content2"[..])],
    ); //can have multiple content types
    send_control_frame(&mut sock_sink, ready_msg).await;

    //2 - send READY frame (with RIGHT content_type)
    let content_type = Bytes::from(&b"test_content"[..]);
    let ready_msg =
        create_control_frame_with_content(ControlHeader::Ready, vec![content_type.clone()]);
    send_control_frame(&mut sock_sink, ready_msg).await;

    //3 - wait for ACCEPT frame
    let mut frame_vec = collect_n_stream(&mut sock_stream, 2).await;

    //take second element, because first will be empty (signifying control frame)
    assert_eq!(frame_vec[0].as_ref().unwrap().len(), 0);
    assert_accept_frame(frame_vec[1].as_mut().unwrap(), content_type);

    drop(sock_stream); //explicitly drop the stream so we don't get warnings about not using it

    // Ensure source actually shut down successfully.
    signal_shutdown(source_name, &mut shutdown).await;
    _ = source_handle.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn data_too_soon() {
    let source_name = "test_source";
    let (tx, rx) = SourceSender::new_test();
    let (path, source_handle, mut shutdown) =
        init_framestream_unix(source_name, create_frame_handler(false), tx);
    let (mut sock_sink, mut sock_stream) = make_unix_stream(path).await.split();

    //1 - send data frame (too soon!)
    send_data_frames(
        &mut sock_sink,
        vec![Ok(Bytes::from("bad")), Ok(Bytes::from("data"))],
    )
    .await;

    //2 - send READY frame (with content_type)
    let content_type = Bytes::from(&b"test_content"[..]);
    let ready_msg =
        create_control_frame_with_content(ControlHeader::Ready, vec![content_type.clone()]);
    send_control_frame(&mut sock_sink, ready_msg).await;

    //3 - wait for ACCEPT frame
    let mut frame_vec = collect_n_stream(&mut sock_stream, 2).await;

    //take second element, because first will be empty (signifying control frame)
    assert_eq!(frame_vec[0].as_ref().unwrap().len(), 0);
    assert_accept_frame(frame_vec[1].as_mut().unwrap(), content_type);

    //4 - send START frame
    send_control_frame(&mut sock_sink, create_control_frame(ControlHeader::Start)).await;

    //5 - send data (will go through)
    send_data_frames(
        &mut sock_sink,
        vec![Ok(Bytes::from("hello")), Ok(Bytes::from("world"))],
    )
    .await;
    let events = collect_n(rx, 2).await;

    //6 - send STOP frame
    send_control_frame(&mut sock_sink, create_control_frame(ControlHeader::Stop)).await;

    assert_eq!(
        events[0].as_log()[log_schema().message_key().unwrap().to_string()],
        "hello".into(),
    );
    assert_eq!(
        events[1].as_log()[log_schema().message_key().unwrap().to_string()],
        "world".into(),
    );

    drop(sock_stream); //explicitly drop the stream so we don't get warnings about not using it

    // Ensure source actually shut down successfully.
    signal_shutdown(source_name, &mut shutdown).await;
    _ = source_handle.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn unidirectional_framestream() {
    let source_name = "test_source";
    let (tx, rx) = SourceSender::new_test();
    let (path, source_handle, mut shutdown) =
        init_framestream_unix(source_name, create_frame_handler(false), tx);
    let (mut sock_sink, _) = make_unix_stream(path).await.split();

    //1 - send START frame (with content_type)
    let content_type = Bytes::from(&b"test_content"[..]);
    let start_msg = create_control_frame_with_content(ControlHeader::Start, vec![content_type]);
    send_control_frame(&mut sock_sink, start_msg).await;

    //4 - send data
    send_data_frames(
        &mut sock_sink,
        vec![Ok(Bytes::from("hello")), Ok(Bytes::from("world"))],
    )
    .await;
    let events = collect_n(rx, 2).await;

    //5 - send STOP frame
    send_control_frame(&mut sock_sink, create_control_frame(ControlHeader::Stop)).await;

    assert_eq!(
        events[0].as_log()[log_schema().message_key().unwrap().to_string()],
        "hello".into(),
    );
    assert_eq!(
        events[1].as_log()[log_schema().message_key().unwrap().to_string()],
        "world".into(),
    );

    // Ensure source actually shut down successfully.
    signal_shutdown(source_name, &mut shutdown).await;
    _ = source_handle.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_spawn_event_handling_tasks() {
    let (out, rx) = SourceSender::new_test();

    let max_frame_handling_tasks = 20;
    let active_task_nums = Arc::new(AtomicUsize::new(0));
    let active_task_nums_copy = Arc::clone(&active_task_nums);
    let max_task_nums_reached = Arc::new(AtomicUsize::new(0));
    let max_task_nums_reached_copy = Arc::clone(&max_task_nums_reached);

    let mut join_handles = vec![];
    let active_task_nums_copy_2 = Arc::clone(&active_task_nums_copy);
    let extra_routine = move || {
        thread::sleep(Duration::from_millis(10));
        max_task_nums_reached_copy.fetch_max(
            active_task_nums_copy_2.load(Ordering::Acquire),
            Ordering::AcqRel,
        );
    };

    let total_events = max_frame_handling_tasks * 10;

    join_handles.push(tokio::spawn(async move {
        future::ready({
            let events = collect_n(rx, total_events).await;
            assert_eq!(total_events, events.len(), "Missed events");
        })
        .await;
    }));

    for i in 0..total_events {
        join_handles.push(
            spawn_event_handling_tasks(
                Bytes::from(format!("event_{i}")),
                MockFrameHandler::new("test_content".to_string(), true, extra_routine.clone()),
                out.clone(),
                None,
                Arc::clone(&active_task_nums_copy),
                max_frame_handling_tasks,
            )
            .await,
        );
    }

    future::join_all(join_handles).await;

    let final_task_nums = active_task_nums.load(Ordering::Acquire);
    assert_eq!(
        0, final_task_nums,
        "There should be NO left-over tasks at the end"
    );

    let max_task_nums_reached_value = max_task_nums_reached.load(Ordering::Acquire);
    assert!(
        max_task_nums_reached_value > 1,
        "MultiThreaded mode does NOT work"
    );
    assert!(
        (max_task_nums_reached_value - max_frame_handling_tasks) < 2,
        "Max number of tasks at any given time should NOT Exceed max_frame_handling_tasks too much"
    );
}

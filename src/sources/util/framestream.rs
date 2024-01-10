#[cfg(unix)]
use std::os::unix::{fs::PermissionsExt, io::AsRawFd};
use std::{
    convert::TryInto,
    fs,
    marker::{Send, Sync},
    path::PathBuf,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

use bytes::{Buf, Bytes, BytesMut};
use futures::{
    executor::block_on,
    future,
    sink::{Sink, SinkExt},
    stream::{self, StreamExt, TryStreamExt},
};
use tokio::{self, net::UnixListener, task::JoinHandle};
use tokio_stream::wrappers::UnixListenerStream;
use tokio_util::codec::{length_delimited, Framed};
use tracing::{field, Instrument};
use vector_lib::lookup::OwnedValuePath;

use crate::{
    event::Event,
    internal_events::{UnixSocketError, UnixSocketFileDeleteError},
    shutdown::ShutdownSignal,
    sources::Source,
    SourceSender,
};

const FSTRM_CONTROL_FRAME_LENGTH_MAX: usize = 512;
const FSTRM_CONTROL_FIELD_CONTENT_TYPE_LENGTH_MAX: usize = 256;

pub type FrameStreamSink = Box<dyn Sink<Bytes, Error = std::io::Error> + Send + Unpin>;

struct FrameStreamReader {
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
    fn socket_path(&self) -> PathBuf;
    fn multithreaded(&self) -> bool;
    fn max_frame_handling_tasks(&self) -> u32;
    fn socket_file_mode(&self) -> Option<u32>;
    fn socket_receive_buffer_size(&self) -> Option<usize>;
    fn socket_send_buffer_size(&self) -> Option<usize>;
    fn host_key(&self) -> &Option<OwnedValuePath>;
    fn timestamp_key(&self) -> Option<&OwnedValuePath>;
    fn source_type_key(&self) -> Option<&OwnedValuePath>;
}

/**
 * Based off of the build_unix_source function.
 * Functions similarly, but uses the FrameStreamReader to deal with
 * framestream control packets, and responds appropriately.
 **/
pub fn build_framestream_unix_source(
    frame_handler: impl FrameHandler + Send + Sync + Clone + 'static,
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
            listener.as_raw_fd(),
            nix::sys::socket::sockopt::RcvBuf,
            &(socket_receive_buffer_size),
        );
        let rcv_buf_size =
            nix::sys::socket::getsockopt(listener.as_raw_fd(), nix::sys::socket::sockopt::RcvBuf);
        info!(
            "Unix socket receive buffer size modified to {}.",
            rcv_buf_size.unwrap()
        );
    }

    // system's 'net.core.wmem_max' might have to be changed if socket send buffer is not updated properly
    if let Some(socket_send_buffer_size) = frame_handler.socket_send_buffer_size() {
        _ = nix::sys::socket::setsockopt(
            listener.as_raw_fd(),
            nix::sys::socket::sockopt::SndBuf,
            &(socket_send_buffer_size),
        );
        let snd_buf_size =
            nix::sys::socket::getsockopt(listener.as_raw_fd(), nix::sys::socket::sockopt::SndBuf);
        info!(
            "Unix socket buffer send size modified to {}.",
            snd_buf_size.unwrap()
        );
    }

    // the permissions to unix socket are restricted from 0o700 to 0o777, which are 448 and 511 in decimal
    if let Some(socket_permission) = frame_handler.socket_file_mode() {
        if !(448..=511).contains(&socket_permission) {
            return Err(format!(
                "Invalid Socket permission {:#o}. Must between 0o700 and 0o777.",
                socket_permission
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
        let active_parsing_task_nums = Arc::new(AtomicU32::new(0));

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
            let content_type = frame_handler.content_type();
            let listen_path = path.clone();
            let mut event_sink = out.clone();
            let active_task_nums_ = Arc::clone(&active_parsing_task_nums);

            let span = info_span!("connection");
            let path = if let Some(addr) = peer_addr {
                if let Some(path) = addr.as_pathname().map(|e| e.to_owned()) {
                    span.record("peer_path", &field::debug(&path));
                    Some(path)
                } else {
                    None
                }
            } else {
                None
            };
            let received_from: Option<Bytes> =
                path.map(|p| p.to_string_lossy().into_owned().into());

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
                .take_until(shutdown.clone())
                .map_err(move |error| {
                    emit!(UnixSocketError {
                        error: &error,
                        path: &listen_path,
                    });
                })
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
                            future::ready({
                                let max_frame_handling_tasks =
                                    frame_handler_copy.max_frame_handling_tasks();
                                let f_handler = frame_handler_copy.clone();
                                let received_from_copy = received_from.clone();
                                let event_sink_copy = event_sink.clone();
                                let active_task_nums_copy = Arc::clone(&active_task_nums_);

                                spawn_event_handling_tasks(
                                    f,
                                    f_handler,
                                    event_sink_copy,
                                    received_from_copy,
                                    active_task_nums_copy,
                                    max_frame_handling_tasks,
                                );
                            })
                        })
                        .await;
                    info!("Finished sending.");
                };
                tokio::spawn(handler.instrument(span.or_current()));
            }
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

fn spawn_event_handling_tasks(
    event_data: Bytes,
    event_handler: impl FrameHandler + Send + Sync + 'static,
    mut event_sink: SourceSender,
    received_from: Option<Bytes>,
    active_task_nums: Arc<AtomicU32>,
    max_frame_handling_tasks: u32,
) -> JoinHandle<()> {
    wait_for_task_quota(&active_task_nums, max_frame_handling_tasks);

    tokio::spawn(async move {
        future::ready({
            if let Some(evt) = event_handler.handle_event(received_from, event_data) {
                if event_sink.send_event(evt).await.is_err() {
                    error!("Encountered error while sending event.");
                }
            }
            active_task_nums.fetch_sub(1, Ordering::AcqRel);
        })
        .await;
    })
}

fn wait_for_task_quota(active_task_nums: &Arc<AtomicU32>, max_tasks: u32) {
    while max_tasks > 0 && max_tasks < active_task_nums.load(Ordering::Acquire) {
        thread::sleep(Duration::from_millis(3));
    }
    active_task_nums.fetch_add(1, Ordering::AcqRel);
}

#[cfg(test)]
mod test {
    #[cfg(unix)]
    use std::{
        path::PathBuf,
        sync::{
            atomic::{AtomicU32, Ordering},
            Arc,
        },
        thread,
    };

    use bytes::{buf::Buf, Bytes, BytesMut};
    use futures::{
        future,
        sink::{Sink, SinkExt},
        stream::{self, StreamExt},
    };
    use tokio::{
        self,
        net::UnixStream,
        task::JoinHandle,
        time::{Duration, Instant},
    };
    use tokio_util::codec::{length_delimited, Framed};
    use vector_lib::config::{LegacyKey, LogNamespace};
    use vector_lib::lookup::{owned_value_path, path, OwnedValuePath};

    use super::{
        build_framestream_unix_source, spawn_event_handling_tasks, ControlField, ControlHeader,
        FrameHandler,
    };
    use crate::{
        config::{log_schema, ComponentKey},
        event::{Event, LogEvent},
        shutdown::SourceShutdownCoordinator,
        test_util::{collect_n, collect_n_stream},
        SourceSender,
    };

    #[derive(Clone)]
    struct MockFrameHandler<F: Send + Sync + Clone + FnOnce() + 'static> {
        content_type: String,
        max_frame_length: usize,
        socket_path: PathBuf,
        multithreaded: bool,
        max_frame_handling_tasks: u32,
        socket_file_mode: Option<u32>,
        socket_receive_buffer_size: Option<usize>,
        socket_send_buffer_size: Option<usize>,
        extra_task_handling_routine: F,
        host_key: Option<OwnedValuePath>,
        timestamp_key: Option<OwnedValuePath>,
        source_type_key: Option<OwnedValuePath>,
        log_namespace: LogNamespace,
    }

    impl<F: Send + Sync + Clone + FnOnce() + 'static> MockFrameHandler<F> {
        pub fn new(content_type: String, multithreaded: bool, extra_routine: F) -> Self {
            Self {
                content_type,
                max_frame_length: bytesize::kib(100u64) as usize,
                socket_path: tempfile::tempdir().unwrap().into_path().join("unix_test"),
                multithreaded,
                max_frame_handling_tasks: 0,
                socket_file_mode: None,
                socket_receive_buffer_size: None,
                socket_send_buffer_size: None,
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

        fn socket_path(&self) -> PathBuf {
            self.socket_path.clone()
        }
        fn multithreaded(&self) -> bool {
            self.multithreaded
        }
        fn max_frame_handling_tasks(&self) -> u32 {
            self.max_frame_handling_tasks
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

    fn init_framestream_unix(
        source_id: &str,
        frame_handler: impl FrameHandler + Send + Sync + Clone + 'static,
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
        let mut stream = stream::iter(frames.into_iter());
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

    fn create_control_frame_with_content(
        header: ControlHeader,
        content_types: Vec<Bytes>,
    ) -> Bytes {
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

    fn create_frame_handler(multithreaded: bool) -> impl FrameHandler + Send + Sync + Clone {
        MockFrameHandler::new("test_content".to_string(), multithreaded, move || {})
    }

    async fn signal_shutdown(source_name: &str, shutdown: &mut SourceShutdownCoordinator) {
        // Now signal to the Source to shut down.
        let deadline = Instant::now() + Duration::from_secs(10);
        let id = ComponentKey::from(source_name);
        let shutdown_complete = shutdown.shutdown_source(&id, deadline);
        let shutdown_success = shutdown_complete.await;
        assert!(shutdown_success);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn normal_framestream_singlethreaded() {
        let source_name = "test_source";
        let (tx, rx) = SourceSender::new_test();
        let (path, source_handle, mut shutdown) =
            init_framestream_unix(source_name, create_frame_handler(false), tx);
        let (mut sock_sink, mut sock_stream) = make_unix_stream(path).await.split();

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
    async fn normal_framestream_multithreaded() {
        let source_name = "test_source";
        let (tx, rx) = SourceSender::new_test();
        let (path, source_handle, mut shutdown) =
            init_framestream_unix(source_name, create_frame_handler(true), tx);
        let (mut sock_sink, mut sock_stream) = make_unix_stream(path).await.split();

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
        assert!(events
            .iter()
            .any(|e| e.as_log()[&message_key] == "hello".into()));
        assert!(events
            .iter()
            .any(|e| e.as_log()[&message_key] == "world".into()));

        drop(sock_stream); //explicitly drop the stream so we don't get warnings about not using it

        // Ensure source actually shut down successfully.
        signal_shutdown(source_name, &mut shutdown).await;
        _ = source_handle.await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn multiple_content_types() {
        let source_name = "test_source";
        let (tx, _) = SourceSender::new_test();
        let (path, source_handle, mut shutdown) =
            init_framestream_unix(source_name, create_frame_handler(false), tx);
        let (mut sock_sink, mut sock_stream) = make_unix_stream(path).await.split();

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
        let active_task_nums = Arc::new(AtomicU32::new(0));
        let active_task_nums_copy = Arc::clone(&active_task_nums);
        let max_task_nums_reached = Arc::new(AtomicU32::new(0));
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
                let events = collect_n(rx, total_events as usize).await;
                assert_eq!(total_events as usize, events.len(), "Missed events");
            })
            .await;
        }));

        for i in 0..total_events {
            join_handles.push(spawn_event_handling_tasks(
                Bytes::from(format!("event_{}", i)),
                MockFrameHandler::new("test_content".to_string(), true, extra_routine.clone()),
                out.clone(),
                None,
                Arc::clone(&active_task_nums_copy),
                max_frame_handling_tasks,
            ));
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
        assert!((max_task_nums_reached_value - max_frame_handling_tasks) < 2, "Max number of tasks at any given time should NOT Exceed max_frame_handling_tasks too much");
    }
}

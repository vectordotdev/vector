use crate::{
    event::Event,
    internal_events::{SocketEventReceived, SocketMode, UnixSocketError},
    shutdown::ShutdownSignal,
    sources::Source,
    Pipeline,
};
use bytes::{Buf, Bytes, BytesMut};
use futures::{
    compat::{Future01CompatExt, Sink01CompatExt},
    executor::block_on,
    future,
    sink::{Sink, SinkExt},
    stream::{self, StreamExt, TryStreamExt},
    FutureExt, TryFutureExt,
};
use futures01::Sink as Sink01;
use std::convert::TryInto;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::marker::{Send, Sync};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicI32, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};
use tokio::{self, net::UnixListener, task::JoinHandle};
use tokio_util::codec::{length_delimited, Framed};
use tracing::field;
use tracing_futures::Instrument;

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
    fn new() -> Self {
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
                error!("Don't know header value {} (expected 0x01 - 0x05)", val);
                Err(())
            }
        }
    }

    fn to_u32(self) -> u32 {
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
                error!("Don't know field type {} (expected 0x01)", val);
                Err(())
            }
        }
    }
    fn to_u32(&self) -> u32 {
        match self {
            ControlField::ContentType => 0x01,
        }
    }
}

fn advance_u32(b: &mut Bytes) -> Result<u32, ()> {
    if b.len() < 4 {
        error!("Malformed frame");
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
            let _ = self.handle_control_frame(frame);
            None
        } else {
            //data frame
            if self.state.control_state == ControlState::ReadingData {
                emit!(SocketEventReceived {
                    byte_size: frame.len(),
                    mode: SocketMode::Unix,
                });
                Some(frame) //return data frame
            } else {
                error!(
                    "Received a data frame while in state {:?}",
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
                        let _ = self.process_fields(header, &mut frame)?;
                        //if didn't error, then we are ok to change state
                        self.state.control_state = ControlState::ReadingData;
                        self.state.is_bidirectional = false; //if first message was START then we are unidirectional (no responses)
                    }
                    _ => error!("Got wrong control frame, expected READY"),
                }
            }
            ControlState::GotReady => {
                match header {
                    ControlHeader::Start => {
                        //check for content type
                        let _ = self.process_fields(header, &mut frame)?;
                        //if didn't error, then we are ok to change state
                        self.state.control_state = ControlState::ReadingData;
                    }
                    _ => error!("Got wrong control frame, expected START"),
                }
            }
            ControlState::ReadingData => {
                match header {
                    ControlHeader::Stop => {
                        //check there aren't any fields
                        let _ = self.process_fields(header, &mut frame)?;
                        if self.state.is_bidirectional {
                            //send FINISH frame -- but only if we are bidirectional
                            self.send_control_frame(Self::make_frame(ControlHeader::Finish, None));
                        }
                        self.state.control_state = ControlState::Stopped; //stream is now done
                    }
                    _ => error!("Got wrong control frame, expected STOP"),
                }
            }
            ControlState::Stopped => error!("Unexpected control frame, current state is STOPPED"),
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
                    error!("Unexpected fields in STOP header");
                    Err(())
                } else {
                    Ok(None)
                }
            }
            _ => {
                error!("Unexpected control header value {:?}", header.to_u32());
                Err(())
            }
        }
    }

    fn process_content_type(&self, frame: &mut Bytes, is_start_frame: bool) -> Result<String, ()> {
        if frame.is_empty() {
            error!("No fields in control frame");
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
                        error!("Content-Type string is too long");
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
                "START control frame can only have one content-type provided (got {})",
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
            "Content types did not match up. Expected {} got {:?}",
            self.expected_content_type, content_types
        );
        Err(())
    }

    fn make_frame(header: ControlHeader, content_type: Option<String>) -> Bytes {
        let mut frame = BytesMut::new();
        frame.extend(&header.to_u32().to_be_bytes());
        if let Some(s) = content_type {
            frame.extend(&ControlField::ContentType.to_u32().to_be_bytes()); //field type: ContentType
            frame.extend(&(s.len() as u32).to_be_bytes()); //length of type
            frame.extend(s.as_bytes());
        }
        Bytes::from(frame)
    }

    fn send_control_frame(&mut self, frame: Bytes) {
        let empty_frame = Bytes::from(&b""[..]); //send empty frame to say we are control frame
        let mut stream = stream::iter(vec![Ok(empty_frame), Ok(frame)].into_iter());

        if let Err(e) = block_on(self.response_sink.lock().unwrap().send_all(&mut stream)) {
            error!("Encountered error '{:#?}' while sending control frame", e);
        }
    }
}

pub trait FrameHandler {
    fn content_type(&self) -> String;
    fn max_length(&self) -> usize;
    fn host_key(&self) -> String;
    fn handle_event(&self, received_from: Option<Bytes>, frame: Bytes) -> Option<Event>;
    fn socket_path(&self) -> PathBuf;
    fn multithreaded(&self) -> bool;
    fn max_frame_handling_tasks(&self) -> i32;
    fn socket_file_mode(&self) -> Option<u32>;
}

/**
 * Based off of the build_unix_source function.
 * Functions similarly, but uses the FrameStreamReader to deal with
 * framestream control packets, and responds appropriately.
 **/
pub fn build_framestream_unix_source(
    frame_handler: impl FrameHandler + Send + Sync + Clone + 'static,
    shutdown: ShutdownSignal,
    out: Pipeline,
) -> Source {
    let path = frame_handler.socket_path();

    let out = out.sink_map_err(|e| error!("error sending event: {:?}", e));

    //check if the path already exists (and try to delete it)
    match fs::metadata(&path) {
        Ok(_) => {
            //exists, so try to delete it
            info!(message = "deleting file", ?path);
            fs::remove_file(&path).expect("failed to delete existing socket");
        }
        Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => {} //doesn't exist, do nothing
        Err(e) => error!("failed to bind to listener socket; error = {:?}", e),
    };

    let fut = async move {
        let mut listener = UnixListener::bind(&path).expect("failed to bind to listener socket");
        
        // the permissions to unix socket are restricted from 0o700 to 0o777, which are 448 and 511 in decimal
        if let Some(socket_permission) = frame_handler.socket_file_mode() {
            if socket_permission < 448 || socket_permission > 511 { 
                panic!("Invalid Socket permission");
            }
            match fs::set_permissions(&path, fs::Permissions::from_mode(socket_permission)) {
                Ok(_) => {
                    info!("socket permissions updated to {:o}", socket_permission);
                }
                Err(e) => error!("failed to update listener socket permissions; error = {:?}", e),
            };
        };

        let parsing_task_counter = Arc::new(AtomicI32::new(0));

        info!(message = "listening...", ?path, r#type = "unix");

        let mut stream = listener.incoming().take_until(shutdown.clone().compat());
        while let Some(socket) = stream.next().await {
            let socket = match socket {
                Err(e) => {
                    error!("failed to accept socket; error = {:?}", e);
                    continue;
                }
                Ok(s) => s,
            };
            let peer_addr = socket.peer_addr().ok();
            let content_type = frame_handler.content_type();
            let listen_path = path.clone();
            let event_sink = out.clone();
            let task_counter = Arc::clone(&parsing_task_counter);

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
                    .max_frame_length(frame_handler.max_length())
                    .new_codec(),
            )
            .split();
            let mut fs_reader = FrameStreamReader::new(Box::new(sock_sink), content_type);
            let frame_handler_copy = frame_handler.clone();
            let frames = sock_stream
                .take_until(shutdown.clone().compat())
                .map_err(move |error| {
                    emit!(UnixSocketError {
                        error,
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
                    future::ready(
                        frame_handler_copy
                            .handle_event(received_from.clone(), f)
                            .map(Ok),
                    )
                });

                let handler = async move {
                    let _ = event_sink.sink_compat().send_all(&mut events).await;
                    info!("finished sending");

                    //TODO: shutdown
                    // let splitstream = events.get_ref().get_ref();
                    // let _ = socket.shutdown(std::net::Shutdown::Both);
                };
                tokio::spawn(handler.instrument(span));
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
                                let task_counter_copy = Arc::clone(&task_counter);

                                let event_handler = move || {
                                    if let Some(evt) = f_handler.handle_event(received_from_copy, f)
                                    {
                                        if let Err(err) = event_sink_copy.wait().send(evt) {
                                            error!(
                                                "Encountered error '{:#?}' while sending event",
                                                err
                                            );
                                        }
                                    }
                                };

                                spawn_event_handling_tasks(
                                    event_handler,
                                    task_counter_copy,
                                    max_frame_handling_tasks,
                                );
                            })
                        })
                        .await;
                    info!("finished sending");
                };
                tokio::spawn(handler.instrument(span));
            }
        }
        Ok(())
    };

    Box::new(fut.boxed().compat())
}

fn spawn_event_handling_tasks<F>(
    event_handler: F,
    task_counter: Arc<AtomicI32>,
    max_frame_handling_tasks: i32,
) -> JoinHandle<()>
where
    F: Send + Sync + Clone + FnOnce() -> () + 'static,
{
    wait_for_task_quota(&task_counter, max_frame_handling_tasks);

    tokio::spawn(async move {
        future::ready({
            event_handler();
            task_counter.fetch_sub(1, Ordering::Relaxed);
        })
        .await;
    })
}

fn wait_for_task_quota(task_counter: &Arc<AtomicI32>, max_tasks: i32) {
    while max_tasks > 0 && max_tasks < task_counter.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(3));
    }
    task_counter.fetch_add(1, Ordering::Relaxed);
}

#[cfg(test)]
mod test {
    use super::{
        build_framestream_unix_source, spawn_event_handling_tasks, ControlField, ControlHeader,
        FrameHandler,
    };
    use crate::test_util::{collect_n, collect_n_stream};
    use crate::{
        event::{self, Event},
        shutdown::SourceShutdownCoordinator,
        Pipeline,
    };
    use bytes::{buf::Buf, Bytes, BytesMut};
    use futures::{
        compat::Future01CompatExt,
        future,
        sink::{Sink, SinkExt},
        stream::{self, StreamExt},
    };
    use futures01::sync::mpsc;
    #[cfg(unix)]
    use std::{
        path::PathBuf,
        sync::{
            atomic::{AtomicI32, Ordering},
            Arc, Mutex,
        },
        thread,
    };
    use tokio::{
        self,
        net::UnixStream,
        task::JoinHandle,
        time::{Duration, Instant},
    };
    use tokio_util::codec::{length_delimited, Framed};

    #[derive(Clone)]
    struct MockFrameHandler {
        content_type: String,
        max_length: usize,
        host_key: String,
        socket_path: PathBuf,
        multithreaded: bool,
        max_frame_handling_tasks: i32,
        socket_file_mode: Option<u32>,
        
    }

    impl MockFrameHandler {
        pub fn new(content_type: String) -> Self {
            Self {
                content_type,
                max_length: bytesize::kib(100u64) as usize,
                host_key: "test_framestream".to_string(),
                socket_path: tempfile::tempdir().unwrap().into_path().join("unix_test"),
                multithreaded: false,
                max_frame_handling_tasks: 0,
                socket_file_mode: None,
            }
        }
    }

    impl FrameHandler for MockFrameHandler {
        fn content_type(&self) -> String {
            self.content_type.clone()
        }
        fn max_length(&self) -> usize {
            self.max_length
        }
        fn host_key(&self) -> String {
            self.host_key.clone()
        }

        fn handle_event(&self, received_from: Option<Bytes>, frame: Bytes) -> Option<Event> {
            let mut event = Event::from(frame);
            event
                .as_mut_log()
                .insert(event::log_schema().source_type_key(), "framestream");
            if let Some(host) = received_from {
                event.as_mut_log().insert(self.host_key(), host);
            }
            Some(event)
        }

        fn socket_path(&self) -> PathBuf {
            self.socket_path.clone()
        }
        fn multithreaded(&self) -> bool {
            self.multithreaded
        }
        fn max_frame_handling_tasks(&self) -> i32 {
            self.max_frame_handling_tasks
        }

        fn socket_file_mode(&self) -> Option<u32> {
            self.socket_file_mode
        }
    }

    fn init_framstream_unix(
        source_name: &str,
        frame_handler: MockFrameHandler,
        sender: mpsc::Sender<event::Event>,
    ) -> (
        PathBuf,
        JoinHandle<Result<(), ()>>,
        SourceShutdownCoordinator,
    ) {
        let socket_path = frame_handler.socket_path();
        let mut shutdown = SourceShutdownCoordinator::default();
        let (shutdown_signal, _) = shutdown.register_source(source_name);
        let server = build_framestream_unix_source(
            frame_handler,
            shutdown_signal,
            Pipeline::from_sender(sender),
        )
        .compat();

        // let mut rt = runtime::Runtime::new().unwrap();
        // rt.spawn(server);
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
        let _ = sock_sink.send_all(&mut stream).await;
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
            frame.extend(&ControlField::ContentType.to_u32().to_be_bytes());
            frame.extend(&(content_type.len() as u32).to_be_bytes());
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

    async fn signal_shutdown(source_name: &str, shutdown: &mut SourceShutdownCoordinator) {
        // Now signal to the Source to shut down.
        let deadline = Instant::now() + Duration::from_secs(10);
        let shutdown_complete = shutdown.shutdown_source(source_name, deadline);
        let shutdown_success = shutdown_complete.compat().await.unwrap();
        assert_eq!(true, shutdown_success);
    }

    #[tokio::test(threaded_scheduler)]
    async fn normal_framestream() {
        let source_name = "test_source";
        let (tx, rx) = mpsc::channel(2);
        let (path, source_handle, mut shutdown) = init_framstream_unix(
            source_name,
            MockFrameHandler::new("test_content".to_string()),
            tx,
        );
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
            vec![Ok(Bytes::from(&"hello"[..])), Ok(Bytes::from(&"world"[..]))],
        )
        .await;
        let events = collect_n(rx, 2).await.unwrap();

        //5 - send STOP frame
        send_control_frame(&mut sock_sink, create_control_frame(ControlHeader::Stop)).await;

        assert_eq!(
            events[0].as_log()[&event::log_schema().message_key()],
            "hello".into(),
        );
        assert_eq!(
            events[1].as_log()[&event::log_schema().message_key()],
            "world".into(),
        );

        std::mem::drop(sock_stream); //explicitly drop the stream so we don't get warnings about not using it

        // Ensure source actually shut down successfully.
        signal_shutdown(source_name, &mut shutdown).await;
        let _ = source_handle.await.unwrap();
    }

    #[tokio::test(threaded_scheduler)]
    async fn multiple_content_types() {
        let source_name = "test_source";
        let (tx, _) = mpsc::channel(2);
        let (path, source_handle, mut shutdown) = init_framstream_unix(
            source_name,
            MockFrameHandler::new("test_content".to_string()),
            tx,
        );
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

        std::mem::drop(sock_stream); //explicitly drop the stream so we don't get warnings about not using it

        // Ensure source actually shut down successfully.
        signal_shutdown(source_name, &mut shutdown).await;
        let _ = source_handle.await.unwrap();
    }

    #[tokio::test(threaded_scheduler)]
    async fn wrong_content_type() {
        let source_name = "test_source";
        let (tx, _) = mpsc::channel(2);
        let (path, source_handle, mut shutdown) = init_framstream_unix(
            source_name,
            MockFrameHandler::new("test_content".to_string()),
            tx,
        );
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

        std::mem::drop(sock_stream); //explicitly drop the stream so we don't get warnings about not using it

        // Ensure source actually shut down successfully.
        signal_shutdown(source_name, &mut shutdown).await;
        let _ = source_handle.await.unwrap();
    }

    #[tokio::test(threaded_scheduler)]
    async fn data_too_soon() {
        let source_name = "test_source";
        let (tx, rx) = mpsc::channel(2);
        let (path, source_handle, mut shutdown) = init_framstream_unix(
            source_name,
            MockFrameHandler::new("test_content".to_string()),
            tx,
        );
        let (mut sock_sink, mut sock_stream) = make_unix_stream(path).await.split();

        //1 - send data frame (too soon!)
        send_data_frames(
            &mut sock_sink,
            vec![Ok(Bytes::from(&"bad"[..])), Ok(Bytes::from(&"data"[..]))],
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
            vec![Ok(Bytes::from(&"hello"[..])), Ok(Bytes::from(&"world"[..]))],
        )
        .await;
        let events = collect_n(rx, 2).await.unwrap();

        //6 - send STOP frame
        send_control_frame(&mut sock_sink, create_control_frame(ControlHeader::Stop)).await;

        assert_eq!(
            events[0].as_log()[&event::log_schema().message_key()],
            "hello".into(),
        );
        assert_eq!(
            events[1].as_log()[&event::log_schema().message_key()],
            "world".into(),
        );

        std::mem::drop(sock_stream); //explicitly drop the stream so we don't get warnings about not using it

        // Ensure source actually shut down successfully.
        signal_shutdown(source_name, &mut shutdown).await;
        let _ = source_handle.await.unwrap();
    }

    #[tokio::test(threaded_scheduler)]
    async fn unidirectional_framestream() {
        let source_name = "test_source";
        let (tx, rx) = mpsc::channel(2);
        let (path, source_handle, mut shutdown) = init_framstream_unix(
            source_name,
            MockFrameHandler::new("test_content".to_string()),
            tx,
        );
        let (mut sock_sink, _) = make_unix_stream(path).await.split();

        //1 - send START frame (with content_type)
        let content_type = Bytes::from(&b"test_content"[..]);
        let start_msg = create_control_frame_with_content(ControlHeader::Start, vec![content_type]);
        send_control_frame(&mut sock_sink, start_msg).await;

        //4 - send data
        send_data_frames(
            &mut sock_sink,
            vec![Ok(Bytes::from(&"hello"[..])), Ok(Bytes::from(&"world"[..]))],
        )
        .await;
        let events = collect_n(rx, 2).await.unwrap();

        //5 - send STOP frame
        send_control_frame(&mut sock_sink, create_control_frame(ControlHeader::Stop)).await;

        assert_eq!(
            events[0].as_log()[&event::log_schema().message_key()],
            "hello".into(),
        );
        assert_eq!(
            events[1].as_log()[&event::log_schema().message_key()],
            "world".into(),
        );

        // std::mem::drop(sock_stream); //explicitly drop the stream so we don't get warnings about not using it

        // Ensure source actually shut down successfully.
        signal_shutdown(source_name, &mut shutdown).await;
        let _ = source_handle.await.unwrap();
    }

    #[tokio::test(threaded_scheduler)]
    async fn test_spawn_event_handling_tasks() {
        let max_frame_handling_tasks = 100;
        let task_counter = Arc::new(AtomicI32::new(0));
        let task_counter_copy = Arc::clone(&task_counter);
        let max_task_counter_value = Arc::new(Mutex::new(0));
        let max_task_counter_value_copy = Arc::clone(&max_task_counter_value);

        let mut handles = vec![];
        let task_counter_copy_2 = Arc::clone(&task_counter_copy);
        let mock_event_handler = move || {
            info!("{}", "mock event handler");
            let current_task_counter = task_counter_copy_2.load(Ordering::Relaxed);
            thread::sleep(Duration::from_millis(10));
            let mut max = max_task_counter_value_copy.lock().unwrap(); // Also to simulate writing to a single sink
            if *max < current_task_counter {
                *max = current_task_counter
            };
        };
        for _ in 0..max_frame_handling_tasks * 10 {
            handles.push(spawn_event_handling_tasks(
                mock_event_handler.clone(),
                Arc::clone(&task_counter_copy),
                max_frame_handling_tasks,
            ));
        }
        future::join_all(handles).await;

        let final_task_counter = task_counter.load(Ordering::Relaxed);
        assert_eq!(
            0, final_task_counter,
            "There should be NO left-over tasks at the end"
        );

        let max_counter_value = max_task_counter_value.lock().unwrap();
        assert!(*max_counter_value > 1, "MultiThreaded mode does NOT work");
        assert!((*max_counter_value - max_frame_handling_tasks) < 2, "Max number of tasks at any given time should NOT Exceed max_frame_handling_tasks too much");
    }
}

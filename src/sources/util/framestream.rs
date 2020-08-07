use crate::{
    event::Event,
    internal_events::{UnixSocketError, UnixSocketEventReceived},
    shutdown::ShutdownSignal,
    sources::Source,
    stream::StreamExt,
};
use bytes::Bytes;
use futures01::{future, sync::mpsc::Sender, Future, Sink, Stream};
use std::convert::TryInto;
use std::fs;
use std::marker::{Send, Sync};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicI32, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};
use tokio01::{
    self,
    codec::{length_delimited, Framed},
    executor::Spawn,
    sync::lock::Lock,
};
use tokio_uds::UnixListener;
use tracing::field;
use tracing_futures::Instrument;

const FSTRM_CONTROL_FRAME_LENGTH_MAX: usize = 512;
const FSTRM_CONTROL_FIELD_CONTENT_TYPE_LENGTH_MAX: usize = 256;

pub type FrameStreamSink = Box<dyn Sink<SinkItem = Bytes, SinkError = std::io::Error> + Send>;

struct FrameStreamReader {
    response_sink: Lock<Option<FrameStreamSink>>,
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
            response_sink: Lock::new(Some(response_sink)),
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
                emit!(UnixSocketEventReceived {
                    byte_size: frame.len()
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
        let mut frame = Bytes::new();
        frame.extend(&header.to_u32().to_be_bytes());
        if let Some(s) = content_type {
            frame.extend(&ControlField::ContentType.to_u32().to_be_bytes()); //field type: ContentType
            frame.extend(&(s.len() as u32).to_be_bytes()); //length of type
            frame.extend(s.as_bytes());
        }
        frame
    }

    fn send_control_frame(&mut self, frame: Bytes) {
        let empty_frame = Bytes::from(&b""[..]); //send empty frame to say we are control frame
        let stream =
            futures01::stream::iter_ok::<_, std::io::Error>(vec![empty_frame, frame].into_iter());

        self.response_sink.poll_lock().map(|mut sink| {
            //send and send_all consume the sink so need to use Option so we can .take()
            //get the sink back as first element of tuple
            let handler = sink
                .take()
                .unwrap()
                .send_all(stream)
                .and_then(move |(same_sink, _stream)| {
                    *sink = Some(same_sink);
                    futures01::done(Ok(()))
                })
                .map_err(|_e| ());
            tokio01::spawn(handler);
        });
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
}

/**
 * Based off of the build_unix_source function.
 * Functions similarly, but uses the FrameStreamReader to deal with
 * framestream control packets, and responds appropriately.
 **/
pub fn build_framestream_unix_source(
    frame_handler: impl FrameHandler + Send + Sync + Clone + 'static,
    shutdown: ShutdownSignal,
    out: Sender<Event>,
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

    Box::new(future::lazy(move || {
        let listener = UnixListener::bind(&path).expect("failed to bind to listener socket");
        let parsing_task_counter = Arc::new(AtomicI32::new(0));

        info!(message = "listening.", ?path, r#type = "unix");

        listener
            .incoming()
            .take_until(shutdown.clone())
            .map_err(|e| error!("failed to accept socket; error = {:?}", e))
            .for_each(move |socket| {
                let peer_addr = socket.peer_addr().ok();
                let content_type = frame_handler.content_type();
                let listen_path = path.clone();
                let event_sink = out.clone();
                let task_counter = parsing_task_counter.clone();

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
                    .take_until(shutdown.clone())
                    .filter_map(move |frame| fs_reader.handle_frame(Bytes::from(frame)));
                if !frame_handler.multithreaded() {
                    let events = frames
                        .filter_map(move |f| {
                            frame_handler_copy.handle_event(received_from.clone(), f)
                        })
                        .map_err(move |error| {
                            emit!(UnixSocketError {
                                error,
                                path: &listen_path,
                            });
                        });

                    let handler = events
                        .forward(event_sink)
                        .map(|_| info!("finished sending"));
                    tokio01::spawn(handler.instrument(span))
                } else {
                    let handler = frames
                        .for_each(move |f| {
                            let max_frame_handling_tasks =
                                frame_handler_copy.max_frame_handling_tasks();
                            let f_handler = frame_handler_copy.clone();
                            let received_from_copy = received_from.clone();
                            let mut event_sink_copy = event_sink.clone();
                            let task_counter_copy = task_counter.clone();

                            let event_handler = move || {
                                if let Some(e) = f_handler.handle_event(received_from_copy, f) {
                                    if let Err(e) = event_sink_copy.get_mut().try_send(e) {
                                        error!("{:#?}", e);
                                    }
                                };
                            };

                            spawn_event_handling_tasks(
                                event_handler,
                                task_counter_copy,
                                max_frame_handling_tasks,
                            );
                            Ok(())
                        })
                        .map_err(move |error| {
                            emit!(UnixSocketError {
                                error,
                                path: &listen_path,
                            });
                        })
                        .map(|_| info!("finished sending"));
                    tokio01::spawn(handler)
                }
            })
    }))
}

fn spawn_event_handling_tasks<F>(
    event_handler: F,
    task_counter: Arc<AtomicI32>,
    max_frame_handling_tasks: i32,
) -> Spawn
where
    F: Send + Sync + Clone + FnOnce() -> () + 'static,
{
    wait_for_task_quota(&task_counter, max_frame_handling_tasks);

    tokio01::spawn(future::lazy(move || {
        event_handler();
        task_counter.fetch_sub(1, Ordering::Relaxed);
        Ok(())
    }))
}

fn wait_for_task_quota(task_counter: &Arc<AtomicI32>, max_tasks: i32) {
    let waiting_time = Duration::from_millis(10);
    while max_tasks > 0 && max_tasks < task_counter.load(Ordering::Relaxed) {
        thread::sleep(waiting_time);
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
        runtime,
        shutdown::ShutdownSignal,
    };
    use bytes::{Bytes, BytesMut};
    use futures01::{sync::mpsc, Future, IntoFuture, Sink, Stream};
    #[cfg(unix)]
    use std::{
        path::PathBuf,
        sync::{
            atomic::{AtomicI32, Ordering},
            Arc, Mutex,
        },
        thread,
        time::Duration,
    };
    use tokio01::{
        self,
        codec::{length_delimited, Framed},
    };
    use tokio_uds::UnixStream;

    #[derive(Clone)]
    struct MockFrameHandler {
        content_type: String,
        max_length: usize,
        host_key: String,
        socket_path: PathBuf,
        multithreaded: bool,
        max_frame_handling_tasks: i32,
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
    }

    fn init_framstream_unix(
        frame_handler: MockFrameHandler,
        sender: mpsc::Sender<event::Event>,
    ) -> (PathBuf, runtime::Runtime) {
        let socket_path = frame_handler.socket_path();
        let server = build_framestream_unix_source(frame_handler, ShutdownSignal::noop(), sender);

        let mut rt = runtime::Runtime::new().unwrap();
        rt.spawn(server);

        // Wait for server to accept traffic
        while let Err(_) = std::os::unix::net::UnixStream::connect(&socket_path) {}

        (socket_path, rt)
    }

    fn make_unix_stream(
        path: PathBuf,
    ) -> Framed<UnixStream, length_delimited::LengthDelimitedCodec> {
        let socket = UnixStream::connect(&path)
            .map_err(|e| panic!("{:}", e))
            .wait()
            .unwrap();
        Framed::new(socket, length_delimited::Builder::new().new_codec())
    }

    fn send_data_frames<S: Sink<SinkItem = Bytes, SinkError = std::io::Error>>(
        sock_sink: S,
        frames: Vec<Bytes>,
    ) -> S {
        let stream = futures01::stream::iter_ok::<_, std::io::Error>(frames.into_iter());
        //send and send_all consume the sink
        sock_sink.send_all(stream).into_future().wait().unwrap().0
    }

    fn send_control_frame<S: Sink<SinkItem = Bytes, SinkError = std::io::Error>>(
        sock_sink: S,
        frame: Bytes,
    ) -> S {
        send_data_frames(sock_sink, vec![Bytes::new(), frame]) //send empty frame to say we are control frame
    }

    fn create_control_frame(header: ControlHeader) -> Bytes {
        Bytes::from(&header.to_u32().to_be_bytes()[..])
    }

    fn create_control_frame_with_content(
        header: ControlHeader,
        content_types: Vec<Bytes>,
    ) -> Bytes {
        let mut frame = create_control_frame(header);
        for content_type in content_types {
            frame.extend(&ControlField::ContentType.to_u32().to_be_bytes());
            frame.extend(&(content_type.len() as u32).to_be_bytes());
            frame.extend(content_type.clone());
        }
        frame
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

    #[test]
    fn normal_framestream() {
        let (tx, rx) = mpsc::channel(2);
        let (path, mut rt) =
            init_framstream_unix(MockFrameHandler::new("test_content".to_string()), tx);
        let (mut sock_sink, mut sock_stream) = make_unix_stream(path).split();

        //1 - send READY frame (with content_type)
        let content_type = Bytes::from(&b"test_content"[..]);
        let ready_msg =
            create_control_frame_with_content(ControlHeader::Ready, vec![content_type.clone()]);
        sock_sink = send_control_frame(sock_sink, ready_msg);

        //2 - wait for ACCEPT frame
        let (mut frame_vec, tmp_stream) =
            rt.block_on(collect_n_stream(sock_stream, 2)).ok().unwrap();
        sock_stream = tmp_stream;
        //take second element, because first will be empty (signifying control frame)
        assert_eq!(frame_vec[0].len(), 0);
        assert_accept_frame(&mut frame_vec[1], content_type);

        //3 - send START frame
        sock_sink = send_control_frame(sock_sink, create_control_frame(ControlHeader::Start));

        //4 - send data
        sock_sink = send_data_frames(
            sock_sink,
            vec![Bytes::from(&"hello"[..]), Bytes::from(&"world"[..])],
        );
        let events = rt.block_on(collect_n(rx, 2)).ok().unwrap();

        //5 - send STOP frame
        let _ = send_control_frame(sock_sink, create_control_frame(ControlHeader::Stop));

        assert_eq!(
            events[0].as_log()[&event::log_schema().message_key()],
            "hello".into(),
        );
        assert_eq!(
            events[1].as_log()[&event::log_schema().message_key()],
            "world".into(),
        );

        std::mem::drop(sock_stream); //explicitly drop the stream so we don't get warnings about not using it
    }

    #[test]
    fn multiple_content_types() {
        let (tx, _) = mpsc::channel(2);
        let (path, mut rt) =
            init_framstream_unix(MockFrameHandler::new("test_content".to_string()), tx);
        let (sock_sink, mut sock_stream) = make_unix_stream(path).split();

        //1 - send READY frame (with content_type)
        let content_type = Bytes::from(&b"test_content"[..]);
        let ready_msg = create_control_frame_with_content(
            ControlHeader::Ready,
            vec![Bytes::from(&b"test_content2"[..]), content_type.clone()],
        ); //can have multiple content types
        let _ = send_control_frame(sock_sink, ready_msg);

        //2 - wait for ACCEPT frame
        let (mut frame_vec, tmp_stream) =
            rt.block_on(collect_n_stream(sock_stream, 2)).ok().unwrap();
        sock_stream = tmp_stream;

        //take second element, because first will be empty (signifying control frame)
        assert_eq!(frame_vec[0].len(), 0);
        assert_accept_frame(&mut frame_vec[1], content_type);

        std::mem::drop(sock_stream); //explicitly drop the stream so we don't get warnings about not using it
    }

    #[test]
    fn wrong_content_type() {
        let (tx, _) = mpsc::channel(2);
        let (path, mut rt) =
            init_framstream_unix(MockFrameHandler::new("test_content".to_string()), tx);
        let (mut sock_sink, mut sock_stream) = make_unix_stream(path).split();

        //1 - send READY frame (with WRONG content_type)
        let ready_msg = create_control_frame_with_content(
            ControlHeader::Ready,
            vec![Bytes::from(&b"test_content2"[..])],
        ); //can have multiple content types
        sock_sink = send_control_frame(sock_sink, ready_msg);

        //2 - send READY frame (with RIGHT content_type)
        let content_type = Bytes::from(&b"test_content"[..]);
        let ready_msg =
            create_control_frame_with_content(ControlHeader::Ready, vec![content_type.clone()]);
        let _ = send_control_frame(sock_sink, ready_msg);

        //3 - wait for ACCEPT frame
        let (mut frame_vec, tmp_stream) =
            rt.block_on(collect_n_stream(sock_stream, 2)).ok().unwrap();
        sock_stream = tmp_stream;

        //take second element, because first will be empty (signifying control frame)
        assert_eq!(frame_vec[0].len(), 0);
        assert_accept_frame(&mut frame_vec[1], content_type);

        std::mem::drop(sock_stream); //explicitly drop the stream so we don't get warnings about not using it
    }

    #[test]
    fn data_too_soon() {
        let (tx, rx) = mpsc::channel(2);
        let (path, mut rt) =
            init_framstream_unix(MockFrameHandler::new("test_content".to_string()), tx);
        let (mut sock_sink, mut sock_stream) = make_unix_stream(path).split();

        //1 - send data frame (too soon!)
        sock_sink = send_data_frames(
            sock_sink,
            vec![Bytes::from(&"bad"[..]), Bytes::from(&"data"[..])],
        );

        //2 - send READY frame (with content_type)
        let content_type = Bytes::from(&b"test_content"[..]);
        let ready_msg =
            create_control_frame_with_content(ControlHeader::Ready, vec![content_type.clone()]);
        sock_sink = send_control_frame(sock_sink, ready_msg);

        //3 - wait for ACCEPT frame
        let (mut frame_vec, tmp_stream) =
            rt.block_on(collect_n_stream(sock_stream, 2)).ok().unwrap();
        sock_stream = tmp_stream;

        //take second element, because first will be empty (signifying control frame)
        assert_eq!(frame_vec[0].len(), 0);
        assert_accept_frame(&mut frame_vec[1], content_type);

        //4 - send START frame
        sock_sink = send_control_frame(sock_sink, create_control_frame(ControlHeader::Start));

        //5 - send data (will go through)
        sock_sink = send_data_frames(
            sock_sink,
            vec![Bytes::from(&"hello"[..]), Bytes::from(&"world"[..])],
        );
        let events = rt.block_on(collect_n(rx, 2)).ok().unwrap();

        //6 - send STOP frame
        let _ = send_control_frame(sock_sink, create_control_frame(ControlHeader::Stop));

        assert_eq!(
            events[0].as_log()[&event::log_schema().message_key()],
            "hello".into(),
        );
        assert_eq!(
            events[1].as_log()[&event::log_schema().message_key()],
            "world".into(),
        );

        std::mem::drop(sock_stream); //explicitly drop the stream so we don't get warnings about not using it
    }

    #[test]
    fn unidirectional_framestream() {
        let (tx, rx) = mpsc::channel(2);
        let (path, mut rt) =
            init_framstream_unix(MockFrameHandler::new("test_content".to_string()), tx);
        let (mut sock_sink, _) = make_unix_stream(path).split();

        //1 - send START frame (with content_type)
        let content_type = Bytes::from(&b"test_content"[..]);
        let start_msg = create_control_frame_with_content(ControlHeader::Start, vec![content_type]);
        sock_sink = send_control_frame(sock_sink, start_msg);

        //4 - send data
        sock_sink = send_data_frames(
            sock_sink,
            vec![Bytes::from(&"hello"[..]), Bytes::from(&"world"[..])],
        );
        let events = rt.block_on(collect_n(rx, 2)).ok().unwrap();

        //5 - send STOP frame
        let _ = send_control_frame(sock_sink, create_control_frame(ControlHeader::Stop));

        assert_eq!(
            events[0].as_log()[&event::log_schema().message_key()],
            "hello".into(),
        );
        assert_eq!(
            events[1].as_log()[&event::log_schema().message_key()],
            "world".into(),
        );

        // std::mem::drop(sock_stream); //explicitly drop the stream so we don't get warnings about not using it
    }

    #[test]
    fn test_spawn_event_handling_tasks() {
        let max_frame_handling_tasks = 100;
        let task_counter = Arc::new(AtomicI32::new(0));
        let task_counter_copy = task_counter.clone();
        let max_task_counter_value = Arc::new(Mutex::new(0));
        let max_task_counter_value_copy = max_task_counter_value.clone();
        tokio01::run(futures01::lazy(move || {
            let mut handles = vec![];
            let task_counter_copy_2 = task_counter_copy.clone();
            let mock_event_handler = move || {
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
                    task_counter_copy.clone(),
                    max_frame_handling_tasks,
                ));
            }
            Ok(())
        }));

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

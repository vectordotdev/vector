use crate::{
    event::Event,
    internal_events::{UnixSocketError, UnixSocketEventReceived},
    shutdown::ShutdownSignal,
    sources::Source,
    stream::StreamExt,
};
use bytes::Bytes;
use futures01::{future, sync::mpsc, Future, Sink, Stream};
use std::convert::TryInto;
#[cfg(unix)]
use std::path::PathBuf;
use std::fs;
use tokio01::{
    self,
    codec::{length_delimited, Framed},
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
    fn to_u32(self) -> u32 {
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
        if frame.len() == 0 {
            //frame length of zero means the next frame is a control frame
            self.state.expect_control_frame = true;
            None
        } else {
            if self.state.expect_control_frame {
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
                if frame.len() == 0 {
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
                if frame.len() != 0 {
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
        if frame.len() == 0 {
            error!("No fields in control frame");
            return Err(());
        }

        let mut content_types = vec![];
        while frame.len() > 0 {
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

/**
 * Based off of the build_unix_source function.
 * Functions similarly, but uses the FrameStreamReader to deal with
 * framestream control packets, and responds appropriately.
 **/
pub fn build_framestream_unix_source(
    path: PathBuf,
    max_length: usize,
    host_key: String,
    content_type: String,
    shutdown: ShutdownSignal,
    out: mpsc::Sender<Event>,
    build_event: impl Fn(&str, Option<Bytes>, Bytes) -> Option<Event>
        + std::marker::Send
        + std::marker::Sync
        + std::clone::Clone
        + 'static,
) -> Source {
    let out = out.sink_map_err(|e| error!("error sending line: {:?}", e));

    //check if the path already exists (and try to delete it)
    match fs::metadata(&path) {
        Ok(_) => {
            //exists, so try to delete it
            info!(message = "deleting file", ?path);
            fs::remove_file(&path).expect("failed to delete existing socket");
        },
        Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => {}, //doesn't exist, do nothing
        Err(e) => error!("failed to bind to listener socket; error = {:?}", e),
    };

    Box::new(future::lazy(move || {
        let listener = UnixListener::bind(&path).expect("failed to bind to listener socket");

        info!(message = "listening.", ?path, r#type = "unix");

        listener
            .incoming()
            .take_until(shutdown.clone())
            .map_err(|e| error!("failed to accept socket; error = {:?}", e))
            .for_each(move |socket| {
                let out = out.clone();
                let peer_addr = socket.peer_addr().ok();
                let host_key = host_key.clone();
                let content_type = content_type.clone();
                let listen_path = path.clone();
                let build_event = build_event.clone();

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
                        .max_frame_length(max_length)
                        .new_codec(),
                )
                .split();
                let mut fs_reader = FrameStreamReader::new(Box::new(sock_sink), content_type);
                let events = sock_stream
                    .take_until(shutdown.clone())
                    .filter_map(
                        move |frame| match fs_reader.handle_frame(Bytes::from(frame)) {
                            Some(f) => build_event(&host_key, received_from.clone(), f),
                            None => None,
                        },
                    )
                    .map_err(move |error| {
                        emit!(UnixSocketError {
                            error,
                            path: &listen_path,
                        });
                    });

                let handler = events.forward(out).map(|_| info!("finished sending"));
                tokio01::spawn(handler.instrument(span))
            })
    }))
}

#[cfg(test)]
mod test {
    use super::{build_framestream_unix_source, ControlField, ControlHeader};
    use crate::test_util::{collect_n, collect_n_stream};
    use crate::{
        event::{self, Event},
        runtime,
        shutdown::ShutdownSignal,
    };
    use bytes::{Bytes, BytesMut};
    use futures01::{sync::mpsc, Future, IntoFuture, Sink, Stream};
    #[cfg(unix)]
    use std::path::PathBuf;
    use tokio01::{
        self,
        codec::{length_delimited, Framed},
    };
    use tokio_uds::UnixStream;

    fn build_event_bytes(
        host_key: &str,
        received_from: Option<Bytes>,
        frame: Bytes,
    ) -> Option<Event> {
        let mut event = Event::from(frame);
        event
            .as_mut_log()
            .insert(event::log_schema().source_type_key(), "framestream");
        if let Some(host) = received_from {
            event.as_mut_log().insert(host_key, host);
        }
        Some(event)
    }

    fn init_framstream_unix(
        content_type: String,
        sender: mpsc::Sender<event::Event>,
    ) -> (PathBuf, runtime::Runtime) {
        let in_path = tempfile::tempdir().unwrap().into_path().join("unix_test");

        let server = build_framestream_unix_source(
            in_path.clone(),
            bytesize::kib(100u64) as usize,
            "test_framestream".to_string(),
            content_type,
            ShutdownSignal::noop(),
            sender,
            build_event_bytes,
        );

        let mut rt = runtime::Runtime::new().unwrap();
        rt.spawn(server);

        // Wait for server to accept traffic
        while let Err(_) = std::os::unix::net::UnixStream::connect(&in_path) {}

        (in_path, rt)
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
        let (path, mut rt) = init_framstream_unix("test_content".to_string(), tx);
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
        let (path, mut rt) = init_framstream_unix("test_content".to_string(), tx);
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
        let (path, mut rt) = init_framstream_unix("test_content".to_string(), tx);
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
        let (path, mut rt) = init_framstream_unix("test_content".to_string(), tx);
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
        let (path, mut rt) = init_framstream_unix("test_content".to_string(), tx);
        let (mut sock_sink, _) = make_unix_stream(path).split();

        //1 - send START frame (with content_type)
        let content_type = Bytes::from(&b"test_content"[..]);
        let start_msg =
            create_control_frame_with_content(ControlHeader::Start, vec![content_type.clone()]);
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
}

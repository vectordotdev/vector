use crate::{
    event::Event,
    internal_events::{UnixSocketError, UnixSocketEventReceived},
    shutdown::ShutdownSignal,
    sources::Source,
    stream::StreamExt,
};
use bytes::Bytes;
use futures01::{future, sync::mpsc, Future, IntoFuture, Sink, Stream};
#[cfg(unix)]
use std::path::PathBuf;
use std::convert::TryInto;
use tokio01::{
    self,
    codec::{Framed, length_delimited},
};
use tokio_uds::UnixListener;
use tracing::field;
use tracing_futures::Instrument;

struct FrameStreamReader<S:Sink<SinkItem = Bytes, SinkError = std::io::Error>> {
    response_sink: Option<S>,
    expected_content_type: String,
    state: FrameStreamState,
}

struct FrameStreamState {
    expect_control_frame: bool,
    control_state: ControlState,
}
impl FrameStreamState {
    fn new() -> Self {
        FrameStreamState {
            expect_control_frame: false,
            //first control frame should be READY
            control_state: ControlState::ReadingControlReady,
        }
    }
}

enum ControlState {
    ReadingControlReady,
    ReadingControlStart,
    ReadingData,
    Stopped,
}

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
            },
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
            },
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

impl<S:Sink<SinkItem = Bytes, SinkError = std::io::Error>> FrameStreamReader<S> {
    pub fn new(response_sink: S, expected_content_type: String) -> Self {
        FrameStreamReader {
            response_sink: Some(response_sink),
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
                emit!(UnixSocketEventReceived { byte_size: frame.len() });
                Some(frame) //return data frame
            }
        }
    }

    fn handle_control_frame(&mut self, mut frame: Bytes) -> Result<(), ()> {
        let header = ControlHeader::from_u32(advance_u32(&mut frame)?)?;

        //match current state to received header
        match self.state.control_state {
            ControlState::ReadingControlReady => {
                match header {
                    ControlHeader::Ready => {
                        let content_type = self.process_content_type(&mut frame)?;
                        self.send_control_frame(Self::make_frame(ControlHeader::Accept, Some(content_type)));
                        self.state.control_state = ControlState::ReadingControlStart; //waiting for a START control frame
                    },
                    _ => error!("Got wrong control frame, expected READY"),
                }
            },
            ControlState::ReadingControlStart => {
                match header {
                    ControlHeader::Start => {
                        self.state.control_state = ControlState::ReadingData; //just change state
                    },
                    _ => error!("Got wrong control frame, expected START"),
                }
            },
            ControlState::ReadingData => {
                match header {
                    ControlHeader::Stop => {
                        self.send_control_frame(Self::make_frame(ControlHeader::Finish, None)); //send FINISH frame
                        self.state.control_state = ControlState::Stopped; //stream is now done
                    },
                    _ => error!("Got wrong control frame, expected STOP"),
                }
            },
            ControlState::Stopped => error!("Unexpected control frame, current state is STOPPED"),
        };
        Ok(())
    }

    fn process_content_type(&self, frame: &mut Bytes) -> Result<String, ()> {
        while frame.len() > 0 {
            //4 bytes of ControlField
            let field_val = advance_u32(frame)?;
            let field_type = ControlField::from_u32(field_val)?;
            match field_type {
                ControlField::ContentType => {
                    //4 bytes giving length of content type
                    let field_len = advance_u32(frame)? as usize;
                    let content_type = std::str::from_utf8(&frame[..field_len]).unwrap();
                    if content_type == self.expected_content_type {
                        return Ok(content_type.to_string());
                    }
                }
            }   
        }

        error!("Content types did not match up.");
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
        let stream = futures01::stream::iter_ok::<_, std::io::Error>(vec![empty_frame, frame].into_iter());

        //send and send_all consume the sink
        let mut tmp_sink = self.response_sink.take().unwrap();
        //get the sink back as first element of tuple
        tmp_sink = tmp_sink.send_all(stream).into_future().wait().unwrap().0; //TODO: better way than .wait().unwrap() (?)
        self.response_sink = Some(tmp_sink);
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
                
                let (sock_sink, sock_stream) = Framed::new(socket, length_delimited::Builder::new().max_frame_length(max_length).new_codec()).split();
                let mut fs_reader = FrameStreamReader::new(sock_sink, content_type);
                let events = sock_stream.take_until(shutdown.clone())
                    .filter_map(move |frame| {
                        match fs_reader.handle_frame(Bytes::from(frame)) {
                            Some(f) => build_event(&host_key, received_from.clone(), f),
                            None => None,
                        }
                    })
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

/*
use super::{SocketListenAddr, TcpSource};
use std::net::SocketAddr;
tokio01::net::{UdpFramed, UdpSocket}
*/

//TODO: udp

// pub fn udp(
//     addr: SocketAddr,
//     _max_length: usize,
//     host_key: String,
//     shutdown: ShutdownSignal,
//     out: mpsc::Sender<Event>,
// ) -> super::Source {
//     let out = out.sink_map_err(|e| error!("error sending line: {:?}", e));

//     Box::new(
//         future::lazy(move || {
//             let socket = UdpSocket::bind(&addr).expect("failed to bind to udp listener socket");

//             info!(
//                 message = "listening.",
//                 addr = &field::display(addr),
//                 r#type = "udp"
//             );

//             future::ok(socket)
//         })
//         .and_then(move |socket| {
//             let host_key = host_key.clone();

//             let (sock_sink, sock_stream) = UdpFramed::new(socket, length_delimited::Builder::new().max_frame_length(max_length).new_codec()).split();
//             let bytes_sink = sock_sink.with(|frame| Ok((frame, addr)).into_future()); //add the SocketAddr to the item (UdpFramed::Item is (Bytes, SocketAddr))

//             let fs_reader = FrameStreamReader::new(bytes_sink, host_key, Some(addr.to_string().into()));

//             let lines_in = sock_stream
//                 .take_until(shutdown)
//                 .filter_map(move |(frame, addr)| fs_reader.handle_frame(Bytes::from(frame)))
//                 .map_err(|error| panic!("TODO") emit!(SyslogUdpReadError { error })); //needs changing

//             lines_in.forward(out).map(|_| info!("finished sending"))
//         }),
//     )
// }


//TODO: TCP


#[cfg(test)]
mod test {
    use futures01::{sync::mpsc, Future, IntoFuture, Sink, Stream};
    #[cfg(unix)]
    use std::path::PathBuf;
    use tokio01::{
        self,
        codec::{Framed, length_delimited},
    };
    use tokio_uds::UnixStream;
    use bytes::{Bytes, BytesMut};
    use super::{build_framestream_unix_source, ControlField, ControlHeader};
    use crate::{
        event::{self, Event},
        shutdown::ShutdownSignal,
        runtime,
    };
    use crate::test_util::{collect_n, collect_n_stream};

    fn build_event_bytes(host_key: &str, received_from: Option<Bytes>, frame: Bytes) -> Option<Event> {
        let mut event = Event::from(frame);
        event
            .as_mut_log()
            .insert(event::log_schema().source_type_key(), "framestream");
        if let Some(host) = received_from {
            event.as_mut_log().insert(host_key, host);
        }
        Some(event)
    }

    fn init_framstream_unix(content_type: String, sender: mpsc::Sender<event::Event>) -> (PathBuf, runtime::Runtime) {
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

    fn make_unix_stream(path: PathBuf) -> Framed<UnixStream, length_delimited::LengthDelimitedCodec> {
        let socket = UnixStream::connect(&path)
            .map_err(|e| panic!("{:}", e))
            .wait()
            .unwrap();
        Framed::new(socket, length_delimited::Builder::new().new_codec())
    }

    fn send_data_frames<S:Sink<SinkItem = Bytes, SinkError = std::io::Error>>(sock_sink: S, frames: Vec<Bytes>) -> S {
        let stream = futures01::stream::iter_ok::<_, std::io::Error>(frames.into_iter());
        //send and send_all consume the sink
        sock_sink.send_all(stream).into_future().wait().unwrap().0
    }

    fn send_control_frame<S:Sink<SinkItem = Bytes, SinkError = std::io::Error>>(sock_sink: S, frame: Bytes) -> S {
        send_data_frames(sock_sink, vec![Bytes::new(), frame]) //send empty frame to say we are control frame
    }

    fn create_ready_frame(content_types: Vec<Bytes>) -> Bytes {
        let mut ready_msg = Bytes::new();
        ready_msg.extend(&ControlHeader::Ready.to_u32().to_be_bytes());
        for content_type in content_types {
            ready_msg.extend(&ControlField::ContentType.to_u32().to_be_bytes());
            ready_msg.extend(&(content_type.len() as u32).to_be_bytes());
            ready_msg.extend(content_type.clone());
        }
        ready_msg
    }

    fn assert_accept_frame(frame: &mut BytesMut, expected_content_type: Bytes) {
        //frame should start with 4 bytes saying ACCEPT

        assert_eq!(
            &frame[..4],
            &ControlHeader::Accept.to_u32().to_be_bytes(),
        );
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
        let (path, mut rt) = init_framstream_unix(
            "test_content".to_string(),
            tx,
        );
        let (mut sock_sink, mut sock_stream) = make_unix_stream(path).split();

        //1 - send READY frame (with content_type)
        let content_type = Bytes::from(&b"test_content"[..]);
        let ready_msg = create_ready_frame(vec![content_type.clone()]);
        sock_sink = send_control_frame(sock_sink, ready_msg);
        
        //2 - wait for ACCEPT frame
        let (mut frame_vec, tmp_stream) = rt.block_on(collect_n_stream(sock_stream, 2)).ok().unwrap();
        sock_stream = tmp_stream;
        //take second element, because first will be empty (signifying control frame)
        assert_accept_frame(&mut frame_vec[1], content_type);

        //3 - send START frame
        sock_sink = send_control_frame(sock_sink, Bytes::from(&ControlHeader::Start.to_u32().to_be_bytes()[..]));

        //4 - send data
        sock_sink = send_data_frames(sock_sink, vec![Bytes::from(&"hello"[..]), Bytes::from(&"world"[..])]);
        let events = rt.block_on(collect_n(rx, 2)).ok().unwrap();

        //5 - send STOP frame
        let _ = send_control_frame(sock_sink, Bytes::from(&ControlHeader::Stop.to_u32().to_be_bytes()[..]));

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
        //TODO: more framestream tests
    }

    #[test]
    fn wrong_content_type() {
        //TODO: more framestream tests
    }

    #[test]
    fn data_too_soon() {
        //TODO: more framestream tests
    }
}
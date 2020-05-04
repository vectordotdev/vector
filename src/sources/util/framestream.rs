use crate::{
    event::Event,
    internal_events::{UnixSocketError, UnixSocketEventReceived},
    shutdown::ShutdownSignal,
    sources::Source,
    stream::StreamExt,
};
use bytes::Bytes;
use futures01::{future, sync::mpsc, Future, Sink, Stream};
#[cfg(unix)]
use std::path::PathBuf;
use tokio01::{
    self,
    codec::{Framed, LengthDelimitedCodec},
};
use tokio_uds::UnixListener;
use tracing::field;
use tracing_futures::Instrument;

struct FrameStreamReader<S:Sink<SinkItem = Bytes, SinkError = std::io::Error>> {
    response_sink: S,
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
    fn from_u32(val: u32) -> Self {
        match val {
            0x01 => ControlHeader::Accept,
            0x02 => ControlHeader::Start,
            0x03 => ControlHeader::Stop,
            0x04 => ControlHeader::Ready,
            0x05 => ControlHeader::Finish,
            _ => panic!("Don't know this header") //TODO: error
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
    fn from_u32(val: u32) -> Self {
        match val {
            0x01 => ControlField::ContentType,
            _ => panic!("Don't know this field type") //TODO: error
        }
    }
    fn to_u32(self) -> u32 {
        match self {
            ControlField::ContentType => 0x01,
        }
    }
}

fn advance_u32(b: &mut Bytes) -> u32 {
    if b.len() < 4 {
        panic!("Not long enough") //TODO: error
    }
    let a = b.split_to(4);
    let mut copy_header:[u8; 4] = [0,0,0,0]; //TODO: better than this
    copy_header.copy_from_slice(&a[..]);
    u32::from_be_bytes(copy_header)
}

impl<S:Sink<SinkItem = Bytes, SinkError = std::io::Error>> FrameStreamReader<S> {
    pub fn new(response_sink: S, expected_content_type: String) -> Self {
        FrameStreamReader {
            response_sink,
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
                self.handle_control_frame(frame);
                None
            } else { 
                emit!(UnixSocketEventReceived { byte_size: frame.len() });
                Some(frame) //return data frame
            }
        }
    }

    fn handle_control_frame(&mut self, frame: Bytes) {
        let header = ControlHeader::from_u32(advance_u32(&mut frame));

        //match current state to received header
        match self.state.control_state {
            ControlState::ReadingControlReady => {
                match header {
                    ControlHeader::Ready => {
                        if let Some(content_type) = self.process_content_type(&mut frame) {
                            self.send_control_frame(Self::make_frame(ControlHeader::Accept, Some(content_type)));
                            self.state.control_state = ControlState::ReadingControlStart; //waiting for a START control frame
                        } else {
                            error!("Content types did not match up.")
                        }
                    },
                    _ => error!("Got wrong control frame, expected READY"), //TODO: error
                }
            },
            ControlState::ReadingControlStart => {
                match header {
                    ControlHeader::Start => {
                        self.state.control_state = ControlState::ReadingData; //just change state
                    },
                    _ => error!("Got wrong control frame, expected START"), //TODO: error
                }
            },
            ControlState::ReadingData => {
                match header {
                    ControlHeader::Stop => {
                        self.send_control_frame(Self::make_frame(ControlHeader::Finish, None)); //send FINISH frame
                        self.state.control_state = ControlState::Stopped; //stream is now done
                    },
                    _ => error!("Got wrong control frame, expected STOP"), //TODO: error
                }
            },
            ControlState::Stopped => error!("Unexpected control frame, current state is STOPPED"), //TODO: error
        }
    }

    fn process_content_type(&self, frame: &mut Bytes) -> Option<String> {
        while frame.len() > 0 {
            //4 bytes of ControlField
            let field_val = advance_u32(frame);
            let field_type = ControlField::from_u32(field_val);
            match field_type {
                ControlField::ContentType => {
                    //4 bytes giving length of content type
                    let field_len = advance_u32(frame) as usize;
                    let content_type = std::str::from_utf8(&frame[..field_len]).unwrap();
                    if content_type == self.expected_content_type {
                        return Some(self.expected_content_type);
                    }
                }
            }   
        }

        None
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
        //TODO: use send_all instead of 2 send calls
        //TODO: better way that .wait().unwrap() (?)
        self.response_sink = self.response_sink.send(empty_frame).wait().unwrap();
        self.response_sink = self.response_sink.send(frame).wait().unwrap();
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
                
                let (sock_sink, sock_stream) = Framed::new(socket, LengthDelimitedCodec::new()).split();
                let fs_reader = FrameStreamReader::new(sock_sink, content_type);
                let events = sock_stream.take_until(shutdown.clone())
                    .filter_map(move |frame| {
                        match fs_reader.handle_frame(Bytes::from(frame)) {
                            Some(f) => build_event(&host_key, received_from, f),
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

//             let (sock_sink, sock_stream) = UdpFramed::new(socket, LengthDelimitedCodec::new()).split();
//             let bytes_sink = sock_sink.with(|frame| Ok((frame, addr)).into_future()); //add the SocketAddr to the item (UdpFramed::Item is (Bytes, SocketAddr))

//             let fs_reader = FrameStreamReader::new(bytes_sink, host_key, Some(addr.to_string().into()));

//             let lines_in = sock_stream
//                 .take_until(shutdown)
//                 .filter_map(move |(frame, addr)| fs_reader.handle_frame(Bytes::from(frame)))
//                 .map_err(|error| panic!("TODO") emit!(SyslogUdpReadError { error })); //TODO: error

//             lines_in.forward(out).map(|_| info!("finished sending"))
//         }),
//     )
// }


//TODO: TCP

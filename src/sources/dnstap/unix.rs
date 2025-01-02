use std::path::PathBuf;

use bytes::Bytes;
use vector_lib::configurable::configurable_component;
use vector_lib::lookup::OwnedValuePath;

use crate::sources::util::framestream::FrameHandler;
use crate::{
    event::Event,
    internal_events::{SocketEventsReceived, SocketMode},
    sources::util::framestream::UnixFrameHandler,
};

pub use super::schema::DnstapEventSchema;
use vector_lib::EstimatedJsonEncodedSizeOf;

/// Unix domain socket configuration for the `dnstap` source.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct UnixConfig {
    /// Absolute path to the socket file to read DNSTAP data from.
    ///
    /// The DNS server must be configured to send its DNSTAP data to this socket file. The socket file is created
    /// if it doesn't already exist when the source first starts.
    pub socket_path: PathBuf,

    /// Unix file mode bits to be applied to the unix socket file as its designated file permissions.
    ///
    /// Note: The file mode value can be specified in any numeric format supported by your configuration
    /// language, but it is most intuitive to use an octal number.
    pub socket_file_mode: Option<u32>,

    /// The size, in bytes, of the receive buffer used for the socket.
    ///
    /// This should not typically needed to be changed.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    pub socket_receive_buffer_size: Option<usize>,

    /// The size, in bytes, of the send buffer used for the socket.
    ///
    /// This should not typically needed to be changed.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    pub socket_send_buffer_size: Option<usize>,
}

impl UnixConfig {
    pub fn new(socket_path: PathBuf) -> Self {
        Self {
            socket_path,
            ..Self::default()
        }
    }
}

impl Default for UnixConfig {
    fn default() -> Self {
        Self {
            socket_path: PathBuf::from("/run/bind/dnstap.sock"),
            socket_file_mode: None,
            socket_receive_buffer_size: None,
            socket_send_buffer_size: None,
        }
    }
}

#[derive(Clone)]
pub struct DnstapFrameHandler<T: FrameHandler + Clone> {
    frame_handler: T,
    socket_path: PathBuf,
    socket_file_mode: Option<u32>,
    socket_receive_buffer_size: Option<usize>,
    socket_send_buffer_size: Option<usize>,
}

impl<T: FrameHandler + Clone> DnstapFrameHandler<T> {
    pub fn new(config: UnixConfig, frame_handler: T) -> Self {
        Self {
            frame_handler,
            socket_path: config.socket_path.clone(),
            socket_file_mode: config.socket_file_mode,
            socket_receive_buffer_size: config.socket_receive_buffer_size,
            socket_send_buffer_size: config.socket_send_buffer_size,
        }
    }
}

impl<T: FrameHandler + Clone> FrameHandler for DnstapFrameHandler<T> {
    fn content_type(&self) -> String {
        self.frame_handler.content_type()
    }

    fn max_frame_length(&self) -> usize {
        self.frame_handler.max_frame_length()
    }

    /**
     * Function to pass into util::framestream::build_framestream_unix_source
     * Takes a data frame from the unix socket and turns it into a Vector Event.
     **/
    fn handle_event(&self, received_from: Option<Bytes>, frame: Bytes) -> Option<Event> {
        self.frame_handler
            .handle_event(received_from, frame)
            .map(|event| {
                if let Event::Log(ref log_event) = event {
                    emit!(SocketEventsReceived {
                        mode: SocketMode::Unix,
                        byte_size: log_event.estimated_json_encoded_size_of(),
                        count: 1
                    })
                }
                event
            })
    }

    fn multithreaded(&self) -> bool {
        self.frame_handler.multithreaded()
    }

    fn max_frame_handling_tasks(&self) -> u32 {
        self.frame_handler.max_frame_handling_tasks()
    }

    fn host_key(&self) -> &Option<OwnedValuePath> {
        self.frame_handler.host_key()
    }

    fn source_type_key(&self) -> Option<&OwnedValuePath> {
        self.frame_handler.source_type_key()
    }

    fn timestamp_key(&self) -> Option<&OwnedValuePath> {
        self.frame_handler.timestamp_key()
    }
}

impl<T: FrameHandler + Clone> UnixFrameHandler for DnstapFrameHandler<T> {
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

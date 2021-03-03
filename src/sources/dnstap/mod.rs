use super::util::framestream::{build_framestream_unix_source, FrameHandler};
use crate::{
    config::{log_schema, DataType, GlobalOptions, SourceConfig, SourceDescription},
    event::Event,
    shutdown::ShutdownSignal,
    Pipeline, Result,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

mod parser;
use parser::DnstapParser;

mod schema;
use schema::DnstapEventSchema;

mod dns_message;
mod dns_message_parser;

#[derive(Deserialize, Serialize, Debug)]
pub struct DnstapConfig {
    #[serde(default = "default_max_frame_length")]
    pub max_frame_length: usize,
    pub host_key: Option<String>,
    pub socket_path: PathBuf,
    pub raw_data_only: Option<bool>,
    pub multithreaded: Option<bool>,
    pub max_frame_handling_tasks: Option<i32>,
    pub socket_file_mode: Option<u32>,
    pub socket_receive_buffer_size: Option<usize>,
    pub socket_send_buffer_size: Option<usize>,
}

fn default_max_frame_length() -> usize {
    bytesize::kib(100u64) as usize
}

impl DnstapConfig {
    pub fn new(socket_path: PathBuf) -> Self {
        Self {
            host_key: None,
            socket_path,
            ..Self::default()
        }
    }

    fn content_type(&self) -> String {
        "protobuf:dnstap.Dnstap".to_string() //content-type for framestream
    }
}

impl Default for DnstapConfig {
    fn default() -> Self {
        Self {
            host_key: Some("host".to_string()),
            max_frame_length: default_max_frame_length(),
            socket_path: PathBuf::from("/run/bind/dnstap.sock"),
            raw_data_only: None,
            multithreaded: None,
            max_frame_handling_tasks: None,
            socket_file_mode: None,
            socket_receive_buffer_size: None,
            socket_send_buffer_size: None,
        }
    }
}

inventory::submit! {
    SourceDescription::new::<DnstapConfig>("dnstap")
}

impl_generate_config_from_default!(DnstapConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "dnstap")]
impl SourceConfig for DnstapConfig {
    async fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> Result<super::Source> {
        let host_key = self
            .host_key
            .clone()
            .unwrap_or_else(|| log_schema().host_key().to_string());

        let frame_handler = DnstapFrameHandler::new(
            self.max_frame_length,
            host_key,
            self.socket_path.clone(),
            self.content_type(),
            if let Some(v) = self.raw_data_only {
                v
            } else {
                false
            },
            if let Some(v) = self.multithreaded {
                v
            } else {
                false
            },
            if let Some(v) = self.max_frame_handling_tasks {
                v
            } else {
                1000
            },
            self.socket_file_mode,
            self.socket_receive_buffer_size,
            self.socket_send_buffer_size,
        );
        Ok(build_framestream_unix_source(frame_handler, shutdown, out))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "dnstap"
    }
}

#[derive(Clone)]
pub struct DnstapFrameHandler {
    max_frame_length: usize,
    host_key: String,
    socket_path: PathBuf,
    content_type: String,
    schema: DnstapEventSchema,
    raw_data_only: bool,
    multithreaded: bool,
    max_frame_handling_tasks: i32,
    socket_file_mode: Option<u32>,
    socket_receive_buffer_size: Option<usize>,
    socket_send_buffer_size: Option<usize>,
}

impl DnstapFrameHandler {
    pub fn new(
        max_frame_length: usize,
        host_key: String,
        socket_path: PathBuf,
        content_type: String,
        raw_data_only: bool,
        multithreaded: bool,
        max_frame_handling_tasks: i32,
        socket_file_mode: Option<u32>,
        socket_receive_buffer_size: Option<usize>,
        socket_send_buffer_size: Option<usize>,
    ) -> Self {
        Self {
            max_frame_length,
            host_key,
            socket_path,
            content_type,
            schema: DnstapEventSchema::new(),
            raw_data_only,
            multithreaded,
            max_frame_handling_tasks,
            socket_file_mode,
            socket_receive_buffer_size,
            socket_send_buffer_size,
        }
    }
}

impl FrameHandler for DnstapFrameHandler {
    fn content_type(&self) -> String {
        self.content_type.clone()
    }

    fn max_frame_length(&self) -> usize {
        self.max_frame_length
    }

    fn host_key(&self) -> String {
        self.host_key.clone()
    }

    /**
     * Function to pass into util::framestream::build_framestream_unix_source
     * Takes a data frame from the unix socket and turns it into a Vector Event.
     **/
    fn handle_event(&self, received_from: Option<Bytes>, frame: Bytes) -> Option<Event> {
        let mut event = Event::new_empty_log();

        let log_event = event.as_mut_log();

        if let Some(host) = received_from {
            log_event.insert(self.host_key(), host);
        }

        if self.raw_data_only {
            log_event.insert(
                &self.schema.dnstap_root_data_schema.raw_data,
                base64::encode(&frame),
            );
            Some(event)
        } else {
            match DnstapParser::new(&self.schema, log_event).parse_dnstap_data(frame) {
                Err(error) => {
                    error!("Dnstap protobuf decode error {:?}", error);
                    None
                }
                Ok(_) => Some(event),
            }
        }
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

    fn socket_receive_buffer_size(&self) -> Option<usize> {
        self.socket_receive_buffer_size
    }

    fn socket_send_buffer_size(&self) -> Option<usize> {
        self.socket_send_buffer_size
    }
}

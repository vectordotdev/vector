use std::path::PathBuf;

use bytes::Bytes;
use serde::{Deserialize, Serialize};

use super::util::framestream::{build_framestream_unix_source, FrameHandler};
use crate::{
    config::{log_schema, DataType, SourceConfig, SourceContext, SourceDescription},
    event::Event,
    internal_events::{DnstapEventReceived, DnstapParseDataError},
    Result,
};

pub mod parser;
pub use parser::{parse_dnstap_data, DnstapParser};

pub mod schema;
use dnsmsg_parser::{dns_message, dns_message_parser};
pub use schema::DnstapEventSchema;

#[derive(Deserialize, Serialize, Debug)]
pub struct DnstapConfig {
    #[serde(default = "default_max_frame_length")]
    pub max_frame_length: usize,
    pub host_key: Option<String>,
    pub socket_path: PathBuf,
    pub raw_data_only: Option<bool>,
    pub multithreaded: Option<bool>,
    pub max_frame_handling_tasks: Option<u32>,
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
    async fn build(&self, cx: SourceContext) -> Result<super::Source> {
        let host_key = self
            .host_key
            .clone()
            .unwrap_or_else(|| log_schema().host_key().to_string());

        let frame_handler = DnstapFrameHandler::new(
            self.max_frame_length,
            self.socket_path.clone(),
            self.content_type(),
            self.raw_data_only.unwrap_or(false),
            self.multithreaded.unwrap_or(false),
            self.max_frame_handling_tasks.unwrap_or(1000),
            self.socket_file_mode,
            self.socket_receive_buffer_size,
            self.socket_send_buffer_size,
            host_key,
            log_schema().timestamp_key(),
        );
        build_framestream_unix_source(frame_handler, cx.shutdown, cx.out)
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
    socket_path: PathBuf,
    content_type: String,
    schema: DnstapEventSchema,
    raw_data_only: bool,
    multithreaded: bool,
    max_frame_handling_tasks: u32,
    socket_file_mode: Option<u32>,
    socket_receive_buffer_size: Option<usize>,
    socket_send_buffer_size: Option<usize>,
    host_key: String,
    timestamp_key: String,
}

impl DnstapFrameHandler {
    pub fn new(
        max_frame_length: usize,
        socket_path: PathBuf,
        content_type: String,
        raw_data_only: bool,
        multithreaded: bool,
        max_frame_handling_tasks: u32,
        socket_file_mode: Option<u32>,
        socket_receive_buffer_size: Option<usize>,
        socket_send_buffer_size: Option<usize>,
        host_key: String,
        timestamp_key: &'static str,
    ) -> Self {
        let mut schema = DnstapEventSchema::new();
        schema
            .dnstap_root_data_schema_mut()
            .set_timestamp(timestamp_key);

        Self {
            max_frame_length,
            socket_path,
            content_type,
            schema,
            raw_data_only,
            multithreaded,
            max_frame_handling_tasks,
            socket_file_mode,
            socket_receive_buffer_size,
            socket_send_buffer_size,
            host_key,
            timestamp_key: timestamp_key.to_string(),
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

    /**
     * Function to pass into util::framestream::build_framestream_unix_source
     * Takes a data frame from the unix socket and turns it into a Vector Event.
     **/
    fn handle_event(&self, received_from: Option<Bytes>, frame: Bytes) -> Option<Event> {
        let mut event = Event::new_empty_log();

        let log_event = event.as_mut_log();

        let frame_size = frame.len();

        if let Some(host) = received_from {
            log_event.insert(self.host_key(), host);
        }

        if self.raw_data_only {
            log_event.insert(
                &self.schema.dnstap_root_data_schema().raw_data(),
                base64::encode(&frame),
            );
            emit!(&DnstapEventReceived {
                byte_size: frame_size
            });
            Some(event)
        } else {
            match parse_dnstap_data(&self.schema, log_event, frame) {
                Err(err) => {
                    emit!(&DnstapParseDataError {
                        error: format!("Dnstap protobuf decode error {:?}.", err).as_str()
                    });
                    None
                }
                Ok(_) => {
                    emit!(&DnstapEventReceived {
                        byte_size: frame_size
                    });
                    Some(event)
                }
            }
        }
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

    fn host_key(&self) -> String {
        self.host_key.clone()
    }

    fn timestamp_key(&self) -> String {
        self.timestamp_key.clone()
    }
}

#[cfg(all(test, feature = "dnstap-integration-tests"))]
mod integration_tests {
    #![allow(clippy::print_stdout)] // tests

    use std::{env, path::Path, process::Command, thread};

    use futures::StreamExt;
    use serde_json::json;
    use tokio::time;

    use super::*;
    use crate::{event::Value, test_util::trace_init, Pipeline};

    async fn test_dnstap(raw_data: bool, query_type: &'static str) {
        trace_init();

        let (sender, mut recv) = Pipeline::new_test();

        tokio::spawn(async move {
            let socket = get_socket(raw_data, query_type);

            DnstapConfig {
                max_frame_length: 102400,
                host_key: Some("key".to_string()),
                socket_path: socket,
                raw_data_only: Some(raw_data),
                multithreaded: Some(false),
                max_frame_handling_tasks: Some(100000),
                socket_file_mode: Some(511),
                socket_receive_buffer_size: Some(10485760),
                socket_send_buffer_size: Some(10485760),
            }
            .build(SourceContext::new_test(sender))
            .await
            .unwrap()
            .await
            .unwrap()
        });

        send_query(raw_data, query_type);

        let event = time::timeout(time::Duration::from_secs(10), recv.next())
            .await
            .expect("fetch dnstap source event timeout")
            .expect("failed to get dnstap source event from a stream");
        let mut events = vec![event];
        loop {
            match time::timeout(time::Duration::from_secs(1), recv.next()).await {
                Ok(Some(event)) => events.push(event),
                Ok(None) => {
                    println!("None: No event");
                    break;
                }
                Err(e) => {
                    println!("Error: {}", e.to_string());
                    break;
                }
            }
        }

        verify_events(raw_data, query_type, &events);

        cleanup(raw_data, query_type);
    }

    fn send_query(raw_data: bool, query_type: &'static str) {
        tokio::spawn(async move {
            let socket = get_socket(raw_data, query_type);
            let dnstap_sock_file = Path::new(&socket);
            let container_tool = get_container_tool();
            let (bind, port) = get_bind_and_port(raw_data, query_type);

            loop {
                thread::sleep(time::Duration::from_millis(100));
                if dnstap_sock_file.exists() {
                    thread::sleep(time::Duration::from_millis(100));
                    start_bind(&container_tool, bind, port);
                    thread::sleep(time::Duration::from_millis(100));
                    match query_type {
                        "query" => {
                            nslookup(&container_tool, port);
                        }
                        "update" => {
                            nsupdate(&container_tool);
                        }
                        _ => (),
                    }
                    break;
                }
            }
        });
    }

    fn verify_events(raw_data: bool, query_event: &'static str, events: &[Event]) {
        if raw_data {
            assert_eq!(events.len(), 2);
            assert!(
                events.iter().all(|v| v.as_log().get("rawData") != None),
                "No rawData field!"
            );
        } else if query_event == "query" {
            assert_eq!(events.len(), 2);
            assert!(
                events
                    .iter()
                    .any(|v| v.as_log().get("messageType")
                        == Some(&Value::Bytes("ClientQuery".into()))),
                "No ClientQuery event!"
            );
            assert!(
                events.iter().any(|v| v.as_log().get("messageType")
                    == Some(&Value::Bytes("ClientResponse".into()))),
                "No ClientResponse event!"
            );
        } else if query_event == "update" {
            assert_eq!(events.len(), 4);
            assert!(
                events
                    .iter()
                    .any(|v| v.as_log().get("messageType")
                        == Some(&Value::Bytes("UpdateQuery".into()))),
                "No UpdateQuery event!"
            );
            assert!(
                events.iter().any(|v| v.as_log().get("messageType")
                    == Some(&Value::Bytes("UpdateResponse".into()))),
                "No UpdateResponse event!"
            );
            assert!(
                events
                    .iter()
                    .any(|v| v.as_log().get("messageType")
                        == Some(&Value::Bytes("AuthQuery".into()))),
                "No UpdateQuery event!"
            );
            assert!(
                events
                    .iter()
                    .any(|v| v.as_log().get("messageType")
                        == Some(&Value::Bytes("AuthResponse".into()))),
                "No UpdateResponse event!"
            );
        }

        for event in events {
            let json = serde_json::to_value(event.as_log().all_fields()).unwrap();
            match query_event {
                "query" => {
                    if json["messageType"] == json!("ClientQuery") {
                        assert_eq!(
                            json["requestData.question[0].domainName"],
                            json!("h1.example.com.")
                        );
                        assert_eq!(json["requestData.rcodeName"], json!("NoError"));
                    } else if json["messageType"] == json!("ClientResponse") {
                        assert_eq!(
                            json["responseData.answers[0].domainName"],
                            json!("h1.example.com.")
                        );
                        assert_eq!(json["responseData.answers[0].rData"], json!("10.0.0.11"));
                        assert_eq!(json["responseData.rcodeName"], json!("NoError"));
                    }
                }
                "update" => {
                    if json["messageType"] == json!("UpdateQuery") {
                        assert_eq!(
                            json["requestData.update[0].domainName"],
                            json!("dh1.example.com.")
                        );
                        assert_eq!(json["requestData.update[0].rData"], json!("10.0.0.21"));
                        assert_eq!(json["requestData.rcodeName"], json!("NoError"));
                    } else if json["messageType"] == json!("UpdateResponse") {
                        assert_eq!(json["responseData.rcodeName"], json!("NoError"));
                    }
                }
                _ => (),
            }
        }
    }

    fn get_socket(raw_data: bool, query_type: &'static str) -> PathBuf {
        let socket_folder = "tests/data/dnstap/socket/".to_owned();
        match query_type {
            "query" => {
                if raw_data {
                    env::current_dir()
                        .unwrap()
                        .join(socket_folder + "dnstap.sock1")
                } else {
                    env::current_dir()
                        .unwrap()
                        .join(socket_folder + "dnstap.sock2")
                }
            }
            "update" => env::current_dir()
                .unwrap()
                .join(socket_folder + "dnstap.sock3"),
            _ => env::current_dir()
                .unwrap()
                .join(socket_folder + "dnstap.sock.default"),
        }
    }

    fn get_container_tool() -> String {
        match env::var_os("CONTAINER_TOOL") {
            Some(val) => val.to_str().unwrap().to_owned(),
            None => String::from("podman"),
        }
    }

    fn get_bind_and_port(raw_data: bool, query_type: &'static str) -> (&str, &str) {
        match query_type {
            "query" => {
                if raw_data {
                    ("/bind1", "8001")
                } else {
                    ("/bind2", "8002")
                }
            }
            "update" => ("/bind3", "8003"),
            _ => ("", ""),
        }
    }

    fn start_bind(container: &str, bind: &'static str, port: &'static str) {
        Command::new(container)
            .arg("exec")
            .arg("vector_dnstap")
            .arg("/usr/sbin/named")
            .arg("-p")
            .arg(port)
            .arg("-t")
            .arg(bind)
            .output()
            .expect("Failed to execute command!");
    }

    fn nslookup(container: &str, port: &'static str) {
        Command::new(container)
            .arg("exec")
            .arg("vector_dnstap")
            .arg("nslookup")
            .arg("-type=A")
            .arg("-port=".to_owned() + port)
            .arg("h1.example.com.")
            .arg("localhost")
            .output()
            .expect("Failed to execute command!");
    }

    fn nsupdate(container: &str) {
        Command::new(container)
            .arg("exec")
            .arg("vector_dnstap")
            .arg("nsupdate")
            .arg("-v")
            .arg("/bind3/etc/bind/nsupdate.txt")
            .output()
            .expect("Failed to execute command!");
    }

    fn get_rndc_port(raw_data: bool, query_type: &'static str) -> &str {
        match query_type {
            "query" => {
                if raw_data {
                    "9001"
                } else {
                    "9002"
                }
            }
            "update" => "9003",
            _ => "",
        }
    }

    fn stop_bind(container: &str, port: &'static str) {
        Command::new(container)
            .arg("exec")
            .arg("vector_dnstap")
            .arg("rndc")
            .arg("-p")
            .arg(port)
            .arg("stop")
            .output()
            .expect("Failed to execute command!");
    }

    fn remove_socket(raw_data: bool, query_type: &'static str) {
        let socket = get_socket(raw_data, query_type);
        let dnstap_sock_file = Path::new(&socket);
        let _ = std::fs::remove_file(dnstap_sock_file);
    }

    fn cleanup(raw_data: bool, query_type: &'static str) {
        stop_bind(&get_container_tool(), get_rndc_port(raw_data, query_type));
        remove_socket(raw_data, query_type);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_dnstap_raw_event() {
        test_dnstap(true, "query").await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_dnstap_query_event() {
        test_dnstap(false, "query").await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_dnstap_update_event() {
        test_dnstap(false, "update").await;
    }
}

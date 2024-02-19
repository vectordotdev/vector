use std::path::PathBuf;

use vector_lib::lookup::owned_value_path;
use vector_lib::{configurable::configurable_component, tls::MaybeTlsSettings};
use vrl::value::{kind::Collection, Kind};

use super::util::framestream::{build_framestream_tcp_source, build_framestream_unix_source};
use crate::{
    config::{log_schema, DataType, SourceConfig, SourceContext, SourceOutput},
    Result,
};

pub mod parser;
pub mod schema;
pub mod tcp;
#[cfg(unix)]
pub mod unix;
use dnsmsg_parser::{dns_message, dns_message_parser};
pub use schema::DnstapEventSchema;
use vector_lib::config::LogNamespace;

/// Configuration for the `dnstap` source.
#[configurable_component(source("dnstap", "Collect DNS logs from a dnstap-compatible server."))]
#[derive(Clone, Debug)]
pub struct DnstapConfig {
    #[serde(flatten)]
    pub mode: Mode,
}

/// Listening mode for the `dnstap` source.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(untagged)]
#[configurable(metadata(docs::enum_tag_description = "The type of dnstap socket to use."))]
#[allow(clippy::large_enum_variant)] // just used for configuration
pub enum Mode {
    /// Listen on TCP.
    Tcp(tcp::TcpConfig),

    /// Listen on a Unix domain socket
    #[cfg(unix)]
    Unix(unix::UnixConfig),
}

impl DnstapConfig {
    pub fn new(socket_path: PathBuf) -> Self {
        Self {
            mode: Mode::Unix(unix::UnixConfig::new(socket_path)),
        }
    }

    fn log_namespace(&self) -> LogNamespace {
        match &self.mode {
            Mode::Tcp(config) => config.log_namespace.unwrap_or(false).into(),
            #[cfg(unix)]
            Mode::Unix(config) => config.log_namespace.unwrap_or(false).into(),
        }
    }

    fn raw_data_only(&self) -> bool {
        match &self.mode {
            Mode::Tcp(config) => config.raw_data_only.unwrap_or(false),
            #[cfg(unix)]
            Mode::Unix(config) => config.raw_data_only.unwrap_or(false),
        }
    }

    pub fn schema_definition(&self, log_namespace: LogNamespace) -> vector_lib::schema::Definition {
        let event_schema = DnstapEventSchema;

        match self.log_namespace() {
            LogNamespace::Legacy => {
                let schema = vector_lib::schema::Definition::empty_legacy_namespace();

                if self.raw_data_only() {
                    if let Some(message_key) = log_schema().message_key() {
                        return schema.with_event_field(
                            message_key,
                            Kind::bytes(),
                            Some("message"),
                        );
                    }
                }
                event_schema.schema_definition(schema)
            }
            LogNamespace::Vector => {
                let schema = vector_lib::schema::Definition::new_with_default_metadata(
                    Kind::object(Collection::empty()),
                    [log_namespace],
                )
                .with_standard_vector_source_metadata();

                if self.raw_data_only() {
                    schema.with_event_field(
                        &owned_value_path!("message"),
                        Kind::bytes(),
                        Some("message"),
                    )
                } else {
                    event_schema.schema_definition(schema)
                }
            }
        }
    }
}

impl Default for DnstapConfig {
    fn default() -> Self {
        Self {
            #[cfg(unix)]
            mode: Mode::Unix(unix::UnixConfig::default()),
            #[cfg(not(unix))]
            mode: Mode::Tcp(tcp::TcpConfig::from_address(std::net::SocketAddr::new(
                std::net::IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0)),
                9000,
            ))),
        }
    }
}

impl_generate_config_from_default!(DnstapConfig);

impl From<tcp::TcpConfig> for DnstapConfig {
    fn from(config: tcp::TcpConfig) -> Self {
        DnstapConfig {
            mode: Mode::Tcp(config),
        }
    }
}

#[cfg(unix)]
impl From<unix::UnixConfig> for DnstapConfig {
    fn from(config: unix::UnixConfig) -> Self {
        DnstapConfig {
            mode: Mode::Unix(config),
        }
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "dnstap")]
impl SourceConfig for DnstapConfig {
    async fn build(&self, cx: SourceContext) -> Result<super::Source> {
        match &self.mode {
            Mode::Tcp(config) => {
                let log_namespace = cx.log_namespace(config.log_namespace);
                let tls_config = config.tls().as_ref().map(|tls| tls.tls_config.clone());

                let tls = MaybeTlsSettings::from_config(&tls_config, true)?;
                let frame_handler =
                    tcp::DnstapFrameHandler::new(config.clone(), tls, log_namespace);

                build_framestream_tcp_source(frame_handler, cx.shutdown, cx.out)
            }
            #[cfg(unix)]
            Mode::Unix(config) => {
                let log_namespace = cx.log_namespace(config.log_namespace);
                let frame_handler = unix::DnstapFrameHandler::new(config.clone(), log_namespace);
                build_framestream_unix_source(frame_handler, cx.shutdown, cx.out)
            }
        }
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(Some(self.log_namespace()));
        let schema_definition = self
            .schema_definition(log_namespace)
            .with_standard_vector_source_metadata();
        vec![SourceOutput::new_logs(DataType::Log, schema_definition)]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use vector_lib::event::{Event, LogEvent};

    use super::*;

    #[test]
    fn simple_matches_schema() {
        let record = r#"{"dataType":"Message",
                         "dataTypeId":1,
                         "messageType":"ClientQuery",
                         "messageTypeId":5,
                         "requestData":{
                           "fullRcode":0,
                           "header":{
                             "aa":false,
                             "ad":true,
                             "anCount":0,
                             "arCount":1,
                             "cd":false,
                             "id":38339,
                             "nsCount":0,
                             "opcode":0,
                             "qdCount":1,
                             "qr":0,
                             "ra":false,
                             "rcode":0,
                             "rd":true,
                             "tc":false},
                           "opt":{
                             "do":false,
                             "ednsVersion":0,
                             "extendedRcode":0,
                             "options":[{"optCode":10,
                                         "optName":"Cookie",
                                         "optValue":"5JiWq4VYa7U="}],
                             "udpPayloadSize":1232},
                           "question":[{"class":"IN","domainName":"whoami.example.org.","questionType":"A","questionTypeId":1}],
                           "rcodeName":"NoError",
                           "time":1667909880863224758,
                           "timePrecision":"ns"},
                         "serverId":"stephenwakely-Precision-5570",
                         "serverVersion":"CoreDNS-1.10.0",
                         "socketFamily":"INET",
                         "socketProtocol":"UDP",
                         "sourceAddress":"0.0.0.0",
                         "sourcePort":54782,
                         "source_type":"dnstap",
                         "time":1667909880863224758,
                         "timePrecision":"ns"
                         }"#;

        let json: serde_json::Value = serde_json::from_str(record).unwrap();
        let mut event = Event::from(LogEvent::from(vrl::value::Value::from(json)));
        event.as_mut_log().insert("timestamp", chrono::Utc::now());

        let definition = DnstapEventSchema;
        let schema = vector_lib::schema::Definition::empty_legacy_namespace()
            .with_standard_vector_source_metadata();

        definition
            .schema_definition(schema)
            .assert_valid_for_event(&event)
    }
}

#[cfg(all(test, feature = "dnstap-integration-tests"))]
mod integration_tests {
    #![allow(clippy::print_stdout)] // tests

    use bollard::exec::{CreateExecOptions, StartExecOptions};
    use bollard::Docker;
    use futures::StreamExt;
    use serde_json::json;
    use tokio::time;
    use vector_lib::event::Event;
    use vector_lib::lookup::lookup_v2::OptionalValuePath;

    use self::unix::UnixConfig;

    use super::*;
    use crate::{
        event::Value,
        test_util::{
            components::{assert_source_compliance, SOURCE_TAGS},
            wait_for,
        },
        SourceSender,
    };

    async fn test_dnstap(raw_data: bool, query_type: &'static str) {
        assert_source_compliance(&SOURCE_TAGS, async {
            let (sender, mut recv) = SourceSender::new_test();

            tokio::spawn(async move {
                let socket = get_socket(raw_data, query_type);

                DnstapConfig {
                    mode: Mode::Unix(UnixConfig {
                        max_frame_length: 102400,
                        host_key: Some(OptionalValuePath::from(owned_value_path!("key"))),
                        socket_path: socket,
                        raw_data_only: Some(raw_data),
                        multithreaded: Some(false),
                        max_frame_handling_tasks: Some(100000),
                        socket_file_mode: Some(511),
                        socket_receive_buffer_size: Some(10485760),
                        socket_send_buffer_size: Some(10485760),
                        log_namespace: None,
                    }),
                }
                .build(SourceContext::new_test(sender, None))
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
                        println!("Error: {}", e);
                        break;
                    }
                }
            }

            verify_events(raw_data, query_type, &events);
        })
        .await;
    }

    fn send_query(raw_data: bool, query_type: &'static str) {
        tokio::spawn(async move {
            let socket_path = get_socket(raw_data, query_type);
            let (query_port, control_port) = get_bind_ports(raw_data, query_type);

            // Wait for the source to create its respective socket before telling BIND to reload, causing it to open
            // that new socket file.
            wait_for(move || {
                let path = socket_path.clone();
                async move { path.exists() }
            })
            .await;

            // Now instruct BIND to reopen its DNSTAP socket file and execute the given query.
            reload_bind_dnstap_socket(control_port).await;

            match query_type {
                "query" => {
                    nslookup(query_port).await;
                }
                "update" => {
                    nsupdate().await;
                }
                _ => (),
            }
        });
    }

    fn verify_events(raw_data: bool, query_event: &'static str, events: &[Event]) {
        if raw_data {
            assert_eq!(events.len(), 2);
            assert!(
                events.iter().all(|v| v.as_log().get("rawData").is_some()),
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
            let json = serde_json::to_value(event.as_log().all_event_fields().unwrap()).unwrap();
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

    fn get_container() -> String {
        std::env::var("CONTAINER_NAME").unwrap_or_else(|_| "vector_dnstap".into())
    }

    fn get_socket(raw_data: bool, query_type: &'static str) -> PathBuf {
        let socket_folder = std::env::var("BIND_SOCKET")
            .map(PathBuf::from)
            .expect("BIND socket directory must be specified via BIND_SOCKET");

        match query_type {
            "query" if raw_data => socket_folder.join("dnstap.sock1"),
            "query" => socket_folder.join("dnstap.sock2"),
            "update" => socket_folder.join("dnstap.sock3"),
            _ => unreachable!("no other test variants should exist"),
        }
    }

    fn get_bind_ports(raw_data: bool, query_type: &'static str) -> (&str, &str) {
        // Returns the query port and control port, respectively, for the given BIND instance.
        match query_type {
            "query" if raw_data => ("8001", "9001"),
            "query" => ("8002", "9002"),
            "update" => ("8003", "9003"),
            _ => unreachable!("no other test variants should exist"),
        }
    }

    async fn dnstap_exec(cmd: Vec<&str>) {
        let docker = Docker::connect_with_unix_defaults().expect("failed binding to docker socket");
        let config = CreateExecOptions {
            cmd: Some(cmd),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            ..Default::default()
        };
        let result = docker
            .create_exec(get_container().as_str(), config)
            .await
            .expect("failed to execute command");
        docker
            .start_exec(&result.id, None::<StartExecOptions>)
            .await
            .expect("failed to execute command");
    }

    async fn reload_bind_dnstap_socket(control_port: &str) {
        dnstap_exec(vec![
            "/usr/sbin/rndc",
            "-p",
            control_port,
            "dnstap",
            "-reopen",
        ])
        .await
    }

    async fn nslookup(port: &str) {
        dnstap_exec(vec![
            "nslookup",
            "-type=A",
            format!("-port={}", port).as_str(),
            "h1.example.com",
            "localhost",
        ])
        .await
    }

    async fn nsupdate() {
        dnstap_exec(vec!["nsupdate", "-v", "/bind3/etc/bind/nsupdate.txt"]).await
    }

    #[tokio::test]
    async fn test_dnstap_raw_event() {
        test_dnstap(true, "query").await;
    }

    #[tokio::test]
    async fn test_dnstap_query_event() {
        test_dnstap(false, "query").await;
    }

    #[tokio::test]
    async fn test_dnstap_update_event() {
        test_dnstap(false, "update").await;
    }
}

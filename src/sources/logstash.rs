use super::util::{SocketListenAddr, TcpIsErrorFatal, TcpSource};
use crate::{
    config::{
        log_schema, DataType, GenerateConfig, Resource, SourceConfig, SourceContext,
        SourceDescription,
    },
    event::{Event, LogEvent, Value},
    tcp::TcpKeepaliveConfig,
    tls::{MaybeTlsSettings, TlsConfig},
};
use bytes::{Buf, Bytes, BytesMut};
use flate2::read::ZlibDecoder;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::{
    collections::{BTreeMap, VecDeque},
    io::{self, Read},
};
use tokio_util::codec::Decoder;

// TODO
// * Handle acking
// * Handle protocol version differences
// * Handle Data frames
// * usize casts bounds
// * Integration tests

#[derive(Deserialize, Serialize, Debug)]
pub struct LogstashConfig {
    address: SocketListenAddr,
    keepalive: Option<TcpKeepaliveConfig>,
    tls: Option<TlsConfig>,
    receive_buffer_bytes: Option<usize>,
}

inventory::submit! {
    SourceDescription::new::<LogstashConfig>("logstash")
}

impl GenerateConfig for LogstashConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            address: SocketListenAddr::SocketAddr("0.0.0.0:5000".parse().unwrap()),
            keepalive: None,
            tls: None,
            receive_buffer_bytes: None,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "logstash")]
impl SourceConfig for LogstashConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let source = LogstashSource {};
        let shutdown_secs = 30;
        let tls = MaybeTlsSettings::from_config(&self.tls, true)?;
        source.run(
            self.address,
            self.keepalive,
            shutdown_secs,
            tls,
            self.receive_buffer_bytes,
            cx.shutdown,
            cx.out,
        )
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "logstash"
    }

    fn resources(&self) -> Vec<Resource> {
        vec![self.address.into()]
    }
}

#[derive(Debug, Clone)]
struct LogstashSource {}

impl TcpSource for LogstashSource {
    type Error = DecodeError;
    type Decoder = LogstashDecoder;

    fn decoder(&self) -> Self::Decoder {
        LogstashDecoder::new()
    }

    fn build_event(
        &self,
        frame: BTreeMap<String, serde_json::Value>,
        host: Bytes,
    ) -> Option<Event> {
        let mut log = LogEvent::from(
            frame
                .into_iter()
                .map(|(key, value)| (key, Value::from(value)))
                .collect::<BTreeMap<_, _>>(),
        );
        if log.get(log_schema().host_key()).is_none() {
            log.insert(log_schema().host_key(), host);
        }
        if log.get(log_schema().timestamp_key()).is_none() {
            if let Some(timestamp) = log.get("@timestamp") {
                let timestamp = timestamp.clone();
                log.insert(log_schema().timestamp_key(), timestamp);
            }
        }
        Some(Event::from(log))
    }
}

#[derive(Debug, Clone)]
enum LogstashDecoderReadState {
    ReadProtocol,
    ReadType(LogstashProtocolVersion),
    ReadFrame(LogstashProtocolVersion, LogstashFrameType),
    PendingFrames(VecDeque<BTreeMap<String, serde_json::Value>>),
}

#[derive(Clone, Debug)]
struct LogstashDecoder {
    state: LogstashDecoderReadState,
}

impl LogstashDecoder {
    fn new() -> Self {
        Self {
            state: LogstashDecoderReadState::ReadProtocol,
        }
    }
}

#[derive(Debug, Snafu)]
pub enum DecodeError {
    #[snafu(display("i/o error: {}", source))]
    IO { source: io::Error },
    #[snafu(display("Unknown logstash protocol version: {}", version))]
    UnknownProtocolVersion { version: char },
    #[snafu(display("Unknown logstash protocol message type: {}", message_type))]
    UnknownMessageType { message_type: char },
    #[snafu(display("Failed to decode JSON frame: {}", source))]
    JsonFrameFailedDecode { source: serde_json::Error },
    #[snafu(display("Failed to decompress compressed frame: {}", source))]
    DecompressionFailed { source: io::Error },
}

impl TcpIsErrorFatal for DecodeError {
    fn is_error_fatal(&self) -> bool {
        use DecodeError::*;

        match self {
            IO { .. } => true,
            UnknownProtocolVersion { .. } => true,
            UnknownMessageType { .. } => true,
            JsonFrameFailedDecode { .. } => false,
            DecompressionFailed { .. } => false,
        }
    }
}

impl From<io::Error> for DecodeError {
    fn from(source: io::Error) -> Self {
        DecodeError::IO { source }
    }
}

#[derive(Debug, Clone, Copy)]
enum LogstashProtocolVersion {
    V1, // 1
    V2, // 2
}

#[derive(Debug, Clone, Copy)]
enum LogstashFrameType {
    WindowSize, // W
    Data,       // D
    Json,       // J
    Compressed, // C
}

// Based on spec at: https://github.com/logstash-plugins/logstash-input-beats/blob/master/PROTOCOL.md
// And implementation from logstash: https://github.com/logstash-plugins/logstash-input-beats/blob/27bad62a26a81fc000a9d21495b8dc7174ab63e9/src/main/java/org/logstash/beats/BeatsParser.java
impl Decoder for LogstashDecoder {
    type Item = BTreeMap<String, serde_json::Value>;
    type Error = DecodeError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        loop {
            match self.state {
                LogstashDecoderReadState::PendingFrames(ref mut frames) => {
                    match frames.pop_front() {
                        Some(frame) => return Ok(Some(frame)),
                        None => {
                            self.state = LogstashDecoderReadState::ReadProtocol;
                        }
                    }
                }
                LogstashDecoderReadState::ReadProtocol => {
                    if src.remaining() < 1 {
                        return Ok(None);
                    }
                    match src.get_u8() {
                        b'1' => {
                            self.state =
                                LogstashDecoderReadState::ReadType(LogstashProtocolVersion::V1);
                        }
                        b'2' => {
                            self.state =
                                LogstashDecoderReadState::ReadType(LogstashProtocolVersion::V2);
                        }
                        version => {
                            return Err(DecodeError::UnknownProtocolVersion {
                                version: version as char,
                            });
                        }
                    }
                }
                LogstashDecoderReadState::ReadType(version) => {
                    if src.remaining() < 1 {
                        return Ok(None);
                    }

                    match src.get_u8() {
                        b'W' => {
                            self.state = LogstashDecoderReadState::ReadFrame(
                                version,
                                LogstashFrameType::WindowSize,
                            );
                        }
                        b'D' => {
                            self.state = LogstashDecoderReadState::ReadFrame(
                                version,
                                LogstashFrameType::Data,
                            )
                        }
                        b'J' => {
                            self.state = LogstashDecoderReadState::ReadFrame(
                                version,
                                LogstashFrameType::Json,
                            )
                        }
                        b'C' => {
                            self.state = LogstashDecoderReadState::ReadFrame(
                                version,
                                LogstashFrameType::Compressed,
                            )
                        }
                        message_type => {
                            return Err(DecodeError::UnknownMessageType {
                                message_type: message_type as char,
                            });
                        }
                    }
                }
                // The window size indicates how many events the writer will send before waiting
                // for acks. As we forward events as we get them, and ack as they are receieved, we
                // do not need to keep track of this.
                LogstashDecoderReadState::ReadFrame(_version, LogstashFrameType::WindowSize) => {
                    if src.remaining() < 4 {
                        return Ok(None);
                    }

                    let _window_size = src.get_u32();
                    self.state = LogstashDecoderReadState::ReadProtocol;
                }
                LogstashDecoderReadState::ReadFrame(_version, LogstashFrameType::Data) => {
                    let mut rest = src.as_ref();

                    if rest.remaining() < 8 {
                        return Ok(None);
                    }
                    let _sequence_number = rest.get_u32();
                    let pair_count = rest.get_u32();

                    let mut fields: BTreeMap<String, serde_json::Value> = BTreeMap::new();
                    for _ in 0..pair_count {
                        if src.remaining() < 4 {
                            return Ok(None);
                        }
                        let key_length = rest.get_u32() as usize;

                        if rest.remaining() < key_length {
                            return Ok(None);
                        }
                        let (key, right) = rest.split_at(key_length);
                        rest = right;

                        if src.remaining() < 4 {
                            return Ok(None);
                        }
                        let value_length = rest.get_u32() as usize;
                        if rest.remaining() < value_length {
                            return Ok(None);
                        }
                        let (value, right) = rest.split_at(value_length);
                        rest = right;

                        fields.insert(
                            String::from_utf8_lossy(key).to_string(),
                            String::from_utf8_lossy(value).into(),
                        );
                    }

                    let remaining = rest.remaining();

                    src.advance(src.remaining() - remaining);

                    self.state = LogstashDecoderReadState::ReadProtocol;

                    return Ok(Some(fields));
                }
                LogstashDecoderReadState::ReadFrame(_version, LogstashFrameType::Json) => {
                    let mut rest = src.as_ref();

                    if rest.remaining() < 8 {
                        return Ok(None);
                    }
                    let _sequence_number = rest.get_u32();
                    let payload_size = rest.get_u32() as usize;

                    if rest.remaining() < payload_size {
                        src.reserve(payload_size as usize);
                        return Ok(None);
                    }

                    let (slice, right) = rest.split_at(payload_size);
                    rest = right;

                    let fields_result: Result<BTreeMap<String, serde_json::Value>, _> =
                        serde_json::from_slice(slice).context(JsonFrameFailedDecode {});

                    let remaining = rest.remaining();

                    src.advance(src.remaining() - remaining);

                    self.state = LogstashDecoderReadState::ReadProtocol;

                    return fields_result.map(Option::Some);
                }
                LogstashDecoderReadState::ReadFrame(_version, LogstashFrameType::Compressed) => {
                    let mut rest = src.as_ref();

                    if rest.remaining() < 4 {
                        return Ok(None);
                    }
                    let payload_size = rest.get_u32() as usize;

                    if rest.remaining() < payload_size {
                        src.reserve(payload_size as usize);
                        return Ok(None);
                    }

                    let (slice, right) = rest.split_at(payload_size);
                    rest = right;

                    let mut buf = {
                        let mut buf = Vec::new();

                        let res = ZlibDecoder::new(io::Cursor::new(slice))
                            .read_to_end(&mut buf)
                            .context(DecompressionFailed)
                            .map(|_| BytesMut::from(&buf[..]));

                        let remaining = rest.remaining();

                        src.advance(src.remaining() - remaining);

                        res
                    }?;

                    let mut decoder = LogstashDecoder::new();

                    let mut frames = VecDeque::new();

                    while let Some(s) = decoder.decode(&mut buf)? {
                        frames.push_back(s);
                    }

                    self.state = LogstashDecoderReadState::PendingFrames(frames);
                }
            };
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<LogstashConfig>();
    }
}

#[cfg(all(test, feature = "logstash-integration-tests"))]
mod integration_tests {
    use super::*;
    use crate::{
        config::SourceContext,
        docker::docker,
        test_util::{collect_ready, next_addr_for_ip, trace_init, wait_for_tcp},
        Pipeline,
    };
    use bollard::{
        container::{Config as ContainerConfig, CreateContainerOptions},
        image::{CreateImageOptions, ListImagesOptions},
        models::HostConfig,
        Docker,
    };
    use futures::{channel::mpsc, StreamExt};
    use std::{collections::HashMap, fs::File, io::Write, net::SocketAddr, time::Duration};
    use tokio::time::sleep;
    use uuid::Uuid;

    #[tokio::test]
    async fn heartbeat() {
        trace_init();

        let image = "docker.elastic.co/beats/heartbeat";
        let tag = "7.12.1";

        let docker = docker(None, None).unwrap();

        let (out, address) = source().await;

        pull_image(&docker, image, tag).await;

        let dir = tempfile::tempdir().unwrap();
        let mut file = File::create(dir.path().join("heartbeat.yml")).unwrap();
        write!(
            &mut file,
            r#"
heartbeat.monitors:
- type: http
  schedule: '@every 1s'
  urls:
    - https://google.com

output.logstash:
  hosts: ['host.docker.internal:{}']
"#,
            address.port()
        )
        .unwrap();

        let options = Some(CreateContainerOptions {
            name: format!("vector_test_logstash_{}", Uuid::new_v4()),
        });
        let config = ContainerConfig {
            image: Some(format!("{}:{}", image, tag)),
            host_config: Some(HostConfig {
                network_mode: Some(String::from("host")),
                extra_hosts: Some(vec![String::from("host.docker.internal:host-gateway")]),
                binds: Some(vec![format!(
                    "{}/heartbeat.yml:{}",
                    dir.path().display(),
                    "/usr/share/heartbeat/heartbeat.yml"
                )]),
                ..Default::default()
            }),
            ..Default::default()
        };

        let container = docker.create_container(options, config).await.unwrap();

        docker
            .start_container::<String>(&container.id, None)
            .await
            .unwrap();

        sleep(Duration::from_secs(5)).await;

        let events = collect_ready(out).await;

        remove_container(&docker, &container.id).await;

        assert!(!events.is_empty());

        let log = events[0].as_log();
        assert_eq!(
            log.get("@metadata.beat"),
            Some(String::from("heartbeat").into()).as_ref()
        );
        assert_eq!(log.get("summary.up"), Some(1.into()).as_ref());
        assert!(log.get("timestamp").is_some());
        assert!(log.get("host").is_some());
    }

    async fn pull_image(docker: &Docker, image: &str, tag: &str) {
        let mut filters = HashMap::new();
        filters.insert(
            String::from("reference"),
            vec![format!("{}:{}", image, tag)],
        );

        let options = Some(ListImagesOptions {
            filters,
            ..Default::default()
        });

        let images = docker.list_images(options).await.unwrap();
        if images.is_empty() {
            // If not found, pull it
            let options = Some(CreateImageOptions {
                from_image: image,
                tag,
                ..Default::default()
            });

            docker
                .create_image(options, None, None)
                .for_each(|item| async move {
                    let info = item.unwrap();
                    if let Some(error) = info.error {
                        panic!("{:?}", error);
                    }
                })
                .await
        }
    }

    async fn source() -> (mpsc::Receiver<Event>, SocketAddr) {
        let (sender, recv) = Pipeline::new_test();
        let address = next_addr_for_ip(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
        tokio::spawn(async move {
            LogstashConfig {
                address: address.into(),
                tls: None,
                keepalive: None,
                receive_buffer_bytes: None,
            }
            .build(SourceContext::new_test(sender))
            .await
            .unwrap()
            .await
            .unwrap()
        });
        wait_for_tcp(address).await;
        (recv, address)
    }

    async fn remove_container(docker: &Docker, id: &str) {
        trace!("Stopping container.");

        let _ = docker
            .stop_container(id, None)
            .await
            .map_err(|e| error!(%e));

        trace!("Removing container.");

        // Don't panic, as this is unrelated to the test
        let _ = docker
            .remove_container(id, None)
            .await
            .map_err(|e| error!(%e));
    }
}

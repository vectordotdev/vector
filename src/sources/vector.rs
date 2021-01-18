use super::util::{SocketListenAddr, TcpSource};
use crate::{
    config::{DataType, GenerateConfig, GlobalOptions, Resource, SourceConfig, SourceDescription},
    event::proto,
    internal_events::{VectorEventReceived, VectorProtoDecodeError},
    shutdown::ShutdownSignal,
    tcp::TcpKeepaliveConfig,
    tls::{MaybeTlsSettings, TlsConfig},
    Event, Pipeline,
};
use bytes::{Bytes, BytesMut};
use prost::Message;
use serde::{Deserialize, Serialize};
use tokio_util::codec::LengthDelimitedCodec;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct VectorConfig {
    pub address: SocketListenAddr,
    pub keepalive: Option<TcpKeepaliveConfig>,
    #[serde(default = "default_shutdown_timeout_secs")]
    pub shutdown_timeout_secs: u64,
    tls: Option<TlsConfig>,
}

fn default_shutdown_timeout_secs() -> u64 {
    30
}

#[cfg(test)]
impl VectorConfig {
    pub fn new(
        address: SocketListenAddr,
        keepalive: Option<TcpKeepaliveConfig>,
        tls: Option<TlsConfig>,
    ) -> Self {
        Self {
            address,
            keepalive,
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
            tls,
        }
    }
}

inventory::submit! {
    SourceDescription::new::<VectorConfig>("vector")
}

impl GenerateConfig for VectorConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            address: SocketListenAddr::SocketAddr("0.0.0.0:9000".parse().unwrap()),
            keepalive: None,
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
            tls: None,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "vector")]
impl SourceConfig for VectorConfig {
    async fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> crate::Result<super::Source> {
        let vector = VectorSource;
        let tls = MaybeTlsSettings::from_config(&self.tls, true)?;
        vector.run(
            self.address,
            self.keepalive,
            self.shutdown_timeout_secs,
            tls,
            shutdown,
            out,
        )
    }

    fn output_type(&self) -> DataType {
        DataType::Any
    }

    fn source_type(&self) -> &'static str {
        "vector"
    }

    fn resources(&self) -> Vec<Resource> {
        vec![self.address.into()]
    }
}

#[derive(Debug, Clone)]
struct VectorSource;

impl TcpSource for VectorSource {
    type Error = std::io::Error;
    type Decoder = LengthDelimitedCodec;

    fn decoder(&self) -> Self::Decoder {
        LengthDelimitedCodec::new()
    }

    fn build_event(&self, frame: BytesMut, _host: Bytes) -> Option<Event> {
        let byte_size = frame.len();
        match proto::EventWrapper::decode(frame).map(Event::from) {
            Ok(event) => {
                emit!(VectorEventReceived { byte_size });
                Some(event)
            }
            Err(error) => {
                emit!(VectorProtoDecodeError { error });
                None
            }
        }
    }
}

#[cfg(feature = "sinks-vector")]
#[cfg(test)]
mod test {
    use super::VectorConfig;
    use crate::shutdown::ShutdownSignal;
    use crate::{
        config::{GlobalOptions, SinkConfig, SinkContext, SourceConfig},
        event::{
            metric::{MetricKind, MetricValue},
            Metric,
        },
        sinks::vector::VectorSinkConfig,
        test_util::{collect_ready, next_addr, wait_for_tcp},
        tls::{TlsConfig, TlsOptions},
        Event, Pipeline,
    };
    use futures::stream;
    use std::net::SocketAddr;
    use tokio::time::{delay_for, Duration};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<VectorConfig>();
    }

    async fn stream_test(addr: SocketAddr, source: VectorConfig, sink: VectorSinkConfig) {
        let (tx, rx) = Pipeline::new_test();

        let server = source
            .build(
                "default",
                &GlobalOptions::default(),
                ShutdownSignal::noop(),
                tx,
            )
            .await
            .unwrap();
        tokio::spawn(server);
        wait_for_tcp(addr).await;

        let cx = SinkContext::new_test();
        let (sink, _) = sink.build(cx).await.unwrap();

        let events = vec![
            Event::from("test"),
            Event::from("events"),
            Event::from("to roundtrip"),
            Event::from("through"),
            Event::from("the native"),
            Event::from("sink"),
            Event::from("and"),
            Event::from("source"),
            Event::Metric(Metric::new(
                String::from("also test a metric"),
                None,
                None,
                None,
                MetricKind::Absolute,
                MetricValue::Counter { value: 1.0 },
            )),
        ];

        sink.run(stream::iter(events.clone())).await.unwrap();

        delay_for(Duration::from_millis(50)).await;

        let output = collect_ready(rx).await;
        assert_eq!(events, output);
    }

    #[tokio::test]
    async fn it_works_with_vector_sink() {
        let addr = next_addr();
        stream_test(
            addr,
            VectorConfig::new(addr.into(), None, None),
            VectorSinkConfig {
                address: format!("localhost:{}", addr.port()),
                keepalive: None,
                tls: None,
            },
        )
        .await;
    }

    #[tokio::test]
    async fn it_works_with_vector_sink_tls() {
        let addr = next_addr();
        stream_test(
            addr,
            VectorConfig::new(addr.into(), None, Some(TlsConfig::test_config())),
            VectorSinkConfig {
                address: format!("localhost:{}", addr.port()),
                keepalive: None,
                tls: Some(TlsConfig {
                    enabled: Some(true),
                    options: TlsOptions {
                        verify_certificate: Some(false),
                        ..Default::default()
                    },
                }),
            },
        )
        .await;
    }
}

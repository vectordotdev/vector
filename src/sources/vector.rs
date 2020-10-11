use super::util::{SocketListenAddr, TcpSource};
use crate::{
    config::{DataType, GenerateConfig, GlobalOptions, SourceConfig, SourceDescription},
    event::proto,
    internal_events::{VectorEventReceived, VectorProtoDecodeError},
    shutdown::ShutdownSignal,
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
    #[serde(default = "default_shutdown_timeout_secs")]
    pub shutdown_timeout_secs: u64,
    tls: Option<TlsConfig>,
}

fn default_shutdown_timeout_secs() -> u64 {
    30
}

#[cfg(test)]
impl VectorConfig {
    pub fn new(address: SocketListenAddr, tls: Option<TlsConfig>) -> Self {
        Self {
            address,
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
            tls,
        }
    }
}

inventory::submit! {
    SourceDescription::new::<VectorConfig>("vector")
}

impl GenerateConfig for VectorConfig {}

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
        vector.run(self.address, self.shutdown_timeout_secs, tls, shutdown, out)
    }

    fn output_type(&self) -> DataType {
        DataType::Any
    }

    fn source_type(&self) -> &'static str {
        "vector"
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
    use futures::{compat::Future01CompatExt, stream};
    use std::net::SocketAddr;
    use tokio::time::{delay_for, Duration};

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
            .unwrap()
            .compat();
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
            Event::Metric(Metric {
                name: String::from("also test a metric"),
                timestamp: None,
                tags: None,
                kind: MetricKind::Absolute,
                value: MetricValue::Counter { value: 1.0 },
            }),
        ];

        sink.run(stream::iter(events.clone())).await.unwrap();

        delay_for(Duration::from_millis(50)).await;

        let output = collect_ready(rx).await.unwrap();
        assert_eq!(events, output);
    }

    #[tokio::test]
    async fn it_works_with_vector_sink() {
        let addr = next_addr();
        stream_test(
            addr,
            VectorConfig::new(addr.into(), None),
            VectorSinkConfig {
                address: format!("localhost:{}", addr.port()),
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
            VectorConfig::new(
                addr.into(),
                Some(TlsConfig {
                    enabled: Some(true),
                    options: TlsOptions {
                        crt_file: Some("tests/data/localhost.crt".into()),
                        key_file: Some("tests/data/localhost.key".into()),
                        ..Default::default()
                    },
                }),
            ),
            VectorSinkConfig {
                address: format!("localhost:{}", addr.port()),
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

use super::util::{SocketListenAddr, TcpSource};
use crate::{
    event::proto,
    internal_events::{VectorEventReceived, VectorProtoDecodeError},
    shutdown::ShutdownSignal,
    tls::{MaybeTlsSettings, TlsConfig},
    topology::config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
    Event,
};
use bytes::{Bytes, BytesMut};
use futures01::sync::mpsc;
use prost::Message;
use serde::{Deserialize, Serialize};
use tokio01::codec::LengthDelimitedCodec;

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
    SourceDescription::new_without_default::<VectorConfig>("vector")
}

#[typetag::serde(name = "vector")]
impl SourceConfig for VectorConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: mpsc::Sender<Event>,
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
        event::{
            metric::{MetricKind, MetricValue},
            Metric,
        },
        sinks::vector::VectorSinkConfig,
        test_util::{next_addr, runtime, wait_for_tcp, CollectCurrent},
        tls::{TlsConfig, TlsOptions},
        topology::config::{GlobalOptions, SinkConfig, SinkContext, SourceConfig},
        Event,
    };
    use futures01::{stream, sync::mpsc, Future, Sink};
    use std::net::SocketAddr;

    fn stream_test(addr: SocketAddr, source: VectorConfig, sink: VectorSinkConfig) {
        let (tx, rx) = mpsc::channel(100);

        let server = source
            .build(
                "default",
                &GlobalOptions::default(),
                ShutdownSignal::noop(),
                tx,
            )
            .unwrap();
        let mut rt = runtime();
        rt.spawn(server);
        wait_for_tcp(addr);

        let cx = SinkContext::new_test(rt.executor());
        let (sink, _) = sink.build(cx).unwrap();

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

        let _ = rt
            .block_on(sink.send_all(stream::iter_ok(events.clone().into_iter())))
            .unwrap();

        std::thread::sleep(std::time::Duration::from_millis(50));

        let (_, output) = CollectCurrent::new(rx).wait().unwrap();
        assert_eq!(events, output);
    }

    #[test]
    fn it_works_with_vector_sink() {
        let addr = next_addr();
        stream_test(
            addr,
            VectorConfig::new(addr.into(), None),
            VectorSinkConfig {
                address: format!("localhost:{}", addr.port()),
                tls: None,
            },
        );
    }

    #[test]
    fn it_works_with_vector_sink_tls() {
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
        );
    }
}

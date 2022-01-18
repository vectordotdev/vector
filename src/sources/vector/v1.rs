use bytes::Bytes;
use getset::Setters;
use prost::Message;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};

use crate::{
    codecs::{self, decoding::Deserializer, LengthDelimitedDecoder},
    config::{DataType, GenerateConfig, Output, Resource, SourceContext},
    event::{proto, Event},
    internal_events::{VectorEventReceived, VectorProtoDecodeError},
    sources::{
        util::{SocketListenAddr, TcpNullAcker, TcpSource},
        Source,
    },
    tcp::TcpKeepaliveConfig,
    tls::{MaybeTlsSettings, TlsConfig},
};

#[derive(Deserialize, Serialize, Debug, Clone, Setters)]
#[serde(deny_unknown_fields)]
pub struct VectorConfig {
    address: SocketListenAddr,
    keepalive: Option<TcpKeepaliveConfig>,
    #[serde(default = "default_shutdown_timeout_secs")]
    shutdown_timeout_secs: u64,
    #[set = "pub"]
    tls: Option<TlsConfig>,
    receive_buffer_bytes: Option<usize>,
}

const fn default_shutdown_timeout_secs() -> u64 {
    30
}

impl VectorConfig {
    pub const fn from_address(address: SocketListenAddr) -> Self {
        Self {
            address,
            keepalive: None,
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
            tls: None,
            receive_buffer_bytes: None,
        }
    }
}

impl GenerateConfig for VectorConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self::from_address(SocketListenAddr::SocketAddr(
            "0.0.0.0:9000".parse().unwrap(),
        )))
        .unwrap()
    }
}

impl VectorConfig {
    pub(super) async fn build(&self, cx: SourceContext) -> crate::Result<Source> {
        let vector = VectorSource;
        let tls = MaybeTlsSettings::from_config(&self.tls, true)?;
        vector.run(
            self.address,
            self.keepalive,
            self.shutdown_timeout_secs,
            tls,
            self.receive_buffer_bytes,
            cx,
            false.into(),
            None,
        )
    }

    pub(super) fn outputs(&self) -> Vec<Output> {
        vec![Output::default(DataType::Any)]
    }

    pub(super) const fn source_type(&self) -> &'static str {
        "vector"
    }

    pub(super) fn resources(&self) -> Vec<Resource> {
        vec![self.address.into()]
    }
}

#[derive(Debug, Clone)]
struct VectorDeserializer;

impl Deserializer for VectorDeserializer {
    fn parse(&self, bytes: Bytes) -> crate::Result<SmallVec<[Event; 1]>> {
        let byte_size = bytes.len();
        match proto::EventWrapper::decode(bytes).map(Event::from) {
            Ok(event) => {
                emit!(&VectorEventReceived { byte_size });
                Ok(smallvec![event])
            }
            Err(error) => {
                emit!(&VectorProtoDecodeError { error: &error });
                Err(Box::new(error))
            }
        }
    }
}

#[derive(Debug, Clone)]
struct VectorSource;

impl TcpSource for VectorSource {
    type Error = codecs::decoding::Error;
    type Item = SmallVec<[Event; 1]>;
    type Decoder = codecs::Decoder;
    type Acker = TcpNullAcker;

    fn decoder(&self) -> Self::Decoder {
        codecs::Decoder::new(
            Box::new(LengthDelimitedDecoder::new()),
            Box::new(VectorDeserializer),
        )
    }

    fn build_acker(&self, _: &[Self::Item]) -> Self::Acker {
        TcpNullAcker
    }
}

#[cfg(feature = "sinks-vector")]
#[cfg(test)]
mod test {
    use std::net::SocketAddr;

    use shared::assert_event_data_eq;
    use tokio::{
        io::AsyncWriteExt,
        net::TcpStream,
        time::{sleep, Duration},
    };
    #[cfg(not(target_os = "windows"))]
    use {
        crate::event::proto,
        bytes::BytesMut,
        futures::SinkExt,
        prost::Message,
        tokio_util::codec::{FramedWrite, LengthDelimitedCodec},
    };

    use super::VectorConfig;
    use crate::{
        config::{ComponentKey, GlobalOptions, SinkContext, SourceContext},
        event::{
            metric::{MetricKind, MetricValue},
            Event, Metric,
        },
        shutdown::ShutdownSignal,
        sinks::vector::v1::VectorConfig as SinkConfig,
        test_util::{collect_ready, next_addr, trace_init, wait_for_tcp},
        tls::{TlsConfig, TlsOptions},
        SourceSender,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<VectorConfig>();
    }

    async fn stream_test(addr: SocketAddr, source: VectorConfig, sink: SinkConfig) {
        let (tx, rx) = SourceSender::new_test();

        let server = source.build(SourceContext::new_test(tx)).await.unwrap();
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
                MetricKind::Absolute,
                MetricValue::Counter { value: 1.0 },
            )),
        ];

        sink.run_events(events.clone()).await.unwrap();

        sleep(Duration::from_millis(50)).await;

        let output = collect_ready(rx).await;
        assert_event_data_eq!(events, output);
    }

    #[tokio::test]
    async fn it_works_with_vector_sink() {
        let addr = next_addr();
        stream_test(
            addr,
            VectorConfig::from_address(addr.into()),
            SinkConfig::from_address(format!("localhost:{}", addr.port())),
        )
        .await;
    }

    #[tokio::test]
    async fn it_works_with_vector_sink_tls() {
        let addr = next_addr();
        stream_test(
            addr,
            {
                let mut config = VectorConfig::from_address(addr.into());
                config.set_tls(Some(TlsConfig::test_config()));
                config
            },
            {
                let mut config = SinkConfig::from_address(format!("localhost:{}", addr.port()));
                config.set_tls(Some(TlsConfig {
                    enabled: Some(true),
                    options: TlsOptions {
                        verify_certificate: Some(false),
                        ..Default::default()
                    },
                }));
                config
            },
        )
        .await;
    }

    #[tokio::test]
    async fn it_closes_stream_on_garbage_data() {
        trace_init();
        let (tx, rx) = SourceSender::new_test();
        let addr = next_addr();

        let config = VectorConfig::from_address(addr.into());

        let (trigger_shutdown, shutdown, shutdown_down) = ShutdownSignal::new_wired();

        let server = config
            .build(SourceContext {
                key: ComponentKey::from("default"),
                globals: GlobalOptions::default(),
                shutdown,
                out: tx,
                proxy: Default::default(),
            })
            .await
            .unwrap();
        tokio::spawn(server);

        wait_for_tcp(addr).await;

        let mut stream = TcpStream::connect(&addr).await.unwrap();
        stream.write(b"hello world \n").await.unwrap();

        tokio::time::sleep(Duration::from_secs(2)).await;
        stream.shutdown().await.unwrap();
        drop(trigger_shutdown);
        shutdown_down.await;

        let output = collect_ready(rx).await;
        assert_eq!(output, []);
    }

    #[tokio::test]
    #[cfg(not(target_os = "windows"))]
    async fn it_processes_stream_of_protobufs() {
        trace_init();
        let (tx, rx) = SourceSender::new_test();
        let addr = next_addr();

        let config = VectorConfig::from_address(addr.into());

        let (trigger_shutdown, shutdown, shutdown_down) = ShutdownSignal::new_wired();

        let server = config
            .build(SourceContext {
                key: ComponentKey::from("default"),
                globals: GlobalOptions::default(),
                shutdown,
                out: tx,
                proxy: Default::default(),
            })
            .await
            .unwrap();
        tokio::spawn(server);

        let event = proto::EventWrapper::from(Event::from("short"));
        let event_len = event.encoded_len();
        let full_len = event_len + 4;

        let mut out = BytesMut::with_capacity(full_len);
        event.encode(&mut out).unwrap();

        wait_for_tcp(addr).await;

        let stream = TcpStream::connect(&addr).await.unwrap();
        let encoder = LengthDelimitedCodec::new();
        let mut sink = FramedWrite::new(stream, encoder);
        sink.send(out.into()).await.unwrap();

        let mut stream = sink.into_inner();
        tokio::time::sleep(Duration::from_secs(2)).await;
        stream.shutdown().await.unwrap();
        drop(trigger_shutdown);
        shutdown_down.await;

        let output = collect_ready(rx).await;
        assert_event_data_eq!([Event::from(event)][..], output.as_slice());
    }
}

use super::util::{SocketListenAddr, TcpSource};
use crate::{
    event::proto,
    tls::{TlsConfig, TlsSettings},
    topology::config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
    Event,
};
use bytes::{Bytes, BytesMut};
use futures01::sync::mpsc;
use prost::Message;
use serde::{Deserialize, Serialize};
use tokio::codec::LengthDelimitedCodec;
use tracing::field;

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

impl VectorConfig {
    pub fn new(address: SocketListenAddr) -> Self {
        Self {
            address,
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
            tls: None,
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
        out: mpsc::Sender<Event>,
    ) -> crate::Result<super::Source> {
        let vector = VectorSource;
        let tls = TlsSettings::from_config(&self.tls, true)?;
        vector.run(self.address, self.shutdown_timeout_secs, tls, out)
    }

    fn output_type(&self) -> DataType {
        DataType::Log
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

    fn build_event(&self, frame: BytesMut, _host: Option<Bytes>) -> Option<Event> {
        match proto::EventWrapper::decode(frame).map(Event::from) {
            Ok(event) => {
                trace!(
                    message = "Received one event.",
                    event = field::debug(&event)
                );
                Some(event)
            }
            Err(e) => {
                error!("failed to parse protobuf message: {:?}", e);
                None
            }
        }
    }
}

#[cfg(feature = "sinks-vector")]
#[cfg(test)]
mod test {
    use super::VectorConfig;
    use crate::{
        sinks::vector::VectorSinkConfig,
        test_util::{next_addr, wait_for_tcp, CollectCurrent},
        topology::config::{GlobalOptions, SinkConfig, SinkContext, SourceConfig},
        Event,
    };
    use futures01::{stream, sync::mpsc, Future, Sink};

    #[test]
    fn tcp_it_works_with_vector_sink() {
        let (tx, rx) = mpsc::channel(100);

        let addr = next_addr();
        let server = VectorConfig::new(addr.into())
            .build("default", &GlobalOptions::default(), tx)
            .unwrap();
        let mut rt = crate::runtime::Runtime::new().unwrap();
        rt.spawn(server);
        wait_for_tcp(addr);

        let cx = SinkContext::new_test(rt.executor());
        let (sink, _) = VectorSinkConfig {
            address: format!("localhost:{}", addr.port()),
        }
        .build(cx)
        .unwrap();

        let events = vec![
            Event::from("test"),
            Event::from("events"),
            Event::from("to roundtrip"),
            Event::from("through"),
            Event::from("the native"),
            Event::from("sink"),
            Event::from("and"),
            Event::from("source"),
        ];

        let _ = rt
            .block_on(sink.send_all(stream::iter_ok(events.clone().into_iter())))
            .unwrap();

        std::thread::sleep(std::time::Duration::from_millis(50));

        let (_, output) = CollectCurrent::new(rx).wait().unwrap();
        assert_eq!(events, output);
    }
}

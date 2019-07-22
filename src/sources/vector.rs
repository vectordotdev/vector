use super::util::TcpSource;
use crate::{
    event::proto,
    topology::config::{DataType, GlobalOptions, SourceConfig},
    Event,
};
use bytes::{Bytes, BytesMut};
use futures::sync::mpsc;
use prost::Message;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::codec::LengthDelimitedCodec;
use tracing::field;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct VectorConfig {
    pub address: SocketAddr,
    #[serde(default = "default_shutdown_timeout_secs")]
    pub shutdown_timeout_secs: u64,
}

fn default_shutdown_timeout_secs() -> u64 {
    30
}

impl VectorConfig {
    pub fn new(address: SocketAddr) -> Self {
        Self {
            address,
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
        }
    }
}

#[typetag::serde(name = "vector")]
impl SourceConfig for VectorConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        out: mpsc::Sender<Event>,
    ) -> Result<super::Source, String> {
        let vector = VectorSource;
        vector.run(self.address, self.shutdown_timeout_secs, out)
    }

    fn output_type(&self) -> DataType {
        DataType::Log
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

#[cfg(test)]
mod test {
    use super::VectorConfig;
    use crate::{
        buffers::Acker,
        sinks::vector::vector,
        test_util::{next_addr, wait_for_tcp, CollectCurrent},
        topology::config::{GlobalOptions, SourceConfig},
        Event,
    };
    use futures::{stream, sync::mpsc, Future, Sink};

    #[test]
    fn tcp_it_works_with_vector_sink() {
        let (tx, rx) = mpsc::channel(100);

        let addr = next_addr();
        let server = VectorConfig::new(addr.clone())
            .build("default", &GlobalOptions::default(), tx)
            .unwrap();
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        rt.spawn(server);
        wait_for_tcp(addr);

        let sink = vector(addr, Acker::Null);
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

        rt.block_on(sink.send_all(stream::iter_ok(events.clone().into_iter())))
            .unwrap();

        std::thread::sleep(std::time::Duration::from_millis(50));

        let (_, output) = CollectCurrent::new(rx).wait().unwrap();
        assert_eq!(events, output);
    }
}

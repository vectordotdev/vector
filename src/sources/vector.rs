use crate::{event::proto, Event};
use futures::{future, sync::mpsc, Future, Sink, Stream};
use prost::Message;
use serde::{Deserialize, Serialize};
use std::net::{SocketAddr, ToSocketAddrs};
use std::time::{Duration, Instant};
use stream_cancel::{StreamExt, Tripwire};
use tokio::{
    codec::{FramedRead, LengthDelimitedCodec},
    net::TcpListener,
    timer,
};
use tokio_trace::field;
use tokio_trace_futures::Instrument;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct VectorConfig {
    pub address: String,
    #[serde(default = "default_shutdown_timeout_secs")]
    pub shutdown_timeout_secs: u64,
}

fn default_shutdown_timeout_secs() -> u64 {
    30
}

impl VectorConfig {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            address: addr.to_string(),
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
        }
    }
}

#[typetag::serde(name = "vector")]
impl crate::topology::config::SourceConfig for VectorConfig {
    fn build(&self, out: mpsc::Sender<Event>) -> Result<super::Source, String> {
        let vector = vector(self.clone(), out)?;
        Ok(vector)
    }
}

pub fn vector(config: VectorConfig, out: mpsc::Sender<Event>) -> Result<super::Source, String> {
    let out = out.sink_map_err(|e| error!("error sending line: {:?}", e));

    let VectorConfig {
        address,
        shutdown_timeout_secs,
    } = config;

    let addr = address
        .to_socket_addrs()
        .map_err(|e| format!("IO Error: {}", e))?
        .next()
        .ok_or_else(|| "Unable to resolve DNS for provided address".to_string())?;

    let source = future::lazy(move || {
        let listener = match TcpListener::bind(&addr) {
            Ok(listener) => listener,
            Err(err) => {
                error!("Failed to bind to listener socket: {}", err);
                return future::Either::B(future::err(()));
            }
        };

        info!(message = "listening.", addr = field::display(&addr));

        let (trigger, tripwire) = Tripwire::new();
        let tripwire = tripwire
            .and_then(move |_| {
                timer::Delay::new(Instant::now() + Duration::from_secs(shutdown_timeout_secs))
                    .map_err(|err| panic!("Timer error: {:?}", err))
            })
            .shared();

        let future = listener
            .incoming()
            .map_err(|e| error!("failed to accept socket; error = {}", e))
            .for_each(move |socket| {
                let peer_addr = socket.peer_addr().ok().map(|s| s.ip().to_string());

                let span = if let Some(addr) = &peer_addr {
                    info_span!("connection", peer_addr = field::display(addr))
                } else {
                    info_span!("connection")
                };

                let inner_span = span.clone();
                let tripwire = tripwire
                    .clone()
                    .map(move |_| {
                        info!(
                            "Resetting connection (still open after {} seconds).",
                            shutdown_timeout_secs
                        )
                    })
                    .map_err(|_| ());

                span.enter(|| {
                    debug!("accepted a new socket.");

                    let out = out.clone();

                    let lines_in = FramedRead::new(socket, LengthDelimitedCodec::new())
                        .take_until(tripwire)
                        .filter_map(|bytes| match proto::EventWrapper::decode(bytes) {
                            Ok(e) => Some(e),
                            Err(e) => {
                                error!("failed to parse protobuf message: {:?}", e);
                                None
                            }
                        })
                        .map(Event::from)
                        .map_err(|e| warn!("connection error: {:?}", e));

                    let handler = lines_in.forward(out).map(|_| debug!("connection closed"));

                    tokio::spawn(handler.instrument(inner_span))
                })
            })
            .inspect(|_| trigger.cancel());
        future::Either::A(future)
    });

    Ok(Box::new(source))
}

#[cfg(test)]
mod test {
    use super::VectorConfig;
    use crate::{
        buffers::Acker,
        sinks::vector::vector,
        test_util::{next_addr, wait_for_tcp, CollectCurrent},
        Event,
    };
    use futures::{stream, sync::mpsc, Future, Sink};

    #[test]
    fn tcp_it_works_with_vector_sink() {
        let (tx, rx) = mpsc::channel(100);

        let addr = next_addr();
        let server = super::vector(VectorConfig::new(addr.clone()), tx).unwrap();
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

use crate::Event;
use bytes::Bytes;
use futures::{future, sync::mpsc, Future, Sink, Stream};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use stream_cancel::{StreamExt, Tripwire};
use tokio::{codec::FramedRead, net::TcpListener, timer};
use tokio_trace::field;
use tokio_trace_futures::Instrument;

pub trait TcpSource
where
    Self: Clone + Send + 'static,
{
    type Decoder: tokio::codec::Decoder<Error = std::io::Error> + Send + 'static;

    fn decoder(&self) -> Self::Decoder;

    fn build_event(
        &self,
        frame: <Self::Decoder as tokio::codec::Decoder>::Item,
        host: Option<Bytes>,
    ) -> Option<Event>;

    fn run(
        self,
        addr: SocketAddr,
        shutdown_timeout_secs: u64,
        out: mpsc::Sender<Event>,
    ) -> Result<crate::sources::Source, String> {
        let out = out.sink_map_err(|e| error!("error sending event: {:?}", e));

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

                    let host = peer_addr.map(Bytes::from);

                    let tripwire = tripwire
                        .clone()
                        .map(move |_| {
                            info!(
                                "Resetting connection (still open after {} seconds).",
                                shutdown_timeout_secs
                            )
                        })
                        .map_err(|_| ());

                    let clone = self.clone();
                    span.enter(|| {
                        debug!("accepted a new socket.");

                        let out = out.clone();

                        let events_in = FramedRead::new(socket, clone.decoder())
                            .take_until(tripwire)
                            .filter_map(move |frame| {
                                let host = host.clone();
                                clone.build_event(frame, host)
                            })
                            .map_err(|e| warn!("connection error: {:?}", e));

                        let handler = events_in.forward(out).map(|_| debug!("connection closed"));

                        tokio::spawn(handler.instrument(span.clone()))
                    })
                })
                .inspect(|_| trigger.cancel());
            future::Either::A(future)
        });

        Ok(Box::new(source))
    }
}

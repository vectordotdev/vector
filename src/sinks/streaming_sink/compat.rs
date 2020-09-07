use super::StreamingSink;
use crate::{sinks::VectorSink, Event};
use futures::{channel::mpsc, compat::CompatSink, TryFutureExt};
use futures01::{future::Future, sink::Sink, Async, AsyncSink, Poll};

/// This function provides the compatibility with our old interfaces.
///
/// There are several aspects to it:
///
/// 1. We use pull-based interface now, before we used push-based interface.
///    Practically this means we're using `Stream` now instead of `Sink`.
/// 2. We are now using futures 0.3, and before we used futures 0.1.
///
/// This function returns a tuple with two items.
/// The first item implements a `futures::Stream<Item = Event>` and can be used
/// with `StreamingSink`.
/// The second item implements a `futures01::Sink<SinkItem = Event>` and can
/// be used with an old topology.
///
/// Items are processed one at a time without internal buffering to maintain
/// the backpressure across the system.
pub fn sink_interface_compat() -> (NewStream, OldSink) {
    let (tx, rx) = mpsc::channel(0);
    let tx = Box::new(CompatSink::new(tx).sink_map_err(|_| ()));
    (rx, tx)
}

/// Implements `futures::Stream<Item = Event>`.
pub type NewStream = mpsc::Receiver<Event>;

/// Implements `futures01::sink::Sink<SinkItem = Event, SinkError = ()>`.
pub type OldSink = Box<dyn Sink<SinkItem = Event, SinkError = ()> + 'static + Send>;

/// This function takes ownership of a new streaming sink and adapts it for the
/// current topology.
///
/// The idea is that we'll be globally switching the sinks at the topology level
/// to a pull-based interface ([`StreamingSink`]), and that at some point this
/// adapter won't be required.
///
/// Spawns the polling loop at the background and returns
/// a current-topology-compatible sink.
///
/// Among other things, this adapter maintains backpressure through the sink, as
/// it'll only go as fast as `streaming_sink` is able to poll items, without any
/// buffering.
pub fn adapt_to_topology(mut streaming_sink: impl StreamingSink + 'static) -> VectorSink {
    let (stream, sink) = sink_interface_compat();

    let handle = tokio::spawn(async move {
        streaming_sink
            .run(stream)
            .await
            .expect("streaming sink error")
    });

    let synched = SynchedSink {
        sink,
        synching: false,
        sync: Box::new(handle.compat().map_err(|res| {
            if res.is_panic() {
                // We are propagating panic as if this spawn indirection isn't here.
                panic!(res);
            }
        })),
    };

    VectorSink::Futures01Sink(Box::new(synched))
}

/// Behaves in all regards like the underlying sink, except that on close
/// it synchronizes on sync.
struct SynchedSink {
    sink: OldSink,
    sync: Box<dyn Future<Item = (), Error = ()> + 'static + Send>,
    synching: bool,
}

impl Sink for SynchedSink {
    type SinkItem = Event;
    type SinkError = ();
    fn start_send(&mut self, item: Self::SinkItem) -> Result<AsyncSink<Event>, ()> {
        self.sink.start_send(item)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        self.sink.poll_complete()
    }

    fn close(&mut self) -> Poll<(), Self::SinkError> {
        if !self.synching {
            if let Async::NotReady = self.sink.close()? {
                return Ok(Async::NotReady);
            }
            self.synching = true;
        }
        self.sync.poll()
    }
}

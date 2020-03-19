use super::StreamingSink;
use crate::sinks;
use crate::topology::config::SinkContext;
use crate::Event;
use futures::channel::mpsc;
use futures::compat::CompatSink;
use futures01::sink::Sink;

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
pub fn adapt_to_topology(
    cx: &mut SinkContext,
    mut streaming_sink: impl StreamingSink + 'static,
) -> sinks::RouterSink {
    let (stream, sink) = sink_interface_compat();

    cx.executor().spawn_std(async move {
        streaming_sink
            .run(stream)
            .await
            .expect("streaming sink error")
    });

    sink
}

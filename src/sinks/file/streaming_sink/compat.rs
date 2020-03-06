use super::StreamingSink;
use crate::sinks;
use crate::topology::config::SinkContext;
use crate::Event;
use futures::channel::mpsc;
use futures::compat::CompatSink;

/// This function provides the compatiblity with our old interfaces.
///
/// There are serveral aspects to it:
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
    (rx, UnitTypeErrorSink01(CompatSink::new(tx)))
}

/// Implements `futures::Stream<Item = Event>`.
pub type NewStream = mpsc::Receiver<Event>;

/// Implements `futures01::Sink<SinkItem = Event>`.
pub type OldSink = UnitTypeErrorSink01<CompatSink<mpsc::Sender<Event>, Event>>;

/// This function takes ownership of a new streaming sink and adapts it for the
/// current topology.
///
/// The idea is that we'll be globally switching the sinks at the topology level
/// to a pull-based interface ([`StreamingSink`]), and that at some point this
/// adapter won't be required.
///
/// Spawns the polling loop at the background and returns
/// a current-topolgy-compatible sink.
///
/// Among other things, this adapter maintains backpressure through the sink, as
/// it'll only go as fast as `streaming_sink` is able to poll items, without any
/// buffering.
pub fn adapt_to_topology(
    _cx: &mut SinkContext,
    mut streaming_sink: impl StreamingSink + 'static,
) -> sinks::RouterSink {
    let (stream, sink) = sink_interface_compat();

    // TODO: use the runtime from the passed `SinkContext` when it's added
    // there.
    tokio02::spawn(async move {
        streaming_sink
            .run(stream)
            .await
            .expect("streaming sink error")
    });

    Box::new(sink)
}

/// Wraps any [`futures01::Sink`] with `SinkError = ()`.
pub struct UnitTypeErrorSink01<T: futures01::Sink>(T);

impl<T: futures01::Sink> futures01::Sink for UnitTypeErrorSink01<T> {
    type SinkItem = <T as futures01::Sink>::SinkItem;
    type SinkError = ();

    fn start_send(
        &mut self,
        item: Self::SinkItem,
    ) -> futures01::StartSend<Self::SinkItem, Self::SinkError> {
        self.0.start_send(item).map_err(|_| ())
    }

    fn poll_complete(&mut self) -> futures01::Poll<(), Self::SinkError> {
        self.0.poll_complete().map_err(|_| ())
    }
}

use std::sync::{atomic::AtomicUsize, Arc};

use futures_util::Stream;
use stream_cancel::Trigger;
use vector_core::event::EventArray;

use crate::{sources::Sources, SourceSender};

use self::{
    sinks::{BasicSinkConfig, ErrorSinkConfig, PanicSinkConfig},
    sources::{
        BackpressureSourceConfig, BasicSourceConfig, ErrorSourceConfig, PanicSourceConfig,
        TripwireSourceConfig,
    },
    transforms::BasicTransformConfig,
};

pub mod sinks;
pub mod sources;
pub mod transforms;

pub fn backpressure_source(counter: &Arc<AtomicUsize>) -> Sources {
    Sources::TestBackpressure(BackpressureSourceConfig {
        counter: Arc::clone(counter),
    })
}

pub fn basic_source() -> (SourceSender, Sources) {
    let (tx, rx) = SourceSender::new_with_buffer(1);
    let source = Sources::TestBasic(BasicSourceConfig::new(rx));
    (tx, source)
}

pub fn basic_source_with_data(data: &str) -> (SourceSender, Sources) {
    let (tx, rx) = SourceSender::new_with_buffer(1);
    let source = Sources::TestBasic(BasicSourceConfig::new_with_data(rx, data));
    (tx, source)
}

pub fn basic_source_with_event_counter(
    force_shutdown: bool,
) -> (SourceSender, Sources, Arc<AtomicUsize>) {
    let event_counter = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = SourceSender::new_with_buffer(1);
    let mut source = BasicSourceConfig::new_with_event_counter(rx, Arc::clone(&event_counter));
    source.set_force_shutdown(force_shutdown);

    (tx, Sources::TestBasic(source), event_counter)
}

pub fn error_source() -> Sources {
    Sources::TestError(ErrorSourceConfig::default())
}

pub fn panic_source() -> Sources {
    Sources::TestPanic(PanicSourceConfig::default())
}

pub fn tripwire_source() -> (Trigger, Sources) {
    let (trigger, source) = TripwireSourceConfig::new();
    (trigger, Sources::TestTripwire(source))
}

pub fn basic_transform(suffix: &str, increase: f64) -> BasicTransformConfig {
    BasicTransformConfig::new(suffix.to_owned(), increase)
}

pub fn basic_sink(channel_size: usize) -> (impl Stream<Item = EventArray>, BasicSinkConfig) {
    let (tx, rx) = SourceSender::new_with_buffer(channel_size);
    let sink = BasicSinkConfig::new(tx, true);
    (rx.into_stream(), sink)
}

pub fn basic_sink_with_data(
    channel_size: usize,
    data: &str,
) -> (impl Stream<Item = EventArray>, BasicSinkConfig) {
    let (tx, rx) = SourceSender::new_with_buffer(channel_size);
    let sink = BasicSinkConfig::new_with_data(tx, true, data);
    (rx.into_stream(), sink)
}

pub fn basic_sink_failing_healthcheck(
    channel_size: usize,
) -> (impl Stream<Item = EventArray>, BasicSinkConfig) {
    let (tx, rx) = SourceSender::new_with_buffer(channel_size);
    let sink = BasicSinkConfig::new(tx, false);
    (rx.into_stream(), sink)
}

pub fn error_sink() -> ErrorSinkConfig {
    ErrorSinkConfig::default()
}

pub fn panic_sink() -> PanicSinkConfig {
    PanicSinkConfig::default()
}

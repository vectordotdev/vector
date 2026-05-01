use std::sync::{Arc, atomic::AtomicUsize};

use futures_util::Stream;
use stream_cancel::Trigger;
use tokio::sync::oneshot::Sender;
use vector_lib::{
    event::EventArray,
    source_sender::{SourceSender, SourceSenderItem},
};

use self::{
    sinks::{
        BackpressureSinkConfig, BasicSinkConfig, ErrorSinkConfig, NoAckSinkConfig,
        OneshotSinkConfig, PanicSinkConfig,
    },
    sources::{
        AckSourceConfig, BackpressureSourceConfig, BasicSourceConfig, ErrorSourceConfig,
        PanicSourceConfig, TripwireSourceConfig,
    },
    transforms::{BasicTransformConfig, ErrorDefinitionTransformConfig},
};

pub mod sinks;
pub mod sources;
pub mod transforms;

pub fn backpressure_source(counter: &Arc<AtomicUsize>) -> BackpressureSourceConfig {
    BackpressureSourceConfig {
        counter: Arc::clone(counter),
    }
}

/// Create an ack-aware source (`can_acknowledge() -> true`).
///
/// The returned `SourceSender` is used to inject events into the source.
/// Attach `BatchNotifier` to events before sending them to observe
/// end-to-end acknowledgement behavior through the topology.
pub fn ack_source() -> (SourceSender, AckSourceConfig) {
    let (tx, rx) = SourceSender::new_test_sender_with_options(1, None);
    (tx, AckSourceConfig::new(rx))
}

pub fn basic_source() -> (SourceSender, BasicSourceConfig) {
    let (tx, rx) = SourceSender::new_test_sender_with_options(1, None);
    (tx, BasicSourceConfig::new(rx))
}

pub fn basic_source_with_data(data: &str) -> (SourceSender, BasicSourceConfig) {
    let (tx, rx) = SourceSender::new_test_sender_with_options(1, None);
    (tx, BasicSourceConfig::new_with_data(rx, data))
}

pub fn basic_source_with_event_counter(
    force_shutdown: bool,
) -> (SourceSender, BasicSourceConfig, Arc<AtomicUsize>) {
    let event_counter = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = SourceSender::new_test_sender_with_options(1, None);
    let mut source = BasicSourceConfig::new_with_event_counter(rx, Arc::clone(&event_counter));
    source.set_force_shutdown(force_shutdown);

    (tx, source, event_counter)
}

pub fn error_source() -> ErrorSourceConfig {
    ErrorSourceConfig::default()
}

pub fn panic_source() -> PanicSourceConfig {
    PanicSourceConfig::default()
}

pub fn tripwire_source() -> (Trigger, TripwireSourceConfig) {
    TripwireSourceConfig::new()
}

pub fn basic_transform(suffix: &str, increase: f64) -> BasicTransformConfig {
    BasicTransformConfig::new(suffix.to_owned(), increase)
}

pub const fn error_definition_transform() -> ErrorDefinitionTransformConfig {
    ErrorDefinitionTransformConfig {}
}

pub const fn backpressure_sink(num_to_consume: usize) -> BackpressureSinkConfig {
    BackpressureSinkConfig { num_to_consume }
}

pub fn basic_sink(channel_size: usize) -> (impl Stream<Item = SourceSenderItem>, BasicSinkConfig) {
    let (tx, rx) = SourceSender::new_test_sender_with_options(channel_size, None);
    let sink = BasicSinkConfig::new(tx, true);
    (rx.into_stream(), sink)
}

/// Create a basic sink with a custom acknowledgements configuration.
#[cfg(test)]
pub fn basic_sink_with_acks(
    channel_size: usize,
    acks: vector_lib::config::AcknowledgementsConfig,
) -> (impl Stream<Item = SourceSenderItem>, BasicSinkConfig) {
    let (tx, rx) = SourceSender::new_test_sender_with_options(channel_size, None);
    let sink = BasicSinkConfig::new(tx, true).with_acknowledgements(acks);
    (rx.into_stream(), sink)
}

pub fn basic_sink_with_data(
    channel_size: usize,
    data: &str,
) -> (
    impl Stream<Item = SourceSenderItem> + use<>,
    BasicSinkConfig,
) {
    let (tx, rx) = SourceSender::new_test_sender_with_options(channel_size, None);
    let sink = BasicSinkConfig::new_with_data(tx, true, data);
    (rx.into_stream(), sink)
}

pub fn basic_sink_failing_healthcheck(
    channel_size: usize,
) -> (impl Stream<Item = SourceSenderItem>, BasicSinkConfig) {
    let (tx, rx) = SourceSender::new_test_sender_with_options(channel_size, None);
    let sink = BasicSinkConfig::new(tx, false);
    (rx.into_stream(), sink)
}

/// Create a sink that holds finalizers indefinitely, preventing ack delivery.
///
/// Returns the config, a receiver that fires when the first event is received,
/// and the shared held-finalizers storage (drop to release acks).
#[cfg(test)]
pub fn no_ack_sink(
    acks: vector_lib::config::AcknowledgementsConfig,
) -> (
    NoAckSinkConfig,
    tokio::sync::oneshot::Receiver<()>,
    std::sync::Arc<std::sync::Mutex<Vec<vector_lib::finalization::EventFinalizers>>>,
) {
    NoAckSinkConfig::new(acks)
}

pub fn error_sink() -> ErrorSinkConfig {
    ErrorSinkConfig::default()
}

pub fn oneshot_sink(tx: Sender<EventArray>) -> OneshotSinkConfig {
    OneshotSinkConfig::new(tx)
}

pub fn panic_sink() -> PanicSinkConfig {
    PanicSinkConfig::default()
}

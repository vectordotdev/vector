use std::collections::HashMap;

use bytes::BytesMut;
use serde::Deserialize;
use serde_json::Value;
use snafu::Snafu;
use tokio_util::codec::Encoder as _;
use vector_lib::codecs::encoding::format::JsonSerializerOptions;

use crate::codecs::Encoder;
use vector_lib::codecs::{
    encoding, JsonSerializer, LengthDelimitedEncoder, LogfmtSerializer, MetricTagValues,
    NewlineDelimitedEncoder,
};
use vector_lib::event::{Event, LogEvent};

/// A test case event for deserialization from yaml file.
/// This is an intermediary step to TestEvent.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum RawTestEvent {
    /// The event is used, as-is, without modification.
    Passthrough(EventData),

    /// The event is potentially modified by the external resource.
    ///
    /// The modification made is dependent on the external resource, but this mode is made available
    /// for when a test case wants to exercise the failure path, but cannot cause a failure simply
    /// by constructing the event in a certain way i.e. adding an invalid field, or removing a
    /// required field, or using an invalid field value, and so on.
    ///
    /// For transforms and sinks, generally, the only way to cause an error is if the event itself
    /// is malformed in some way, which can be achieved without this test event variant.
    AlternateEncoder { fail_encoding_of: EventData },

    /// The event will be rejected by the external resource.
    ResourceReject {
        external_resource_rejects: EventData,
    },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventData {
    /// A simple log event.
    Log(String),
    /// A log event built from key-value pairs
    LogBuilder(HashMap<String, Value>),
}

impl EventData {
    /// Converts this event data into an `Event`.
    pub fn into_event(self) -> Event {
        match self {
            Self::Log(message) => Event::Log(LogEvent::from_bytes_legacy(&message.into())),
            Self::LogBuilder(data) => {
                let mut log_event = LogEvent::default();
                for (k, v) in data {
                    log_event
                        .parse_path_and_insert(&k, v)
                        .unwrap_or_else(|_| panic!("Unable to build log event for {}", &k));
                }
                Event::Log(log_event)
            }
        }
    }
}

/// An event used in a test case.
/// It is important to have created the event with all fields, immediately after deserializing from the
/// test case definition yaml file. This ensures that the event data we are using in the expected/actual
/// metrics collection is based on the same event. Namely, one issue that can arise from creating the event
/// from the event data twice (once for the expected and once for actual), it can result in a timestamp in
/// the event which may or may not have the same millisecond precision as it's counterpart.
///
/// For transforms and sinks, generally, the only way to cause an error is if the event itself
/// is malformed in some way, which can be achieved without this test event variant.
#[derive(Clone, Debug, Deserialize)]
#[serde(from = "RawTestEvent")]
#[serde(untagged)]
pub enum TestEvent {
    /// The event is used, as-is, without modification.
    Passthrough(Event),

    /// The event is encoded using an encoding that differs from the component's
    /// configured encoding, which should cause an error when the event is decoded.
    FailWithAlternateEncoder(Event),

    /// The event encodes successfully but when the external resource receives that event, it should
    /// throw a failure.
    FailWithExternalResource(Event),
}

impl TestEvent {
    #[allow(clippy::missing_const_for_fn)] // const cannot run destructor
    pub fn into_event(self) -> Event {
        match self {
            Self::Passthrough(event) => event,
            Self::FailWithAlternateEncoder(event) => event,
            Self::FailWithExternalResource(event) => event,
        }
    }

    pub fn get_event(&mut self) -> &mut Event {
        match self {
            Self::Passthrough(event) => event,
            Self::FailWithAlternateEncoder(event) => event,
            Self::FailWithExternalResource(event) => event,
        }
    }

    /// (should_fail, event)
    pub fn get(self) -> (bool, Event) {
        match self {
            Self::Passthrough(event) => (false, event),
            Self::FailWithAlternateEncoder(event) => (true, event),
            Self::FailWithExternalResource(event) => (true, event),
        }
    }

    /// True if the event should fail, false otherwise.
    pub const fn should_fail(&self) -> bool {
        match self {
            Self::Passthrough(_) => false,
            Self::FailWithAlternateEncoder(_) | Self::FailWithExternalResource(_) => true,
        }
    }

    /// True if the event should be rejected by the external resource in order to
    /// trigger a failure path.
    pub const fn should_reject(&self) -> bool {
        match self {
            Self::Passthrough(_) | Self::FailWithAlternateEncoder(_) => false,
            Self::FailWithExternalResource(_) => true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Snafu)]
pub enum RawTestEventParseError {}

impl From<RawTestEvent> for TestEvent {
    fn from(other: RawTestEvent) -> Self {
        match other {
            RawTestEvent::Passthrough(event_data) => {
                TestEvent::Passthrough(event_data.into_event())
            }
            RawTestEvent::AlternateEncoder {
                fail_encoding_of: event_data,
            } => TestEvent::FailWithAlternateEncoder(event_data.into_event()),
            RawTestEvent::ResourceReject {
                external_resource_rejects: event_data,
            } => TestEvent::FailWithExternalResource(event_data.into_event()),
        }
    }
}

pub fn encode_test_event(
    encoder: &mut Encoder<encoding::Framer>,
    buf: &mut BytesMut,
    event: TestEvent,
) {
    match event {
        TestEvent::Passthrough(event) | TestEvent::FailWithExternalResource(event) => {
            // Encode the event normally.
            encoder
                .encode(event, buf)
                .expect("should not fail to encode input event");
        }
        TestEvent::FailWithAlternateEncoder(event) => {
            // This is a little fragile, but we check what serializer this encoder uses, and based
            // on `Serializer::supports_json`, we choose an opposing codec. For example, if the
            // encoder supports JSON, we'll use a serializer that doesn't support JSON, and vise
            // versa.
            let mut alt_encoder = if encoder.serializer().supports_json() {
                Encoder::<encoding::Framer>::new(
                    LengthDelimitedEncoder::default().into(),
                    LogfmtSerializer.into(),
                )
            } else {
                Encoder::<encoding::Framer>::new(
                    NewlineDelimitedEncoder::default().into(),
                    JsonSerializer::new(
                        MetricTagValues::default(),
                        JsonSerializerOptions::default(),
                    )
                    .into(),
                )
            };

            alt_encoder
                .encode(event, buf)
                .expect("should not fail to encode input event");
        }
    }
}

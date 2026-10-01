//! Closed-with-escape-hatch enums for kind, status, and sampling priority.

use vector_common::byte_size_of::ByteSizeOf;

mod unknown {
    /// OTLP span-kind wire number outside the known set.
    ///
    /// Only [`super::SpanKind::from_i32`] constructs this, and only for integers that are
    /// not a named kind.
    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    pub struct UnknownSpanKind(i32);

    impl UnknownSpanKind {
        pub(super) const fn new(value: i32) -> Option<Self> {
            match value {
                super::SpanKind::UNSPECIFIED
                | super::SpanKind::INTERNAL
                | super::SpanKind::SERVER
                | super::SpanKind::CLIENT
                | super::SpanKind::PRODUCER
                | super::SpanKind::CONSUMER => None,
                value => Some(Self(value)),
            }
        }

        /// Returns the stored wire integer.
        #[must_use]
        pub const fn get(self) -> i32 {
            self.0
        }
    }

    /// OTLP status code outside the known set, with the status message that accompanied it.
    ///
    /// Only [`super::SpanStatus::from_i32`] constructs this, and only for codes other than
    /// unset, ok, and error.
    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    pub struct UnknownSpanStatus {
        code: i32,
        message: String,
    }

    impl UnknownSpanStatus {
        pub(super) fn new(code: i32, message: String) -> Option<Self> {
            match code {
                super::SpanStatus::UNSET | super::SpanStatus::OK | super::SpanStatus::ERROR => None,
                code => Some(Self { code, message }),
            }
        }

        /// Returns the stored status-code integer.
        #[must_use]
        pub const fn code(&self) -> i32 {
            self.code
        }

        /// Returns the status message stored with this unrecognized code.
        #[must_use]
        pub const fn message(&self) -> &str {
            self.message.as_str()
        }
    }

    /// Datadog sampling-priority wire number outside the known set.
    ///
    /// Only [`super::SamplingPriority::from_i32`] constructs this, and only for integers
    /// other than `-1`, `0`, `1`, and `2`.
    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    pub struct UnknownSamplingPriority(i32);

    impl UnknownSamplingPriority {
        pub(super) const fn new(value: i32) -> Option<Self> {
            match value {
                super::SamplingPriority::USER_REJECT
                | super::SamplingPriority::AUTO_REJECT
                | super::SamplingPriority::AUTO_KEEP
                | super::SamplingPriority::USER_KEEP => None,
                value => Some(Self(value)),
            }
        }

        /// Returns the stored wire integer.
        #[must_use]
        pub const fn get(self) -> i32 {
            self.0
        }
    }
}

pub use unknown::{UnknownSamplingPriority, UnknownSpanKind, UnknownSpanStatus};

/// OpenTelemetry span kind, including unknown future wire values.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum SpanKind {
    /// OTLP `SPAN_KIND_UNSPECIFIED`.
    #[default]
    Unspecified,
    /// OTLP `SPAN_KIND_INTERNAL`.
    Internal,
    /// OTLP `SPAN_KIND_SERVER`.
    Server,
    /// OTLP `SPAN_KIND_CLIENT`.
    Client,
    /// OTLP `SPAN_KIND_PRODUCER`.
    Producer,
    /// OTLP `SPAN_KIND_CONSUMER`.
    Consumer,
    /// Unrecognized enum number from a newer OpenTelemetry version.
    Other(UnknownSpanKind),
}

impl SpanKind {
    /// OTLP `SPAN_KIND_UNSPECIFIED`.
    pub const UNSPECIFIED: i32 = 0;
    /// OTLP `SPAN_KIND_INTERNAL`.
    pub const INTERNAL: i32 = 1;
    /// OTLP `SPAN_KIND_SERVER`.
    pub const SERVER: i32 = 2;
    /// OTLP `SPAN_KIND_CLIENT`.
    pub const CLIENT: i32 = 3;
    /// OTLP `SPAN_KIND_PRODUCER`.
    pub const PRODUCER: i32 = 4;
    /// OTLP `SPAN_KIND_CONSUMER`.
    pub const CONSUMER: i32 = 5;

    /// Normalizes a raw wire integer into a named variant when it matches a known value.
    ///
    /// # Panics
    ///
    /// Panics if a known wire number is not mapped to a named variant. That is an internal bug.
    #[must_use]
    pub const fn from_i32(value: i32) -> Self {
        match value {
            Self::UNSPECIFIED => Self::Unspecified,
            Self::INTERNAL => Self::Internal,
            Self::SERVER => Self::Server,
            Self::CLIENT => Self::Client,
            Self::PRODUCER => Self::Producer,
            Self::CONSUMER => Self::Consumer,
            other => Self::Other(
                UnknownSpanKind::new(other)
                    .expect("known wire number was not mapped to a named variant"),
            ),
        }
    }

    /// Returns the OTLP wire integer for this kind.
    #[must_use]
    pub const fn to_i32(self) -> i32 {
        match self {
            Self::Unspecified => Self::UNSPECIFIED,
            Self::Internal => Self::INTERNAL,
            Self::Server => Self::SERVER,
            Self::Client => Self::CLIENT,
            Self::Producer => Self::PRODUCER,
            Self::Consumer => Self::CONSUMER,
            Self::Other(value) => value.get(),
        }
    }
}

impl From<i32> for SpanKind {
    fn from(value: i32) -> Self {
        Self::from_i32(value)
    }
}

impl ByteSizeOf for SpanKind {
    fn allocated_bytes(&self) -> usize {
        0
    }
}

/// OpenTelemetry span status, including unknown future codes.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub enum SpanStatus {
    /// OTLP `STATUS_CODE_UNSET`.
    #[default]
    Unset,
    /// OTLP `STATUS_CODE_OK`.
    Ok,
    /// OTLP `STATUS_CODE_ERROR`, with an optional message (`Error("")` is representable).
    Error(String),
    /// Unrecognized status code from a newer OpenTelemetry version.
    Other(UnknownSpanStatus),
}

impl SpanStatus {
    /// OTLP `STATUS_CODE_UNSET`.
    pub const UNSET: i32 = 0;
    /// OTLP `STATUS_CODE_OK`.
    pub const OK: i32 = 1;
    /// OTLP `STATUS_CODE_ERROR`.
    pub const ERROR: i32 = 2;

    /// Normalizes a raw status code and message.
    ///
    /// A known code becomes the named variant. Unset and ok drop `message`. Error and
    /// unrecognized codes keep it.
    ///
    /// # Panics
    ///
    /// Panics if a known status code is not mapped to a named variant. That is an internal bug.
    #[must_use]
    pub fn from_i32(code: i32, message: impl Into<String>) -> Self {
        match code {
            Self::UNSET => Self::Unset,
            Self::OK => Self::Ok,
            Self::ERROR => Self::Error(message.into()),
            other => Self::Other(
                UnknownSpanStatus::new(other, message.into())
                    .expect("known wire number was not mapped to a named variant"),
            ),
        }
    }

    /// Returns the OTLP status code integer.
    #[must_use]
    pub fn to_i32(&self) -> i32 {
        match self {
            Self::Unset => Self::UNSET,
            Self::Ok => Self::OK,
            Self::Error(_) => Self::ERROR,
            Self::Other(status) => status.code(),
        }
    }

    /// Status message for `Error` and `Other`; empty for `Unset` and `Ok`.
    #[must_use]
    pub fn message(&self) -> &str {
        match self {
            Self::Unset | Self::Ok => "",
            Self::Error(message) => message,
            Self::Other(status) => status.message(),
        }
    }
}

impl ByteSizeOf for SpanStatus {
    fn allocated_bytes(&self) -> usize {
        match self {
            Self::Unset | Self::Ok => 0,
            Self::Error(message) => message.len(),
            Self::Other(status) => status.message().len(),
        }
    }
}

/// Datadog sampling priority, including out-of-range library values.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SamplingPriority {
    /// `-1`
    UserReject,
    /// `0`
    AutoReject,
    /// `1`
    AutoKeep,
    /// `2`
    UserKeep,
    /// Out-of-range value emitted by some Datadog tracing libraries.
    Other(UnknownSamplingPriority),
}

impl SamplingPriority {
    /// Datadog `USER_REJECT`.
    pub const USER_REJECT: i32 = -1;
    /// Datadog `AUTO_REJECT`.
    pub const AUTO_REJECT: i32 = 0;
    /// Datadog `AUTO_KEEP`.
    pub const AUTO_KEEP: i32 = 1;
    /// Datadog `USER_KEEP`.
    pub const USER_KEEP: i32 = 2;

    /// Normalizes a raw priority integer into a named variant when it matches a known value.
    ///
    /// # Panics
    ///
    /// Panics if a known wire number is not mapped to a named variant. That is an internal bug.
    #[must_use]
    pub const fn from_i32(value: i32) -> Self {
        match value {
            Self::USER_REJECT => Self::UserReject,
            Self::AUTO_REJECT => Self::AutoReject,
            Self::AUTO_KEEP => Self::AutoKeep,
            Self::USER_KEEP => Self::UserKeep,
            other => Self::Other(
                UnknownSamplingPriority::new(other)
                    .expect("known wire number was not mapped to a named variant"),
            ),
        }
    }

    /// Returns the Datadog wire integer for this priority.
    #[must_use]
    pub const fn to_i32(self) -> i32 {
        match self {
            Self::UserReject => Self::USER_REJECT,
            Self::AutoReject => Self::AUTO_REJECT,
            Self::AutoKeep => Self::AUTO_KEEP,
            Self::UserKeep => Self::USER_KEEP,
            Self::Other(value) => value.get(),
        }
    }
}

impl From<i32> for SamplingPriority {
    fn from(value: i32) -> Self {
        Self::from_i32(value)
    }
}

impl ByteSizeOf for SamplingPriority {
    fn allocated_bytes(&self) -> usize {
        0
    }
}

#[cfg(test)]
mod tests {
    use similar_asserts::assert_eq;

    use super::{SamplingPriority, SpanKind, SpanStatus};

    #[test]
    fn from_i32_normalizes_known_other_values() {
        assert_eq!(SpanKind::from_i32(3), SpanKind::Client);
        assert_eq!(SpanKind::from(SpanKind::CLIENT), SpanKind::Client);
        assert_eq!(SpanKind::Client.to_i32(), 3);
        let unknown_kind = SpanKind::from_i32(99);
        assert!(matches!(unknown_kind, SpanKind::Other(kind) if kind.get() == 99));
        assert_eq!(unknown_kind.to_i32(), 99);
        assert_eq!(SpanKind::from_i32(-1).to_i32(), -1);

        assert_eq!(
            SpanStatus::from_i32(2, "boom"),
            SpanStatus::Error("boom".into())
        );
        assert_eq!(
            SpanStatus::from_i32(2, ""),
            SpanStatus::Error(String::new())
        );
        assert_eq!(SpanStatus::Error("boom".into()).message(), "boom");
        let unknown_status = SpanStatus::from_i32(9, "x");
        assert!(matches!(
            &unknown_status,
            SpanStatus::Other(status) if status.code() == 9 && status.message() == "x"
        ));
        assert_eq!(unknown_status.to_i32(), 9);
        assert_eq!(unknown_status.message(), "x");
        assert_eq!(SpanStatus::from_i32(0, "dropped"), SpanStatus::Unset);
        assert_eq!(SpanStatus::from_i32(1, "dropped").message(), "");

        assert_eq!(SamplingPriority::from_i32(1), SamplingPriority::AutoKeep);
        assert_eq!(SamplingPriority::AutoKeep.to_i32(), 1);
        assert_eq!(SamplingPriority::UserReject.to_i32(), -1);
        let unknown_priority = SamplingPriority::from_i32(7);
        assert!(
            matches!(unknown_priority, SamplingPriority::Other(priority) if priority.get() == 7)
        );
        assert_eq!(unknown_priority.to_i32(), 7);
        assert_eq!(SamplingPriority::from_i32(-2).to_i32(), -2);
    }
}

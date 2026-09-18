//! Closed-with-escape-hatch enums for kind, status, and sampling priority.

use vector_common::byte_size_of::ByteSizeOf;

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
    Other(i32),
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
    #[must_use]
    pub const fn from_i32(value: i32) -> Self {
        match value {
            Self::UNSPECIFIED => Self::Unspecified,
            Self::INTERNAL => Self::Internal,
            Self::SERVER => Self::Server,
            Self::CLIENT => Self::Client,
            Self::PRODUCER => Self::Producer,
            Self::CONSUMER => Self::Consumer,
            other => Self::Other(other),
        }
    }

    /// Returns the OTLP wire integer for this kind.
    ///
    /// `Other(known)` is rewritten to the named variant before projection, so
    /// `SpanKind::Other(3).to_i32()` matches [`Self::Client`].
    #[must_use]
    pub const fn to_i32(self) -> i32 {
        match self.normalize() {
            Self::Unspecified => Self::UNSPECIFIED,
            Self::Internal => Self::INTERNAL,
            Self::Server => Self::SERVER,
            Self::Client => Self::CLIENT,
            Self::Producer => Self::PRODUCER,
            Self::Consumer => Self::CONSUMER,
            Self::Other(value) => value,
        }
    }

    /// Rewrites `Other(known)` into the corresponding named variant.
    #[must_use]
    pub const fn normalize(self) -> Self {
        match self {
            Self::Other(value) => Self::from_i32(value),
            other => other,
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
    Other(i32, String),
}

impl SpanStatus {
    /// OTLP `STATUS_CODE_UNSET`.
    pub const UNSET: i32 = 0;
    /// OTLP `STATUS_CODE_OK`.
    pub const OK: i32 = 1;
    /// OTLP `STATUS_CODE_ERROR`.
    pub const ERROR: i32 = 2;

    /// Normalizes a raw status code and message.
    #[must_use]
    pub fn from_i32(code: i32, message: impl Into<String>) -> Self {
        match code {
            Self::UNSET => Self::Unset,
            Self::OK => Self::Ok,
            Self::ERROR => Self::Error(message.into()),
            other => Self::Other(other, message.into()),
        }
    }

    /// Returns the OTLP status code integer.
    ///
    /// `Other(known, _)` is rewritten to the named variant before projection, so
    /// `SpanStatus::Other(2, _).to_i32()` matches [`Self::Error`].
    #[must_use]
    pub fn to_i32(&self) -> i32 {
        match self {
            Self::Unset => Self::UNSET,
            Self::Ok => Self::OK,
            Self::Error(_) => Self::ERROR,
            Self::Other(code, _) => match *code {
                Self::UNSET => Self::UNSET,
                Self::OK => Self::OK,
                Self::ERROR => Self::ERROR,
                other => other,
            },
        }
    }

    /// Status message for `Error` and `Other`; empty for `Unset` and `Ok`.
    #[must_use]
    pub fn message(&self) -> &str {
        match self {
            Self::Unset | Self::Ok => "",
            Self::Error(message) | Self::Other(_, message) => message,
        }
    }

    /// Rewrites `Other(known, _)` into the corresponding named variant.
    #[must_use]
    pub fn normalize(self) -> Self {
        match self {
            Self::Other(code, message) => Self::from_i32(code, message),
            other => other,
        }
    }
}

impl ByteSizeOf for SpanStatus {
    fn allocated_bytes(&self) -> usize {
        match self {
            Self::Unset | Self::Ok => 0,
            Self::Error(message) | Self::Other(_, message) => message.len(),
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
    Other(i32),
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
    #[must_use]
    pub const fn from_i32(value: i32) -> Self {
        match value {
            Self::USER_REJECT => Self::UserReject,
            Self::AUTO_REJECT => Self::AutoReject,
            Self::AUTO_KEEP => Self::AutoKeep,
            Self::USER_KEEP => Self::UserKeep,
            other => Self::Other(other),
        }
    }

    /// Returns the Datadog wire integer for this priority.
    ///
    /// `Other(known)` is rewritten to the named variant before projection, so
    /// `SamplingPriority::Other(1).to_i32()` matches [`Self::AutoKeep`].
    #[must_use]
    pub const fn to_i32(self) -> i32 {
        match self.normalize() {
            Self::UserReject => Self::USER_REJECT,
            Self::AutoReject => Self::AUTO_REJECT,
            Self::AutoKeep => Self::AUTO_KEEP,
            Self::UserKeep => Self::USER_KEEP,
            Self::Other(value) => value,
        }
    }

    /// Rewrites `Other(known)` into the corresponding named variant.
    #[must_use]
    pub const fn normalize(self) -> Self {
        match self {
            Self::Other(value) => Self::from_i32(value),
            other => other,
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
        assert_eq!(SpanKind::from_i32(99), SpanKind::Other(99));
        assert_eq!(SpanKind::Other(3).normalize(), SpanKind::Client);
        assert_eq!(SpanKind::Client.to_i32(), 3);
        assert_eq!(SpanKind::Other(3).to_i32(), SpanKind::Client.to_i32());
        assert_eq!(SpanKind::Other(99).to_i32(), 99);

        assert_eq!(
            SpanStatus::from_i32(2, "boom"),
            SpanStatus::Error("boom".into())
        );
        assert_eq!(
            SpanStatus::from_i32(2, ""),
            SpanStatus::Error(String::new())
        );
        assert_eq!(
            SpanStatus::from_i32(9, "x"),
            SpanStatus::Other(9, "x".into())
        );
        assert_eq!(
            SpanStatus::Other(2, "kept".into()).normalize(),
            SpanStatus::Error("kept".into())
        );

        assert_eq!(SamplingPriority::from_i32(1), SamplingPriority::AutoKeep);
        assert_eq!(SamplingPriority::from_i32(7), SamplingPriority::Other(7));
        assert_eq!(
            SamplingPriority::Other(1).normalize(),
            SamplingPriority::AutoKeep
        );
        assert_eq!(
            SamplingPriority::Other(1).to_i32(),
            SamplingPriority::AutoKeep.to_i32()
        );
        assert_eq!(SamplingPriority::UserReject.to_i32(), -1);
        assert_eq!(
            SpanStatus::Other(2, "kept".into()).to_i32(),
            SpanStatus::ERROR
        );
    }
}

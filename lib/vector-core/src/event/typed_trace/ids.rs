//! Non-zero trace and span identifiers.

use std::{
    fmt,
    num::{NonZeroU64, NonZeroU128},
};

use vector_common::byte_size_of::ByteSizeOf;

/// Identifier rejected because it was zero or had the wrong byte length.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidIdError {
    /// The numeric value was zero.
    Zero,
    /// A byte slice did not match the identifier width.
    InvalidLength {
        /// Expected byte length.
        expected: usize,
        /// Actual byte length.
        actual: usize,
    },
}

impl fmt::Display for InvalidIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Zero => f.write_str("identifier must be non-zero"),
            Self::InvalidLength { expected, actual } => {
                write!(f, "identifier must be {expected} bytes, got {actual}")
            }
        }
    }
}

impl std::error::Error for InvalidIdError {}

/// 128-bit non-zero trace identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TraceId(NonZeroU128);

impl TraceId {
    /// Constructs a trace ID from a non-zero 128-bit integer.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidIdError::Zero`] when `id` is zero.
    pub const fn new(id: u128) -> Result<Self, InvalidIdError> {
        match NonZeroU128::new(id) {
            Some(id) => Ok(Self(id)),
            None => Err(InvalidIdError::Zero),
        }
    }

    /// Reconstructs a 128-bit ID from Datadog's low/high 64-bit halves.
    ///
    /// Either half may be zero when the other is not. The combined value must be non-zero.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidIdError::Zero`] when both halves are zero.
    pub const fn from_datadog(low: u64, high: u64) -> Result<Self, InvalidIdError> {
        Self::new(((high as u128) << 64) | (low as u128))
    }

    /// Interprets 16 big-endian bytes as a trace ID.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidIdError::Zero`] when the bytes are all zero.
    pub const fn from_bytes(bytes: [u8; 16]) -> Result<Self, InvalidIdError> {
        Self::new(u128::from_be_bytes(bytes))
    }

    /// Interprets a byte slice as a 16-byte big-endian trace ID.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidIdError::InvalidLength`] when the slice is not 16 bytes, or
    /// [`InvalidIdError::Zero`] when the bytes are all zero.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, InvalidIdError> {
        let bytes: [u8; 16] = bytes
            .try_into()
            .map_err(|_| InvalidIdError::InvalidLength {
                expected: 16,
                actual: bytes.len(),
            })?;
        Self::from_bytes(bytes)
    }

    /// Returns the integer value.
    #[inline]
    pub const fn get(self) -> u128 {
        self.0.get()
    }

    /// Low 64 bits of the ID. May be zero when the high half is non-zero.
    #[inline]
    #[allow(clippy::cast_possible_truncation)] // Datadog wire `traceID` is the low 64 bits.
    pub const fn low_u64(self) -> u64 {
        self.0.get() as u64
    }

    /// High 64 bits of the ID.
    #[inline]
    #[allow(clippy::cast_possible_truncation)] // High half after shifting occupies the low 64 bits.
    pub const fn high_u64(self) -> u64 {
        (self.0.get() >> 64) as u64
    }

    /// Big-endian 16-byte encoding used by OTLP.
    #[inline]
    pub const fn to_bytes(self) -> [u8; 16] {
        self.0.get().to_be_bytes()
    }
}

impl TryFrom<u128> for TraceId {
    type Error = InvalidIdError;

    fn try_from(value: u128) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<[u8; 16]> for TraceId {
    type Error = InvalidIdError;

    fn try_from(value: [u8; 16]) -> Result<Self, Self::Error> {
        Self::from_bytes(value)
    }
}

impl From<NonZeroU128> for TraceId {
    fn from(id: NonZeroU128) -> Self {
        Self(id)
    }
}

impl From<TraceId> for u128 {
    fn from(id: TraceId) -> Self {
        id.get()
    }
}

impl fmt::Display for TraceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:032x}", self.get())
    }
}

impl ByteSizeOf for TraceId {
    fn allocated_bytes(&self) -> usize {
        0
    }
}

/// 64-bit non-zero span identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SpanId(NonZeroU64);

impl SpanId {
    /// Constructs a span ID from a non-zero 64-bit integer.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidIdError::Zero`] when `id` is zero.
    pub const fn new(id: u64) -> Result<Self, InvalidIdError> {
        match NonZeroU64::new(id) {
            Some(id) => Ok(Self(id)),
            None => Err(InvalidIdError::Zero),
        }
    }

    /// Interprets 8 big-endian bytes as a span ID.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidIdError::Zero`] when the bytes are all zero.
    pub const fn from_bytes(bytes: [u8; 8]) -> Result<Self, InvalidIdError> {
        Self::new(u64::from_be_bytes(bytes))
    }

    /// Interprets a byte slice as an 8-byte big-endian span ID.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidIdError::InvalidLength`] when the slice is not 8 bytes, or
    /// [`InvalidIdError::Zero`] when the bytes are all zero.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, InvalidIdError> {
        let bytes: [u8; 8] = bytes
            .try_into()
            .map_err(|_| InvalidIdError::InvalidLength {
                expected: 8,
                actual: bytes.len(),
            })?;
        Self::from_bytes(bytes)
    }

    /// Returns the integer value.
    #[inline]
    pub const fn get(self) -> u64 {
        self.0.get()
    }

    /// Big-endian 8-byte encoding used by OTLP.
    #[inline]
    pub const fn to_bytes(self) -> [u8; 8] {
        self.0.get().to_be_bytes()
    }
}

impl TryFrom<u64> for SpanId {
    type Error = InvalidIdError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<[u8; 8]> for SpanId {
    type Error = InvalidIdError;

    fn try_from(value: [u8; 8]) -> Result<Self, Self::Error> {
        Self::from_bytes(value)
    }
}

impl From<NonZeroU64> for SpanId {
    fn from(id: NonZeroU64) -> Self {
        Self(id)
    }
}

impl From<SpanId> for u64 {
    fn from(id: SpanId) -> Self {
        id.get()
    }
}

impl fmt::Display for SpanId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.get())
    }
}

impl ByteSizeOf for SpanId {
    fn allocated_bytes(&self) -> usize {
        0
    }
}

#[cfg(test)]
mod tests {
    use std::num::{NonZeroU64, NonZeroU128};

    use similar_asserts::assert_eq;

    use super::{InvalidIdError, SpanId, TraceId};

    #[test]
    fn trace_id_rejects_zero_and_splits_halves() {
        assert_eq!(TraceId::new(0), Err(InvalidIdError::Zero));
        assert_eq!(TraceId::from_datadog(0, 0), Err(InvalidIdError::Zero));

        let low_only = TraceId::from_datadog(0xABCD, 0).unwrap();
        assert_eq!(low_only.low_u64(), 0xABCD);
        assert_eq!(low_only.high_u64(), 0);

        let high_only = TraceId::from_datadog(0, 0x12).unwrap();
        assert_eq!(high_only.low_u64(), 0);
        assert_eq!(high_only.high_u64(), 0x12);
        assert_eq!(high_only.get(), 0x12 << 64);

        let both = TraceId::from_datadog(u64::MAX, 1).unwrap();
        assert_eq!(both.low_u64(), u64::MAX);
        assert_eq!(both.high_u64(), 1);
    }

    #[test]
    fn identifiers_from_bytes_and_nonzero_from() {
        assert_eq!(
            TraceId::from_slice(&[0; 8]),
            Err(InvalidIdError::InvalidLength {
                expected: 16,
                actual: 8
            })
        );
        assert_eq!(TraceId::from_bytes([0; 16]), Err(InvalidIdError::Zero));

        let mut bytes = [0u8; 16];
        bytes[15] = 1;
        let id = TraceId::from_bytes(bytes).unwrap();
        assert_eq!(id.to_bytes(), bytes);
        assert_eq!(id.to_string(), "00000000000000000000000000000001");

        let nz = NonZeroU128::new(7).unwrap();
        assert_eq!(TraceId::from(nz).get(), 7);

        assert_eq!(
            SpanId::from_slice(&[0; 4]),
            Err(InvalidIdError::InvalidLength {
                expected: 8,
                actual: 4
            })
        );
        assert_eq!(SpanId::from_bytes([0; 8]), Err(InvalidIdError::Zero));
        let mut span_bytes = [0u8; 8];
        span_bytes[7] = 0xAB;
        let sid = SpanId::from_bytes(span_bytes).unwrap();
        assert_eq!(sid.to_bytes(), span_bytes);
        assert_eq!(sid.to_string(), "00000000000000ab");
        assert_eq!(SpanId::from(NonZeroU64::new(9).unwrap()).get(), 9);
    }
}

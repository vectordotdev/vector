//! Low-level protobuf wire-format decoding used by the wire-to-Arrow encoder.
//!
//! This is a self-contained subset of a proto wire parser: just enough to
//! walk a serialized message's tag/value pairs in a single pass and read
//! scalar values out of them. The encoder pairs these raw wire values against
//! a proto `MessageDescriptor` in [`super::scan`] / [`super::append`], so no
//! schema knowledge lives here.

use std::fmt;

/// Error type for protobuf wire-format parsing failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// Not enough bytes to read a fixed-size or length-delimited field.
    BufferTooShort {
        /// Number of bytes the field required.
        needed: usize,
        /// Number of bytes actually available.
        available: usize,
        /// The field number being parsed.
        field_num: i32,
    },
    /// Field number out of valid range (1 to 536,870,911).
    InvalidFieldNumber {
        /// The out-of-range field number.
        field_num: i32,
    },
    /// Invalid wire type value (must be 0-5).
    InvalidWireType(u8),
    /// Not enough bytes to parse a varint.
    TruncatedVarint,
    /// Deprecated group wire types are not supported.
    UnsupportedGroupWireType,
    /// Varint encoding uses more than 10 bytes.
    VarintTooLong,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::BufferTooShort {
                needed,
                available,
                field_num,
            } => write!(
                f,
                "Field #{field_num}: Input buffer too short: need {needed} bytes, have {available}"
            ),
            ParseError::InvalidFieldNumber { field_num } => write!(
                f,
                "Field number {field_num} is out of valid range (must be 1 to 536,870,911)"
            ),
            ParseError::InvalidWireType(wt) => write!(f, "Invalid wire type: {wt}"),
            ParseError::TruncatedVarint => write!(f, "Truncated varint"),
            ParseError::UnsupportedGroupWireType => write!(f, "Group wire types are not supported"),
            ParseError::VarintTooLong => write!(f, "Varint exceeds 10 bytes"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Result type for wire-format parsing operations.
pub type ParseResult<T> = Result<T, ParseError>;

/// Raw wire value before schema interpretation.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum WireValue<'a> {
    /// A base-128 varint (bool, int32/64, uint32/64, sint32/64 (zigzag), enum).
    Varint(u64),
    /// Fixed 8-byte value (fixed64, sfixed64, double).
    I64(u64),
    /// Length-delimited value (string, bytes, embedded message, packed repeated).
    Len(&'a [u8]),
    /// Fixed 4-byte value (fixed32, sfixed32, float).
    I32(u32),
}

/// ZigZag decode a 32-bit value (used for sint32).
#[inline(always)]
pub fn decode_zigzag32(n: u32) -> i32 {
    ((n >> 1) as i32) ^ -((n & 1) as i32)
}

/// ZigZag decode a 64-bit value (used for sint64).
#[inline(always)]
pub fn decode_zigzag64(n: u64) -> i64 {
    ((n >> 1) as i64) ^ -((n & 1) as i64)
}

/// A wire type as seen on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum WireType {
    /// The Varint WireType indicates the value is a single VARINT.
    Varint = 0,
    /// The I64 WireType indicates that the value is precisely 8 bytes in
    /// little-endian order containing a 64-bit signed integer or double type.
    I64 = 1,
    /// The Len WireType indicates that the value is a length represented as a
    /// VARINT followed by exactly that number of bytes.
    Len = 2,
    /// Deprecated protobuf groups (start).
    StartGroup = 3,
    /// Deprecated protobuf groups (end).
    EndGroup = 4,
    /// The I32 WireType indicates that the value is precisely 4 bytes in
    /// little-endian order containing a 32-bit signed integer or float type.
    I32 = 5,
}

impl TryFrom<u64> for WireType {
    type Error = ParseError;

    #[inline(always)]
    fn try_from(value: u64) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(WireType::Varint),
            1 => Ok(WireType::I64),
            2 => Ok(WireType::Len),
            3 => Ok(WireType::StartGroup),
            4 => Ok(WireType::EndGroup),
            5 => Ok(WireType::I32),
            _ => Err(ParseError::InvalidWireType(value as u8)),
        }
    }
}

/// Parsed wire field with field number and raw value.
#[derive(Debug, PartialEq, Clone, Copy)]
pub struct WireField<'a> {
    /// The field number (tag >> 3).
    pub field_num: i32,
    /// The raw wire value.
    pub value: WireValue<'a>,
}

/// Parse a VARINT, returning the parsed value and the remaining bytes.
/// A 64-bit varint can require up to 10 bytes (64 bits / 7 bits per byte).
///
/// Optimized with fast paths for 1-5 byte varints (covers ~99.9% of cases).
///
/// Returns `Err(ParseError::TruncatedVarint)` if buffer is too short.
/// Returns `Err(ParseError::VarintTooLong)` if varint exceeds 10 bytes.
#[inline(always)]
pub fn try_read_varint(data: &[u8]) -> ParseResult<(u64, &[u8])> {
    match *data {
        // Empty buffer.
        [] => Err(ParseError::TruncatedVarint),
        // Fast path: 1-byte varint (values 0-127, very common for field tags and small ints).
        [b0, ref rest @ ..] if b0 < 0x80 => Ok((b0 as u64, rest)),
        // Only 1 byte but continuation bit set.
        [_] => Err(ParseError::TruncatedVarint),
        // Fast path: 2-byte varint (values 128-16383).
        [b0, b1, ref rest @ ..] if b1 < 0x80 => {
            Ok((((b0 & 0x7f) as u64) | ((b1 as u64) << 7), rest))
        }
        // Only 2 bytes but continuation bit set.
        [_, _] => Err(ParseError::TruncatedVarint),
        // Fast path: 3-byte varint (values 16384-2097151).
        [b0, b1, b2, ref rest @ ..] if b2 < 0x80 => Ok((
            ((b0 & 0x7f) as u64) | (((b1 & 0x7f) as u64) << 7) | ((b2 as u64) << 14),
            rest,
        )),
        // Only 3 bytes but continuation bit set.
        [_, _, _] => Err(ParseError::TruncatedVarint),
        // Fast path: 4-byte varint (values 2097152-268435455).
        [b0, b1, b2, b3, ref rest @ ..] if b3 < 0x80 => Ok((
            ((b0 & 0x7f) as u64)
                | (((b1 & 0x7f) as u64) << 7)
                | (((b2 & 0x7f) as u64) << 14)
                | ((b3 as u64) << 21),
            rest,
        )),
        // Only 4 bytes but continuation bit set.
        [_, _, _, _] => Err(ParseError::TruncatedVarint),
        // Fast path: 5-byte varint (values 268435456-34359738367).
        [b0, b1, b2, b3, b4, ref rest @ ..] if b4 < 0x80 => Ok((
            ((b0 & 0x7f) as u64)
                | (((b1 & 0x7f) as u64) << 7)
                | (((b2 & 0x7f) as u64) << 14)
                | (((b3 & 0x7f) as u64) << 21)
                | ((b4 as u64) << 28),
            rest,
        )),
        // Slow path: 6+ byte varints (rare).
        _ => parse_varint_slow(data),
    }
}

/// Slow path for varints with 6+ bytes.
#[inline(always)]
fn parse_varint_slow(data: &[u8]) -> ParseResult<(u64, &[u8])> {
    let mut value = 0u64;
    let mut shift = 0;

    // Process bytes 0-8 (each contributes 7 bits).
    for i in 0..9 {
        let Some(&b) = data.get(i) else {
            return Err(ParseError::TruncatedVarint);
        };
        value |= ((b & 0x7f) as u64) << shift;
        if b < 0x80 {
            return Ok((value, &data[i + 1..]));
        }
        shift += 7;
    }

    // 10th byte (index 9): can only contribute bit 0 (9*7 + 1 = 64 bits total).
    // Bits 1-6 would overflow u64, bit 7 (continuation) would require 11+ bytes.
    let Some(&b) = data.get(9) else {
        return Err(ParseError::TruncatedVarint);
    };
    if b > 0x01 {
        return Err(ParseError::VarintTooLong);
    }
    value |= (b as u64) << shift;
    Ok((value, &data[10..]))
}

/// Convert a tag into a field number and a WireType.
/// Returns error if wire type is invalid or field number is out of range.
#[inline(always)]
fn try_unpack_tag(tag: u64) -> ParseResult<(i32, WireType)> {
    let field_num = (tag >> 3) as i32;
    let wire_type = WireType::try_from(tag & 0x7)?;

    // Validate field number range per protobuf spec.
    // Field numbers must be 1 to 536,870,911 (2^29 - 1).
    if !(1..=536_870_911).contains(&field_num) {
        return Err(ParseError::InvalidFieldNumber { field_num });
    }

    Ok((field_num, wire_type))
}

/// Parse a field, returning the field and remaining bytes.
///
/// Returns error on malformed input (truncated buffer, invalid wire type, etc.)
#[inline(always)]
pub fn try_parse_field(data: &[u8]) -> ParseResult<(WireField<'_>, &[u8])> {
    let (tag, remainder) = try_read_varint(data)?;
    let (field_num, wire_type) = try_unpack_tag(tag)?;
    let (fieldvalue, remainder) = match wire_type {
        WireType::Varint => {
            let (value, remainder) = try_read_varint(remainder)?;
            (WireValue::Varint(value), remainder)
        }
        WireType::I64 => {
            // Fixed 8 bytes in little-endian order.
            let Some((bytes, rest)) = remainder.split_first_chunk::<8>() else {
                return Err(ParseError::BufferTooShort {
                    needed: 8,
                    available: remainder.len(),
                    field_num,
                });
            };
            let value = u64::from_le_bytes(*bytes);
            (WireValue::I64(value), rest)
        }
        WireType::Len => {
            let (len, remainder) = try_read_varint(remainder)?;
            let len = len as usize;
            if remainder.len() < len {
                return Err(ParseError::BufferTooShort {
                    needed: len,
                    available: remainder.len(),
                    field_num,
                });
            }
            let (value, remainder) = remainder.split_at(len);
            (WireValue::Len(value), remainder)
        }
        WireType::I32 => {
            // Fixed 4 bytes in little-endian order.
            let Some((bytes, rest)) = remainder.split_first_chunk::<4>() else {
                return Err(ParseError::BufferTooShort {
                    needed: 4,
                    available: remainder.len(),
                    field_num,
                });
            };
            let value = u32::from_le_bytes(*bytes);
            (WireValue::I32(value), rest)
        }
        WireType::StartGroup | WireType::EndGroup => {
            return Err(ParseError::UnsupportedGroupWireType);
        }
    };
    Ok((
        WireField {
            field_num,
            value: fieldvalue,
        },
        remainder,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_parsing() {
        // Test cases: (input bytes, expected value, expected remaining bytes).
        let cases = [
            // 1-byte varints.
            (&[0x00][..], 0, &[][..]),
            (&[0x01][..], 1, &[][..]),
            (&[0x7F][..], 127, &[][..]),
            // 2-byte varints.
            (&[0x80, 0x01][..], 128, &[][..]),
            // 3-byte varints.
            (&[0x80, 0x80, 0x01][..], 16384, &[][..]),
            // 4-byte varints.
            (&[0x80, 0x80, 0x80, 0x01][..], 2097152, &[][..]),
            (&[0xFF, 0xFF, 0xFF, 0x7F][..], 268435455, &[][..]),
            // 5-byte varints.
            (&[0x80, 0x80, 0x80, 0x80, 0x01][..], 268435456, &[][..]),
            (&[0xFF, 0xFF, 0xFF, 0xFF, 0x7F][..], 34359738367, &[][..]),
            // With trailing bytes.
            (&[0x01, 0x02, 0x03][..], 1, &[0x02, 0x03][..]),
            (&[0x80, 0x80, 0x80, 0x01, 0xFF][..], 2097152, &[0xFF][..]),
        ];
        for (data, expected_val, expected_rest) in cases {
            let (value, rest) = try_read_varint(data).expect("Failed to parse varint");
            assert_eq!(value, expected_val, "data: {data:?}");
            assert_eq!(rest, expected_rest, "data: {data:?}");
        }
    }

    #[test]
    fn varint_errors() {
        // Truncated varints.
        assert_eq!(try_read_varint(&[0x80]), Err(ParseError::TruncatedVarint));
        assert_eq!(
            try_read_varint(&[0x80, 0x80]),
            Err(ParseError::TruncatedVarint)
        );
        assert_eq!(
            try_read_varint(&[0x80, 0x80, 0x80]),
            Err(ParseError::TruncatedVarint)
        );
        assert_eq!(
            try_read_varint(&[0x80, 0x80, 0x80, 0x80]),
            Err(ParseError::TruncatedVarint)
        );
        assert_eq!(
            try_read_varint(&[0x80, 0x80, 0x80, 0x80, 0x80]),
            Err(ParseError::TruncatedVarint)
        );

        // Varint too long (11 bytes).
        let too_long = &[
            0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x01,
        ];
        assert_eq!(try_read_varint(too_long), Err(ParseError::VarintTooLong));
    }

    #[test]
    fn varint_10th_byte_validation() {
        // 10-byte varints: bytes 0-8 all have continuation bit set, byte 9 is the 10th byte.
        // The 10th byte can only have bit 0 set (bits 1-6 would overflow u64, bit 7 would need 11+ bytes).

        // Valid: 10th byte = 0x00 (contributes 0 to the value).
        let valid_zero = &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00];
        let (value, rest) = try_read_varint(valid_zero).unwrap();
        assert_eq!(value, 0);
        assert!(rest.is_empty());

        // Valid: 10th byte = 0x01 (sets bit 63, gives 2^63).
        let valid_one = &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x01];
        let (value, rest) = try_read_varint(valid_one).unwrap();
        assert_eq!(value, 1u64 << 63);
        assert!(rest.is_empty());

        // Valid: u64::MAX = all bits set.
        let max_u64 = &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01];
        let (value, rest) = try_read_varint(max_u64).unwrap();
        assert_eq!(value, u64::MAX);
        assert!(rest.is_empty());

        // Invalid: 10th byte = 0x02 (bit 1 set, would overflow u64).
        let overflow_bit1 = &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x02];
        assert_eq!(
            try_read_varint(overflow_bit1),
            Err(ParseError::VarintTooLong)
        );

        // Invalid: 10th byte = 0x7F (bits 1-6 all set, would overflow u64).
        let overflow_bits = &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x7F];
        assert_eq!(
            try_read_varint(overflow_bits),
            Err(ParseError::VarintTooLong)
        );

        // Invalid: 10th byte = 0x80 (continuation bit set, would need 11+ bytes).
        let continuation = &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80];
        assert_eq!(
            try_read_varint(continuation),
            Err(ParseError::VarintTooLong)
        );
    }

    #[test]
    fn zigzag_decoding() {
        let cases32 = [
            (0, 0),
            (1, -1),
            (2, 1),
            (3, -2),
            (4, 2),
            (99, -50),
            (100, 50),
        ];
        for (encoded, expected) in cases32 {
            assert_eq!(decode_zigzag32(encoded), expected, "zigzag32({encoded})");
        }

        let cases64 = [(0, 0), (1, -1), (2, 1), (3, -2), (4, 2)];
        for (encoded, expected) in cases64 {
            assert_eq!(decode_zigzag64(encoded), expected, "zigzag64({encoded})");
        }
    }

    #[test]
    fn wire_type_conversion() {
        let valid = [
            (0, WireType::Varint),
            (1, WireType::I64),
            (2, WireType::Len),
            (3, WireType::StartGroup),
            (4, WireType::EndGroup),
            (5, WireType::I32),
        ];
        for (val, expected) in valid {
            assert_eq!(WireType::try_from(val), Ok(expected));
        }

        assert_eq!(
            WireType::try_from(6u64),
            Err(ParseError::InvalidWireType(6))
        );
        assert_eq!(
            WireType::try_from(7u64),
            Err(ParseError::InvalidWireType(7))
        );
    }

    #[test]
    fn field_parsing() {
        // (data, expected_field_num, expected_value).
        let cases = [
            // Varint: field 1, value 150. Tag = 8, 150 = 0x96 0x01.
            (&[8, 0x96, 0x01][..], 1, WireValue::Varint(150)),
            // I32: field 1, tag = 13, value 0x01020304 little-endian.
            (
                &[13, 0x04, 0x03, 0x02, 0x01][..],
                1,
                WireValue::I32(0x01020304),
            ),
            // I64: field 1, tag = 9.
            (
                &[9, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08][..],
                1,
                WireValue::I64(0x0807060504030201),
            ),
            // Len: field 1, tag = 10, length 3.
            (
                &[10, 3, 0xAA, 0xBB, 0xCC][..],
                1,
                WireValue::Len(&[0xAA, 0xBB, 0xCC]),
            ),
        ];
        for (data, expected_num, expected_val) in cases {
            let (field, rest) = try_parse_field(data).unwrap();
            assert_eq!(field.field_num, expected_num, "data: {data:?}");
            assert_eq!(field.value, expected_val, "data: {data:?}");
            assert!(rest.is_empty(), "data: {data:?}");
        }
    }

    #[test]
    fn field_parsing_errors() {
        // Invalid wire type 6: tag = (1 << 3) | 6 = 14.
        assert_eq!(try_parse_field(&[14]), Err(ParseError::InvalidWireType(6)));

        // Group wire type: tag = (1 << 3) | 3 = 11.
        assert_eq!(
            try_parse_field(&[11]),
            Err(ParseError::UnsupportedGroupWireType)
        );

        // Buffer too short for I32: tag 13, only 2 bytes.
        assert_eq!(
            try_parse_field(&[13, 0x01, 0x02]),
            Err(ParseError::BufferTooShort {
                needed: 4,
                available: 2,
                field_num: 1,
            })
        );

        // Buffer too short for I64: tag 9, only 4 bytes.
        assert_eq!(
            try_parse_field(&[9, 0x01, 0x02, 0x03, 0x04]),
            Err(ParseError::BufferTooShort {
                needed: 8,
                available: 4,
                field_num: 1,
            })
        );

        // Buffer too short for Len: tag 10, length 100, only 3 bytes.
        assert_eq!(
            try_parse_field(&[10, 100, 0x01, 0x02, 0x03]),
            Err(ParseError::BufferTooShort {
                needed: 100,
                available: 3,
                field_num: 1,
            })
        );

        // Invalid field number 0: tag = (0 << 3) | 0 = 0.
        assert_eq!(
            try_parse_field(&[0]),
            Err(ParseError::InvalidFieldNumber { field_num: 0 })
        );

        // Invalid field number 536_870_912 (2^29) - exceeds max valid field number 536_870_911.
        // Tag = (536_870_912 << 3) | 0 = 4_294_967_296 -> varint [0x80, 0x80, 0x80, 0x80, 0x10].
        assert_eq!(
            try_parse_field(&[0x80, 0x80, 0x80, 0x80, 0x10]),
            Err(ParseError::InvalidFieldNumber {
                field_num: 536_870_912
            })
        );

        // Max valid field number 536_870_911 (2^29 - 1) is okay.
        // Tag = (536_870_911 << 3) | 0 = 4_294_967_288 -> varint [0xF8, 0xFF, 0xFF, 0xFF, 0x0F].
        let (field, _) = try_parse_field(&[0xF8, 0xFF, 0xFF, 0xFF, 0x0F, 0x01]).unwrap();
        assert_eq!(field.field_num, 536_870_911);
    }
}

//! Wire helpers for `datadog.trace.idx`.
//!
//! These functions follow the Datadog Agent's accessors: a bounds-checked
//! string table, `LegacyTraceID` / `TraceIDHigh` splitting, `SpanKind` names,
//! and the legacy `_dd.p.dm` sampling-mechanism tag.

use std::fmt;

use crate::trace::idx::SpanKind;

/// A string-table index that does not point at an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidStringRef {
    /// The out-of-range index.
    pub index: u32,
}

impl fmt::Display for InvalidStringRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "idx string table ref {} is out of range",
            self.index
        )
    }
}

impl std::error::Error for InvalidStringRef {}

/// Borrowed view of an idx string table.
///
/// Payloads produced by the agent place `""` at index 0. An out-of-range ref is
/// a corrupt payload; the agent panics on one instead of sending it.
#[derive(Debug, Clone, Copy)]
pub struct StringTable<'a> {
    strings: &'a [String],
}

impl<'a> StringTable<'a> {
    /// Borrows `strings` as a string table.
    #[must_use]
    pub const fn new(strings: &'a [String]) -> Self {
        Self { strings }
    }

    /// Returns the string at `index`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidStringRef`] when `index` is past the end of the table.
    pub fn get(&self, index: u32) -> Result<&'a str, InvalidStringRef> {
        usize::try_from(index)
            .ok()
            .and_then(|position| self.strings.get(position))
            .map(String::as_str)
            .ok_or(InvalidStringRef { index })
    }
}

/// Splits a big-endian trace ID into `(high, low)` 64-bit halves.
///
/// IDs shorter than 16 bytes are right-aligned into a zero-padded 16-byte
/// buffer, matching the agent's `LegacyTraceID` and `TraceIDHigh`. IDs of 16
/// bytes or more use the first 16 bytes, which is what those accessors read.
#[must_use]
pub fn split_trace_id(trace_id: &[u8]) -> (u64, u64) {
    let mut buf = [0_u8; 16];
    if trace_id.len() >= 16 {
        buf.copy_from_slice(&trace_id[..16]);
    } else {
        let start = 16 - trace_id.len();
        buf[start..].copy_from_slice(trace_id);
    }
    let mut high = [0_u8; 8];
    let mut low = [0_u8; 8];
    high.copy_from_slice(&buf[..8]);
    low.copy_from_slice(&buf[8..]);
    (u64::from_be_bytes(high), u64::from_be_bytes(low))
}

/// Joins the high and low halves of a trace ID into a 16-byte big-endian ID.
#[must_use]
pub fn join_trace_id(high: u64, low: u64) -> [u8; 16] {
    let mut buf = [0_u8; 16];
    buf[..8].copy_from_slice(&high.to_be_bytes());
    buf[8..].copy_from_slice(&low.to_be_bytes());
    buf
}

/// Returns the legacy `span.kind` name for `kind`.
///
/// `UNSPECIFIED` is `"unknown"`, matching `InternalSpan.SpanKind`.
#[must_use]
pub const fn span_kind_name(kind: SpanKind) -> &'static str {
    match kind {
        SpanKind::Internal => "internal",
        SpanKind::Server => "server",
        SpanKind::Client => "client",
        SpanKind::Producer => "producer",
        SpanKind::Consumer => "consumer",
        SpanKind::Unspecified => "unknown",
    }
}

/// Parses a legacy `span.kind` name.
///
/// `"internal"`, a missing name, and any unknown name, including `"unknown"`,
/// become [`SpanKind::Internal`], matching `ConvertToIdx`.
#[must_use]
pub fn span_kind_from_name(name: &str) -> SpanKind {
    match name {
        "server" => SpanKind::Server,
        "client" => SpanKind::Client,
        "producer" => SpanKind::Producer,
        "consumer" => SpanKind::Consumer,
        _ => SpanKind::Internal,
    }
}

/// Formats a sampling mechanism as the legacy `_dd.p.dm` tag value `"-<n>"`.
#[must_use]
pub fn format_sampling_mechanism(mechanism: u32) -> String {
    format!("-{mechanism}")
}

/// Parses a legacy `_dd.p.dm` tag value.
///
/// Accepts the `"-<n>"` form written by [`format_sampling_mechanism`] and a
/// bare unsigned integer. A leading `+` is rejected, matching Go's
/// `ParseUint`.
#[must_use]
pub fn parse_sampling_mechanism(value: &str) -> Option<u32> {
    let digits = value.strip_prefix('-').unwrap_or(value);
    (!digits.is_empty() && !digits.starts_with('+'))
        .then(|| digits.parse().ok())
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::{
        InvalidStringRef, StringTable, format_sampling_mechanism, join_trace_id,
        parse_sampling_mechanism, span_kind_from_name, span_kind_name, split_trace_id,
    };
    use crate::trace::idx::SpanKind;

    #[test]
    fn string_table_rejects_out_of_range_refs() {
        let empty = StringTable::new(&[]);
        assert_eq!(empty.get(0), Err(InvalidStringRef { index: 0 }));

        let strings = vec![String::new(), "service".to_owned()];
        let table = StringTable::new(&strings);
        assert_eq!(table.get(0).unwrap(), "");
        assert_eq!(table.get(1).unwrap(), "service");
        assert_eq!(table.get(2), Err(InvalidStringRef { index: 2 }));
        assert_eq!(
            table.get(u32::MAX),
            Err(InvalidStringRef { index: u32::MAX })
        );
    }

    #[test]
    fn reexport_is_visible_on_the_trace_module() {
        let strings = vec![String::new()];
        let table = crate::trace::idx::StringTable::new(&strings);
        assert_eq!(table.get(0).unwrap(), "");
    }

    #[test]
    fn trace_ids_split_and_join_with_legacy_alignment() {
        let high = 0x0123_4567_89ab_cdef;
        let low = 0xfedc_ba98_7654_3210;
        assert_eq!(split_trace_id(&join_trace_id(high, low)), (high, low));

        assert_eq!(split_trace_id(&[]), (0, 0));
        assert_eq!(split_trace_id(&[0x01, 0x02]), (0, 0x0102));
        assert_eq!(split_trace_id(&[0xff; 8]), (0, u64::MAX));
        assert_eq!(
            split_trace_id(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09]),
            (0x01, 0x0203_0405_0607_0809)
        );

        let mut overlong = join_trace_id(high, low).to_vec();
        overlong.push(0xaa);
        assert_eq!(split_trace_id(&overlong), (high, low));
    }

    #[test]
    fn span_kinds_map_in_both_directions() {
        for (kind, name) in [
            (SpanKind::Internal, "internal"),
            (SpanKind::Server, "server"),
            (SpanKind::Client, "client"),
            (SpanKind::Producer, "producer"),
            (SpanKind::Consumer, "consumer"),
        ] {
            assert_eq!(span_kind_name(kind), name);
            assert_eq!(span_kind_from_name(name), kind);
        }
        assert_eq!(span_kind_name(SpanKind::Unspecified), "unknown");
        assert_eq!(span_kind_from_name("unknown"), SpanKind::Internal);
        assert_eq!(span_kind_from_name(""), SpanKind::Internal);
        assert_eq!(span_kind_from_name("SPAN_KIND_SERVER"), SpanKind::Internal);
    }

    #[test]
    fn sampling_mechanism_formats_and_parses() {
        assert_eq!(format_sampling_mechanism(4), "-4");
        assert_eq!(parse_sampling_mechanism("-4"), Some(4));
        assert_eq!(
            parse_sampling_mechanism(&format_sampling_mechanism(0)),
            Some(0)
        );
        assert_eq!(parse_sampling_mechanism("12"), Some(12));
        assert_eq!(parse_sampling_mechanism("-0"), Some(0));
        assert_eq!(parse_sampling_mechanism(""), None);
        assert_eq!(parse_sampling_mechanism("-"), None);
        assert_eq!(parse_sampling_mechanism("+1"), None);
        assert_eq!(parse_sampling_mechanism("1.5"), None);
        assert_eq!(parse_sampling_mechanism("-4extra"), None);
    }
}

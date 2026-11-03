//! Trace flags and W3C `tracestate` storage.

use bitmask_enum::bitmask;
use vector_common::byte_size_of::ByteSizeOf;

/// OTLP `Span.flags` / `Link.flags` bitfield.
///
/// Unknown bits are retained so reserved OTLP bits and future W3C flags round-trip.
/// [`From<u32>`] keeps the raw word; [`Self::truncate`] drops bits that have no named flag.
#[bitmask(u32)]
#[derive(Default)]
pub enum OtlpSpanFlags {
    /// W3C sampled bit (`traceparent` flags low bit).
    SAMPLED = 0x0001,
    /// OTLP bit indicating that [`Self::CONTEXT_IS_REMOTE`] is meaningful.
    CONTEXT_HAS_IS_REMOTE = 0x0100,
    /// OTLP parent- / link-target-is-remote bit.
    CONTEXT_IS_REMOTE = 0x0200,
}

impl OtlpSpanFlags {
    /// Low 8 bits, the W3C trace-flags byte.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)] // W3C flags occupy only the low byte.
    pub const fn w3c_byte(self) -> u8 {
        self.bits() as u8
    }

    /// OTLP remoteness tristate: `None` when the presence bit is unset.
    #[must_use]
    pub const fn context_is_remote(self) -> Option<bool> {
        if self.contains(Self::CONTEXT_HAS_IS_REMOTE) {
            Some(self.contains(Self::CONTEXT_IS_REMOTE))
        } else {
            None
        }
    }
}

impl ByteSizeOf for OtlpSpanFlags {
    fn allocated_bytes(&self) -> usize {
        0
    }
}

/// Raw W3C `tracestate` header.
///
/// The string is stored verbatim. Structured accessors parse on demand; [`Self::insert`]
/// rebuilds the header with the mutated member at the front per W3C Trace Context §3.3.1.
/// List entries that do not contain `=`, including empty and whitespace-only entries, are not
/// members. Mutations copy every entry they do not change through unchanged.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TraceState(String);

impl TraceState {
    /// Stores a raw header without validation.
    #[must_use]
    pub fn from_raw(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Returns the raw header string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns `true` when the raw header is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the value of the first member with `key`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.members().find_map(|(k, v)| (k == key).then_some(v))
    }

    /// Inserts or updates `key`, moving it to the head of the list.
    ///
    /// Every other entry is copied through unchanged, including empty entries,
    /// whitespace-only entries, and the original text of retained members.
    pub fn insert(&mut self, key: &str, val: &str) {
        let mut out = String::with_capacity(self.0.len() + key.len() + val.len() + 2);
        out.push_str(key);
        out.push('=');
        out.push_str(val);
        for entry in self.entries_except(key) {
            out.push(',');
            out.push_str(entry);
        }
        self.0 = out;
    }

    /// Removes every member with `key`.
    ///
    /// Returns `true` if at least one member was removed. Every other entry is copied
    /// through unchanged, including empty entries, whitespace-only entries, and the
    /// original text of retained members.
    pub fn remove(&mut self, key: &str) -> bool {
        if self.get(key).is_none() {
            return false;
        }
        self.0 = self.entries_except(key).collect::<Vec<_>>().join(",");
        true
    }

    /// Iterates `(key, value)` members in header order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> + '_ {
        self.members()
    }

    fn list_entries(&self) -> impl Iterator<Item = ListEntry<'_>> + '_ {
        // An empty header has no entries. `split` would otherwise yield one empty part.
        let parts = (!self.0.is_empty()).then(|| self.0.split(','));
        parts
            .into_iter()
            .flatten()
            .map(|part| match part.trim().split_once('=') {
                Some((key, value)) => ListEntry {
                    raw: part,
                    key: key.trim(),
                    value: Some(value),
                },
                None => ListEntry {
                    raw: part,
                    key: part,
                    value: None,
                },
            })
    }

    /// Original text of every entry except members whose key equals `key`.
    fn entries_except<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.list_entries()
            .filter(move |entry| entry.value.is_none() || entry.key != key)
            .map(|entry| entry.raw)
    }

    fn members(&self) -> impl Iterator<Item = (&str, &str)> + '_ {
        self.list_entries()
            .filter_map(|entry| entry.value.map(|value| (entry.key, value)))
    }
}

impl From<String> for TraceState {
    fn from(value: String) -> Self {
        Self::from_raw(value)
    }
}

impl From<&str> for TraceState {
    fn from(value: &str) -> Self {
        Self::from_raw(value)
    }
}

impl ByteSizeOf for TraceState {
    fn allocated_bytes(&self) -> usize {
        self.0.len()
    }
}

/// One comma-separated list entry.
///
/// `raw` is the original slice between commas. `value` is `Some` for a `key=value`
/// member and `None` when the entry has no `=`, including when the entry is empty or
/// whitespace-only. `key` is the trimmed member key, or `raw` when there is no `=`.
#[derive(Copy, Clone)]
struct ListEntry<'a> {
    raw: &'a str,
    key: &'a str,
    value: Option<&'a str>,
}

#[cfg(test)]
mod tests {
    use similar_asserts::assert_eq;

    use super::{OtlpSpanFlags, TraceState};

    #[test]
    fn trace_flags_retain_unknown_bits_and_remoteness() {
        let flags = OtlpSpanFlags::from(0x8000_0001);
        assert!(flags.contains(OtlpSpanFlags::SAMPLED));
        assert_eq!(flags.bits() & 0x8000_0000, 0x8000_0000);
        assert_eq!(flags.w3c_byte(), 0x01);
        assert_eq!(flags.context_is_remote(), None);

        let unknown = OtlpSpanFlags::from(0x0000_0004);
        assert_eq!(unknown.w3c_byte(), 0x04);
        assert!(!unknown.contains(OtlpSpanFlags::SAMPLED));

        let local = OtlpSpanFlags::CONTEXT_HAS_IS_REMOTE;
        assert_eq!(local.context_is_remote(), Some(false));

        let remote = OtlpSpanFlags::CONTEXT_HAS_IS_REMOTE | OtlpSpanFlags::CONTEXT_IS_REMOTE;
        assert_eq!(remote.context_is_remote(), Some(true));

        let remote_without_presence = OtlpSpanFlags::CONTEXT_IS_REMOTE;
        assert_eq!(remote_without_presence.context_is_remote(), None);
        assert_eq!(
            remote_without_presence.bits() & OtlpSpanFlags::CONTEXT_IS_REMOTE.bits(),
            OtlpSpanFlags::CONTEXT_IS_REMOTE.bits()
        );
    }

    #[test]
    fn trace_state_mutation_moves_member_to_head() {
        let mut state = TraceState::from_raw("vendor=1,other=2");
        assert_eq!(state.get("vendor"), Some("1"));
        assert_eq!(
            state.iter().collect::<Vec<_>>(),
            vec![("vendor", "1"), ("other", "2")]
        );

        state.insert("foo", "bar");
        assert_eq!(state.as_str(), "foo=bar,vendor=1,other=2");

        state.insert("vendor", "updated");
        assert_eq!(state.as_str(), "vendor=updated,foo=bar,other=2");

        assert!(state.remove("foo"));
        assert_eq!(state.as_str(), "vendor=updated,other=2");
        assert!(!state.remove("missing"));

        let empty = TraceState::default();
        assert!(empty.is_empty());
        let mut mutated = empty;
        mutated.insert("a", "b");
        assert_eq!(mutated.as_str(), "a=b");
    }

    #[test]
    fn trace_state_mutation_retains_unparsed_fragments() {
        let mut state = TraceState::from_raw("opaque,vendor=1");
        assert_eq!(state.get("vendor"), Some("1"));
        assert_eq!(state.iter().collect::<Vec<_>>(), vec![("vendor", "1")]);

        state.insert("foo", "bar");
        assert_eq!(state.as_str(), "foo=bar,opaque,vendor=1");

        assert!(state.remove("vendor"));
        assert_eq!(state.as_str(), "foo=bar,opaque");
        assert!(!state.remove("missing"));
        assert_eq!(state.as_str(), "foo=bar,opaque");

        let mut spaced = TraceState::from_raw(" opaque ,vendor=1, trailing ");
        spaced.insert("vendor", "updated");
        assert_eq!(spaced.as_str(), "vendor=updated, opaque , trailing ");
        assert!(spaced.remove("vendor"));
        assert_eq!(spaced.as_str(), " opaque , trailing ");
    }

    #[test]
    fn trace_state_mutation_retains_empty_fragments() {
        let mut state = TraceState::from_raw("vendor=1,,other=2");
        assert_eq!(
            state.iter().collect::<Vec<_>>(),
            vec![("vendor", "1"), ("other", "2")]
        );

        state.insert("foo", "bar");
        assert_eq!(state.as_str(), "foo=bar,vendor=1,,other=2");

        assert!(state.remove("vendor"));
        assert_eq!(state.as_str(), "foo=bar,,other=2");

        let mut spaced = TraceState::from_raw("vendor=1, ,other=2");
        assert!(spaced.remove("vendor"));
        assert_eq!(spaced.as_str(), " ,other=2");

        let mut leading = TraceState::from_raw(",,vendor=1");
        leading.insert("foo", "bar");
        assert_eq!(leading.as_str(), "foo=bar,,,vendor=1");
        assert!(leading.remove("vendor"));
        assert_eq!(leading.as_str(), "foo=bar,,");
    }

    #[test]
    fn trace_state_mutation_retains_member_text() {
        let mut spaced = TraceState::from_raw(" vendor=1 ,other=2 ");
        spaced.insert("foo", "bar");
        assert_eq!(spaced.as_str(), "foo=bar, vendor=1 ,other=2 ");
        assert!(spaced.remove("foo"));
        assert_eq!(spaced.as_str(), " vendor=1 ,other=2 ");

        let mut around_equals = TraceState::from_raw("vendor =1,other=2");
        around_equals.insert("foo", "bar");
        assert_eq!(around_equals.as_str(), "foo=bar,vendor =1,other=2");
    }
}

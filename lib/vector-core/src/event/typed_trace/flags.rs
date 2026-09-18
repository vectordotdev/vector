//! Trace flags and W3C `tracestate` storage.

use vector_common::byte_size_of::ByteSizeOf;

bitflags::bitflags! {
    /// OTLP `Span.flags` / `Link.flags` bitfield.
    ///
    /// Unknown bits are retained so reserved OTLP bits and future W3C flags round-trip.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct TraceFlags: u32 {
        /// W3C sampled bit (`traceparent` flags low bit).
        const SAMPLED = 0x0001;
        /// OTLP bit indicating that [`Self::CONTEXT_IS_REMOTE`] is meaningful.
        const CONTEXT_HAS_IS_REMOTE = 0x0100;
        /// OTLP parent- / link-target-is-remote bit.
        const CONTEXT_IS_REMOTE = 0x0200;
        const _ = !0;
    }
}

impl TraceFlags {
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

impl ByteSizeOf for TraceFlags {
    fn allocated_bytes(&self) -> usize {
        0
    }
}

/// Raw W3C `tracestate` header.
///
/// The string is stored verbatim. Structured accessors parse on demand; [`Self::insert`]
/// rebuilds the header with the mutated member at the front per W3C Trace Context §3.3.1.
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
        members(&self.0).find_map(|(k, v)| (k == key).then_some(v))
    }

    /// Inserts or updates `key`, moving it to the head of the list.
    pub fn insert(&mut self, key: &str, val: &str) {
        let mut out = String::with_capacity(self.0.len() + key.len() + val.len() + 2);
        out.push_str(key);
        out.push('=');
        out.push_str(val);
        for (existing_key, existing_val) in members(&self.0) {
            if existing_key == key {
                continue;
            }
            out.push(',');
            out.push_str(existing_key);
            out.push('=');
            out.push_str(existing_val);
        }
        self.0 = out;
    }

    /// Removes every member with `key`.
    ///
    /// Returns `true` if at least one member was removed.
    pub fn remove(&mut self, key: &str) -> bool {
        let mut out = String::with_capacity(self.0.len());
        let mut removed = false;
        for (existing_key, existing_val) in members(&self.0) {
            if existing_key == key {
                removed = true;
                continue;
            }
            if !out.is_empty() {
                out.push(',');
            }
            out.push_str(existing_key);
            out.push('=');
            out.push_str(existing_val);
        }
        if removed {
            self.0 = out;
        }
        removed
    }

    /// Iterates `(key, value)` members in header order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> + '_ {
        members(&self.0)
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

fn members(header: &str) -> impl Iterator<Item = (&str, &str)> + '_ {
    header.split(',').filter_map(|part| {
        let part = part.trim();
        if part.is_empty() {
            return None;
        }
        let (key, value) = part.split_once('=')?;
        Some((key.trim(), value.trim()))
    })
}

#[cfg(test)]
mod tests {
    use similar_asserts::assert_eq;

    use super::{TraceFlags, TraceState};

    #[test]
    fn trace_flags_retain_unknown_bits_and_remoteness() {
        let flags = TraceFlags::from_bits_retain(0x8000_0001);
        assert!(flags.contains(TraceFlags::SAMPLED));
        assert_eq!(flags.bits() & 0x8000_0000, 0x8000_0000);
        assert_eq!(flags.w3c_byte(), 0x01);
        assert_eq!(flags.context_is_remote(), None);

        let unknown = TraceFlags::from_bits_retain(0x0000_0004);
        assert_eq!(unknown.w3c_byte(), 0x04);
        assert!(!unknown.contains(TraceFlags::SAMPLED));

        let local = TraceFlags::CONTEXT_HAS_IS_REMOTE;
        assert_eq!(local.context_is_remote(), Some(false));

        let remote = TraceFlags::CONTEXT_HAS_IS_REMOTE | TraceFlags::CONTEXT_IS_REMOTE;
        assert_eq!(remote.context_is_remote(), Some(true));

        let remote_without_presence = TraceFlags::CONTEXT_IS_REMOTE;
        assert_eq!(remote_without_presence.context_is_remote(), None);
        assert_eq!(
            remote_without_presence.bits() & TraceFlags::CONTEXT_IS_REMOTE.bits(),
            TraceFlags::CONTEXT_IS_REMOTE.bits()
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
}

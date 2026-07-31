//! Common code shared across Antithesis scenarios. Each scenario crate (e.g.
//! `scenarios/vector_to_vector_e2e_disk`) owns its own test-command bins. When two
//! scenarios need the same HTTP or oracle helpers, factor them into modules here.

/// Largest source payload used by this experiment. Hex encoding doubles this to
/// a 128 KiB JSON field, leaving substantial headroom below disk_v2's 256 KiB
/// write buffer after record framing is added.
const MAX_PAYLOAD_LENGTH: usize = 64 * 1024;

/// Payload lengths in bytes, one per id class. Every generated record remains
/// below the disk_v2 write-buffer size so this workload does not exercise the
/// large-record path.
const PAYLOAD_LENGTHS: [usize; 5] = [0, 1, 4 * 1024, 16 * 1024, MAX_PAYLOAD_LENGTH];

/// Payload length selected for an id.
pub fn payload_length(id: u64) -> usize {
    PAYLOAD_LENGTHS[(id % PAYLOAD_LENGTHS.len() as u64) as usize]
}

/// True for the largest small-record class used by the terminal rollover probe.
pub fn is_rollover_probe_payload(id: u64) -> bool {
    payload_length(id) == MAX_PAYLOAD_LENGTH
}

/// One splitmix64 step. A full-avalanche mixer, so flipping any input bit
/// scrambles the whole output. Seeding the stream with this keyed by id means a
/// length-preserving corruption still changes the bytes the oracle expects.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The exact payload bytes issued for `id`. Deterministic in `id` alone, so the
/// producer regenerates the same record on every retry and the oracle regenerates
/// the same expected bytes with no per-id state to carry. Length comes from the
/// id's class; content is a splitmix64 stream seeded by id.
pub fn payload_for(id: u64) -> Vec<u8> {
    let len = payload_length(id);
    let mut out = Vec::with_capacity(len);
    let mut state = id;
    while out.len() < len {
        let chunk = splitmix64(&mut state).to_le_bytes();
        let take = (len - out.len()).min(chunk.len());
        out.extend_from_slice(&chunk[..take]);
    }
    out
}

/// Hex-encoding of `payload_for(id)`. Hex survives JSON and Vector transport
/// without escaping concerns, and a corruption of the bytes shows up as a hex
/// mismatch.
pub fn payload_field(id: u64) -> String {
    let bytes = payload_for(id);
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from_digit((b >> 4) as u32, 16).unwrap());
        s.push(char::from_digit((b & 0x0f) as u32, 16).unwrap());
    }
    s
}

/// Decode the hex produced by [`payload_field`] back to bytes. Returns `None` on
/// any non-hex or odd-length input so the oracle can tell a mangled field from a
/// content mismatch.
pub fn decode_payload_field(field: &str) -> Option<Vec<u8>> {
    if !field.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(field.len() / 2);
    let mut bytes = field.bytes();
    while let (Some(hi), Some(lo)) = (bytes.next(), bytes.next()) {
        let hi = (hi as char).to_digit(16)?;
        let lo = (lo as char).to_digit(16)?;
        out.push(((hi << 4) | lo) as u8);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_is_deterministic_in_id() {
        for id in 0..32u64 {
            assert_eq!(payload_for(id), payload_for(id));
        }
    }

    #[test]
    fn payload_length_follows_class() {
        for id in 0..PAYLOAD_LENGTHS.len() as u64 {
            assert_eq!(payload_length(id), PAYLOAD_LENGTHS[id as usize]);
            assert_eq!(payload_for(id).len(), PAYLOAD_LENGTHS[id as usize]);
        }
    }

    #[test]
    fn rollover_probe_uses_the_largest_small_record_class() {
        for id in 0..PAYLOAD_LENGTHS.len() as u64 {
            assert_eq!(
                is_rollover_probe_payload(id),
                payload_length(id) == MAX_PAYLOAD_LENGTH
            );
        }
    }

    #[test]
    fn distinct_ids_differ_in_content_at_equal_length() {
        // Ids in the same nonzero-length class but different ids must not produce
        // the same bytes, or a swapped-id corruption would slip past the oracle.
        // Ids 2 and 7 share the same nonempty payload class.
        let a = payload_for(2);
        let b = payload_for(7);
        assert_eq!(a.len(), b.len());
        assert!(!a.is_empty());
        assert_ne!(a, b);
    }

    #[test]
    fn hex_round_trips() {
        for id in 0..32u64 {
            let field = payload_field(id);
            assert_eq!(decode_payload_field(&field).unwrap(), payload_for(id));
        }
    }
}

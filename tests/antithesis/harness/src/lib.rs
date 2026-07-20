//! Common code shared across Antithesis scenarios. Each scenario crate (e.g.
//! `scenarios/vector_to_vector_e2e_disk`) owns its own test-command bins. When two
//! scenarios need the same HTTP or oracle helpers, factor them into modules here.

use std::time::Duration;

use serde_json::json;
use vector_buffers::WRITE_BUFFER_SIZE_V2;

/// Payload lengths in bytes, one per id class. Sized around the disk_v2 write
/// buffer so the produced records straddle the boundary at which the buffer is
/// flushed to the data file: empty, a single byte, fractions of, just under, at,
/// just over, and a record several times larger than the buffer.
const PAYLOAD_LENGTHS: [usize; 8] = [
    0,
    1,
    WRITE_BUFFER_SIZE_V2 / 4,
    WRITE_BUFFER_SIZE_V2 / 2,
    WRITE_BUFFER_SIZE_V2 - 1,
    WRITE_BUFFER_SIZE_V2,
    WRITE_BUFFER_SIZE_V2 + 1,
    768 * 1024,
];

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
    let len = PAYLOAD_LENGTHS[(id % PAYLOAD_LENGTHS.len() as u64) as usize];
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
    hex::encode(payload_for(id))
}

/// Decode the hex produced by [`payload_field`] back to bytes. Returns `None` on
/// any non-hex or odd-length input so the oracle can tell a mangled field from a
/// content mismatch.
pub fn decode_payload_field(field: &str) -> Option<Vec<u8>> {
    hex::decode(field).ok()
}

/// Claim one fresh id from the oracle. `None` if the oracle is unreachable.
pub async fn claim(client: &reqwest::Client, oracle_url: &str) -> Option<u64> {
    let resp = client
        .post(format!("{oracle_url}/claim"))
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .ok()?;
    resp.text().await.ok()?.trim().parse().ok()
}

/// POST one event to the source. `true` on a 2xx, meaning the pipeline took
/// end-to-end responsibility for the event. The payload is a deterministic
/// function of the id, so every retry re-sends the exact same record and the
/// oracle can recompute the expected bytes.
pub async fn post_event(
    client: &reqwest::Client,
    source_url: &str,
    id: u64,
    timeout: Duration,
) -> bool {
    let event = json!([{ "id": id, "data": payload_field(id) }]);
    matches!(
        client.post(source_url).timeout(timeout).json(&event).send().await,
        Ok(resp) if resp.status().is_success()
    )
}

/// Tell the oracle the pipeline acked this id, so it must come back. `true` if
/// the oracle recorded the obligation.
pub async fn report_acked(client: &reqwest::Client, oracle_url: &str, id: u64) -> bool {
    matches!(
        client
            .post(format!("{oracle_url}/acked"))
            .timeout(Duration::from_secs(10))
            .body(id.to_string())
            .send()
            .await,
        Ok(resp) if resp.status().is_success()
    )
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
            assert_eq!(payload_for(id).len(), PAYLOAD_LENGTHS[id as usize]);
        }
    }

    #[test]
    fn distinct_ids_differ_in_content_at_equal_length() {
        // Ids in the same nonzero-length class but different ids must not produce
        // the same bytes, or a swapped-id corruption would slip past the oracle.
        // Ids 2 and 10 share class 2 (a buffer-quarter of bytes).
        let a = payload_for(2);
        let b = payload_for(10);
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

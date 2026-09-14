use serde::{Deserialize, Serialize};

use crate::payload_field;

/// An event submitted by a producer or by the final liveness probe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub id: u64,
    pub data: String,
}

impl Event {
    pub fn for_id(id: u64) -> Self {
        Self {
            id,
            data: payload_field(id),
        }
    }
}

/// The oracle's conservation and integrity verdict at one point in time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OracleReport {
    pub issued: u64,
    pub acked: u64,
    pub delivered: u64,
    pub delivered_total: u64,
    pub missing_count: u64,
    pub missing_sample: Vec<u64>,
    pub spurious_count: u64,
    pub corrupted_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payload_field;

    #[test]
    fn event_uses_the_canonical_payload() {
        let event = Event::for_id(12);
        assert_eq!(event.id, 12);
        assert_eq!(event.data, payload_field(12));
    }

    #[test]
    fn report_round_trips_as_json() {
        let report = OracleReport {
            issued: 4,
            acked: 3,
            delivered: 2,
            delivered_total: 3,
            missing_count: 1,
            missing_sample: vec![2],
            spurious_count: 0,
            corrupted_count: 0,
        };
        let encoded = serde_json::to_string(&report).unwrap();
        assert_eq!(
            serde_json::from_str::<OracleReport>(&encoded).unwrap(),
            report
        );
    }
}

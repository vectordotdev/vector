use std::num::NonZeroU32;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use vector_common::byte_size_of::{self, ByteSizeOf};

use super::{MetricKind, MetricValue};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MetricData {
    #[serde(flatten)]
    pub time: MetricTime,

    pub kind: MetricKind,

    #[serde(flatten)]
    pub value: MetricValue,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct MetricTime {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime<Utc>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval_ms: Option<NonZeroU32>,
}

impl ByteSizeOf for MetricTime {
    fn allocated_bytes(&self) -> usize {
        0
    }

    fn estimated_json_encoded_size_of(&self) -> usize {
        const BRACES_SIZE: usize = 2;
        const COMMA_SIZE: usize = 1;

        let mut size = BRACES_SIZE;

        if let Some(timestamp) = &self.timestamp {
            const TIMESTAMP_KEY_SIZE: usize = 9;
            size += TIMESTAMP_KEY_SIZE + timestamp.estimated_json_encoded_size_of();

            if self.interval_ms.is_some() {
                size += COMMA_SIZE;
            }
        }

        if let Some(interval) = self.interval_ms {
            const INTERVAL_KEY_SIZE: usize = 8;
            size += INTERVAL_KEY_SIZE + interval.get().estimated_json_encoded_size_of();
        }

        size
    }
}

impl MetricData {
    /// Gets a reference to the timestamp for this data, if available.
    pub fn timestamp(&self) -> Option<&DateTime<Utc>> {
        self.time.timestamp.as_ref()
    }

    /// Gets a reference to the value of this data.
    pub fn value(&self) -> &MetricValue {
        &self.value
    }

    /// Gets a mutable reference to the value of this data.
    pub fn value_mut(&mut self) -> &mut MetricValue {
        &mut self.value
    }

    /// Consumes this metric, returning it as an absolute metric.
    ///
    /// If the metric was already absolute, nothing is changed.
    #[must_use]
    pub fn into_absolute(self) -> Self {
        Self {
            time: self.time,
            kind: MetricKind::Absolute,
            value: self.value,
        }
    }

    /// Consumes this metric, returning it as an incremental metric.
    ///
    /// If the metric was already incremental, nothing is changed.
    #[must_use]
    pub fn into_incremental(self) -> Self {
        Self {
            time: self.time,
            kind: MetricKind::Incremental,
            value: self.value,
        }
    }

    /// Creates a `MetricData` directly from the raw components of another `MetricData`.
    pub fn from_parts(time: MetricTime, kind: MetricKind, value: MetricValue) -> Self {
        Self { time, kind, value }
    }

    /// Decomposes a `MetricData` into its individual parts.
    pub fn into_parts(self) -> (MetricTime, MetricKind, MetricValue) {
        (self.time, self.kind, self.value)
    }

    /// Updates this metric by adding the value from `other`.
    #[must_use]
    pub fn update(&mut self, other: &Self) -> bool {
        let (new_ts, new_interval) = match (
            self.time.timestamp,
            self.time.interval_ms,
            other.time.timestamp,
            other.time.interval_ms,
        ) {
            (Some(t1), Some(i1), Some(t2), Some(i2)) => {
                let delta_t = match TryInto::<u32>::try_into(
                    t1.timestamp_millis().abs_diff(t2.timestamp_millis()),
                ) {
                    Ok(delta_t) => delta_t,
                    Err(_) => return false,
                };

                if t1 > t2 {
                    // The interval window starts from the beginning of `other` (aka `t2`)
                    // and goes to the end of `self` (which is `t1 + i1`).
                    (Some(t2), NonZeroU32::new(delta_t + i1.get()))
                } else {
                    // The interval window starts from the beginning of `self` (aka `t1`)
                    // and goes to the end of `other` (which is `t2 + i2`).

                    (Some(t1), NonZeroU32::new(delta_t + i2.get()))
                }
            }
            (Some(t), _, None, _) | (None, _, Some(t), _) => (Some(t), None),
            (Some(t1), _, Some(t2), _) => (Some(t1.max(t2)), None),
            (_, _, _, _) => (None, None),
        };

        self.value.add(&other.value) && {
            self.time.timestamp = new_ts;
            self.time.interval_ms = new_interval;
            true
        }
    }

    /// Adds the data from the `other` metric to this one.
    ///
    /// The other metric must be incremental and contain the same value type as this one.
    #[must_use]
    pub fn add(&mut self, other: &Self) -> bool {
        other.kind == MetricKind::Incremental && self.update(other)
    }

    /// Subtracts the data from the `other` metric from this one.
    ///
    /// The other metric must contain the same value type as this one.
    #[must_use]
    pub fn subtract(&mut self, other: &Self) -> bool {
        self.value.subtract(&other.value)
    }

    /// Zeroes out the data in this metric.
    pub fn zero(&mut self) {
        self.value.zero();
    }
}

impl AsRef<MetricData> for MetricData {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl PartialOrd for MetricData {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.time.timestamp.partial_cmp(&other.time.timestamp)
    }
}

impl ByteSizeOf for MetricData {
    fn allocated_bytes(&self) -> usize {
        self.value.allocated_bytes()
    }

    fn estimated_json_encoded_size_of(&self) -> usize {
        const BRACES_SIZE: usize = 2;
        const COMMA_SIZE: usize = 1;
        const COLON_SIZE: usize = 1;
        const KIND_KEY_SIZE: usize = 4;

        let mut size = BRACES_SIZE;

        let time_size = self.time.estimated_json_encoded_size_of();

        // It could be an empty object, which gets flattened into nothing.
        if time_size > 2 {
            // Flattening, so no need for nested braces.
            size += time_size - BRACES_SIZE + COMMA_SIZE;
        }

        size + self.value.estimated_json_encoded_size_of() - BRACES_SIZE
            + COMMA_SIZE
            + byte_size_of::string_like_estimated_json_byte_size(KIND_KEY_SIZE)
            + COLON_SIZE
            + self.kind.as_str().len()
    }
}

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use vector_common::byte_size_of::ByteSizeOf;

use super::{MetricKind, MetricValue};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MetricData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime<Utc>>,

    pub kind: MetricKind,

    #[serde(flatten)]
    pub value: MetricValue,
}

impl MetricData {
    /// Gets a reference to the timestamp for this data, if available.
    pub fn timestamp(&self) -> Option<&DateTime<Utc>> {
        self.timestamp.as_ref()
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
            timestamp: self.timestamp,
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
            timestamp: self.timestamp,
            kind: MetricKind::Incremental,
            value: self.value,
        }
    }

    /// Creates a `MetricData` directly from the raw components of another `MetricData`.
    pub fn from_parts(
        timestamp: Option<DateTime<Utc>>,
        kind: MetricKind,
        value: MetricValue,
    ) -> Self {
        Self {
            timestamp,
            kind,
            value,
        }
    }

    /// Decomposes a `MetricData` into its individual parts.
    pub fn into_parts(self) -> (Option<DateTime<Utc>>, MetricKind, MetricValue) {
        (self.timestamp, self.kind, self.value)
    }

    /// Updates this metric by adding the value from `other`.
    #[must_use]
    pub fn update(&mut self, other: &Self) -> bool {
        self.value.add(&other.value) && {
            // Update the timestamp to the latest one
            self.timestamp = match (self.timestamp, other.timestamp) {
                (None, None) => None,
                (Some(t), None) | (None, Some(t)) => Some(t),
                (Some(t1), Some(t2)) => Some(t1.max(t2)),
            };
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
        self.timestamp.partial_cmp(&other.timestamp)
    }
}

impl ByteSizeOf for MetricData {
    fn allocated_bytes(&self) -> usize {
        self.value.allocated_bytes()
    }
}

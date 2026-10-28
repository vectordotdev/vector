//! Parsing SQS notifications into batches of S3 objects.

use chrono::{DateTime, Utc};

/// Identifies an S3 object by bucket and key, with its expected region.
#[derive(Debug)]
pub struct S3ObjectReference {
    pub(super) bucket: String,
    pub(super) key: String,
    pub(super) region: String,
}

impl S3ObjectReference {
    /// Create a reference with a key used as-is, without URL decoding.
    /// `region` must match the source's configured region.
    #[must_use]
    pub const fn new(bucket: String, key: String, region: String) -> Self {
        Self {
            bucket,
            key,
            region,
        }
    }

    /// Bucket containing the object.
    #[must_use]
    pub fn bucket(&self) -> &str {
        &self.bucket
    }

    /// Object key, ready to pass to S3.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Region of the referenced object.
    #[must_use]
    pub fn region(&self) -> &str {
        &self.region
    }
}

/// S3 objects processed together, in order, with one age check if deferral is enabled.
/// A batch does not have its own SQS receipt.
#[derive(Debug)]
pub struct NotificationBatch {
    pub(super) objects: Vec<S3ObjectReference>,
    pub(super) published_at: Option<DateTime<Utc>>,
}

impl NotificationBatch {
    /// Group objects with an optional publication time.
    ///
    /// Deferral requires a timestamp for nonempty batches. The source validates
    /// all regions, then checks age once before reading the objects. Empty batches
    /// are skipped. Use separate batches for records that need separate age checks.
    #[must_use]
    pub const fn new(objects: Vec<S3ObjectReference>, published_at: Option<DateTime<Utc>>) -> Self {
        Self {
            objects,
            published_at,
        }
    }

    /// Objects in processing order.
    #[must_use]
    pub fn objects(&self) -> &[S3ObjectReference] {
        &self.objects
    }

    /// Notification time used for deferral, not for timestamps of downloaded events.
    #[must_use]
    pub const fn published_at(&self) -> Option<DateTime<Utc>> {
        self.published_at
    }
}

/// Batches from one notification, processed in order until an error occurs.
///
/// The parser prepares all results before returning. The source processes each
/// batch before starting the next. Return an error as the first item if the whole
/// message is invalid. An empty list means no work.
pub type NotificationBatches =
    Vec<Result<NotificationBatch, Box<dyn std::error::Error + Send + Sync>>>;

/// Parses SQS notifications into batches of S3 objects.
///
/// Replaces the built-in S3/SNS parsing, including any message wrappers.
/// Parsing should not make network requests.
///
/// If a later batch fails the age check, the entire original SQS message is
/// forwarded to the deferred queue, so earlier batches may be delivered again.
pub trait NotificationParser: Send + Sync {
    /// Parse the original SQS message body.
    /// On error, the source does not delete the message. Retries may repeat
    /// objects that were already delivered.
    fn parse(&self, body: &str) -> NotificationBatches;
}

#[cfg(test)]
mod tests;

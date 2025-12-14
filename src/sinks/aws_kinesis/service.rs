use std::{
    marker::PhantomData,
    task::{Context, Poll},
};

use aws_smithy_runtime_api::client::{orchestrator::HttpResponse, result::SdkError};
use aws_types::region::Region;

use super::{
    record::{Record, SendRecord},
    sink::BatchKinesisRequest,
};
use crate::{event::EventStatus, sinks::prelude::*};

pub struct KinesisService<C, T, E> {
    pub client: C,
    pub stream_name: String,
    pub region: Option<Region>,
    pub _phantom_t: PhantomData<T>,
    pub _phantom_e: PhantomData<E>,
}

impl<C, T, E> Clone for KinesisService<C, T, E>
where
    C: Clone,
{
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            stream_name: self.stream_name.clone(),
            region: self.region.clone(),
            _phantom_e: self._phantom_e,
            _phantom_t: self._phantom_t,
        }
    }
}

pub struct KinesisResponse {
    pub(crate) failure_count: usize,
    pub(crate) events_byte_size: GroupedCountByteSize,
    #[cfg(feature = "sinks-aws_kinesis_streams")]
    /// Track individual failed records for retry logic (Streams only)
    pub(crate) failed_records: Vec<RecordResult>,
}

#[derive(Clone)]
pub struct RecordResult {
    pub index: usize, // Original position in batch
    pub success: bool,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

impl DriverResponse for KinesisResponse {
    fn event_status(&self) -> EventStatus {
        if self.failure_count > 0
            && let GroupedCountByteSize::Untagged { size } = &self.events_byte_size
            && size.0 == 0
        {
            // If there are failures and no successful events, return Rejected
            // This happens when retries are exhausted and all events failed
            return EventStatus::Rejected;
        }
        // Either no failures, or partial success (some events succeeded)
        EventStatus::Delivered
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.events_byte_size
    }

    fn events_rejected(&self) -> Option<usize> {
        if self.failure_count > 0 {
            // Report the number of failed events for partial failure tracking
            // This allows the driver to emit component_discarded_events_total
            // for partial failures while still counting successful events
            Some(self.failure_count)
        } else {
            None
        }
    }
}

impl<R, C, T, E> Service<BatchKinesisRequest<R>> for KinesisService<C, T, E>
where
    R: Record<T = T> + Clone,
    C: SendRecord + Clone + Sync + Send + 'static,
    Vec<<C as SendRecord>::T>: FromIterator<T>,
    <C as SendRecord>::T: Send,
{
    type Response = KinesisResponse;
    type Error = SdkError<<C as SendRecord>::E, HttpResponse>;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    // Emission of an internal event in case of errors is handled upstream by the caller.
    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Emission of internal events for errors and dropped events is handled upstream by the caller.
    fn call(&mut self, requests: BatchKinesisRequest<R>) -> Self::Future {
        let estimated_json_encoded_sizes: Vec<usize> = requests
            .events
            .iter()
            .map(
                |req| match req.get_metadata().events_estimated_json_encoded_byte_size() {
                    GroupedCountByteSize::Untagged { size } => size.1.get(),
                    GroupedCountByteSize::Tagged { .. } => 0,
                },
            )
            .collect();

        let records = requests
            .events
            .into_iter()
            .map(|req| req.record.get())
            .collect();

        let client = self.client.clone();
        let stream_name = self.stream_name.clone();

        Box::pin(async move {
            let mut response = client.send(records, stream_name).await?;

            // Calculate the byte size for successful events only
            let successful_size: usize = {
                if response.failed_records.is_empty() {
                    // Fast path: no failures, sum all sizes
                    estimated_json_encoded_sizes.iter().sum()
                } else {
                    // Build a HashSet of failed indices for O(1) lookup
                    use std::collections::HashSet;
                    let failed_indices: HashSet<usize> =
                        response.failed_records.iter().map(|fr| fr.index).collect();

                    // Sum sizes for all non-failed records
                    estimated_json_encoded_sizes
                        .iter()
                        .enumerate()
                        .filter_map(|(index, size)| {
                            if failed_indices.contains(&index) {
                                None
                            } else {
                                Some(size)
                            }
                        })
                        .sum()
                }
            };

            let successful_count = estimated_json_encoded_sizes.len() - response.failure_count;
            response.events_byte_size =
                CountByteSize(successful_count, JsonSize::new(successful_size)).into();

            Ok(response)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventStatus;
    use vector_lib::{internal_event::CountByteSize, json_size::JsonSize};

    #[test]
    fn test_event_status_delivered_with_partial_success() {
        // Scenario: 10 events sent, 7 succeeded, 3 failed
        // events_byte_size reflects only the 7 successful events
        #[cfg(feature = "sinks-aws_kinesis_streams")]
        let failed_records = vec![
            RecordResult {
                index: 3,
                success: false,
                error_code: Some("ProvisionedThroughputExceededException".to_string()),
                error_message: Some("Rate exceeded".to_string()),
            },
            RecordResult {
                index: 7,
                success: false,
                error_code: Some("ProvisionedThroughputExceededException".to_string()),
                error_message: Some("Rate exceeded".to_string()),
            },
            RecordResult {
                index: 9,
                success: false,
                error_code: Some("InternalFailure".to_string()),
                error_message: Some("Internal error".to_string()),
            },
        ];

        let response = KinesisResponse {
            failure_count: 3,
            events_byte_size: CountByteSize(7, JsonSize::new(700)).into(), // Only 7 successful
            #[cfg(feature = "sinks-aws_kinesis_streams")]
            failed_records,
        };

        assert_eq!(
            response.event_status(),
            EventStatus::Delivered,
            "Partial success should return Delivered (successful events are delivered)"
        );

        // Verify events_rejected reports the failed events
        assert_eq!(
            response.events_rejected(),
            Some(3),
            "events_rejected should report 3 failed events"
        );

        // Verify events_sent reflects only successful events
        if let GroupedCountByteSize::Untagged { size } = response.events_sent() {
            assert_eq!(size.0, 7, "events_sent should report 7 successful events");
        } else {
            panic!("Expected Untagged variant");
        }
    }

    #[test]
    fn test_event_status_delivered_when_no_failures() {
        let response = KinesisResponse {
            failure_count: 0,
            events_byte_size: CountByteSize(10, JsonSize::new(1000)).into(),
            #[cfg(feature = "sinks-aws_kinesis_streams")]
            failed_records: vec![],
        };

        assert_eq!(
            response.event_status(),
            EventStatus::Delivered,
            "Response with no failures should return Delivered status"
        );

        assert_eq!(
            response.events_rejected(),
            None,
            "events_rejected should not report failed events"
        );

        if let GroupedCountByteSize::Untagged { size } = response.events_sent() {
            assert_eq!(size.0, 10, "events_sent should report 10 successful events");
            assert_eq!(size.1.get(), 1000, "events_sent should report 1000 bytes");
        } else {
            panic!("Expected Untagged variant");
        }
    }

    #[test]
    fn test_event_status_rejected_when_total_failure() {
        // Scenario: All events failed
        #[cfg(feature = "sinks-aws_kinesis_streams")]
        let failed_records = vec![
            RecordResult {
                index: 0,
                success: false,
                error_code: Some("ProvisionedThroughputExceededException".to_string()),
                error_message: Some("Rate exceeded".to_string()),
            },
            RecordResult {
                index: 1,
                success: false,
                error_code: Some("ProvisionedThroughputExceededException".to_string()),
                error_message: Some("Rate exceeded".to_string()),
            },
        ];

        let response = KinesisResponse {
            failure_count: 2,
            events_byte_size: CountByteSize(0, JsonSize::new(0)).into(), // No successes
            #[cfg(feature = "sinks-aws_kinesis_streams")]
            failed_records,
        };

        assert_eq!(
            response.event_status(),
            EventStatus::Rejected,
            "Total failure (no successful events) should return Rejected status"
        );

        assert_eq!(
            response.events_rejected(),
            Some(2),
            "events_rejected should report 0 failed event"
        );

        // Verify events_sent is empty
        if let GroupedCountByteSize::Untagged { size } = response.events_sent() {
            assert_eq!(size.0, 0, "events_sent should report 0 successful event");
            assert_eq!(size.1.get(), 0, "events_sent should report 0 byte");
        } else {
            panic!("Expected Untagged variant");
        }
    }
}

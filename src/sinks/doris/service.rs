use crate::sinks::{
    doris::{
        client::{DorisStreamLoadResponse, StreamLoadStatus, ThreadSafeDorisSinkClient},
        sink::DorisPartitionKey,
    },
    prelude::{BoxFuture, DriverResponse, Service},
    util::http::HttpRequest,
};
use bytes::Bytes;
use http::Response;
use serde_json;
use snafu::Snafu;
use std::{
    sync::Arc,
    task::{Context, Poll},
};
use tracing::info;
use vector_common::{
    finalization::EventStatus,
    request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata},
};

#[derive(Clone)]
pub struct DorisService {
    client: ThreadSafeDorisSinkClient,
    log_request: bool,
    reporter: Arc<super::progress::ProgressReporter>,
}

impl DorisService {
    pub fn new(
        client: ThreadSafeDorisSinkClient,
        log_request: bool,
        reporter: Arc<super::progress::ProgressReporter>,
    ) -> DorisService {
        DorisService {
            client,
            log_request,
            reporter,
        }
    }
    pub(crate) async fn reporter_run(
        &self,
        log_request: bool,
        response: DorisStreamLoadResponse,
    ) -> Result<(), crate::Error> {
        let reporter = Arc::clone(&self.reporter);
        let stream_load_status = response.stream_load_status;
        let http_status_code = response.http_status_code;
        let response_json = response.response_json;
        if log_request {
            // Format the JSON with proper indentation for better readability
            let formatted_json = match serde_json::to_string_pretty(&response_json) {
                Ok(pretty_json) => pretty_json,
                Err(err) => {
                    // Log the error but continue with the original format
                    tracing::warn!(message = "Failed to prettify JSON response", error = %err);
                    response_json.to_string()
                }
            };

            info!(
                message = "Doris stream load response received.",
                status_code = %http_status_code,
                stream_load_status = %stream_load_status,
                response = %formatted_json
            );
        }
        if http_status_code.is_success() {
            if stream_load_status == StreamLoadStatus::Successful {
                // Update byte count statistics
                if let Some(load_bytes) = response_json.get("LoadBytes").and_then(|b| b.as_i64()) {
                    reporter.incr_total_bytes(load_bytes);
                }
                // Update row count statistics
                if let Some(loaded_rows) = response_json
                    .get("NumberLoadedRows")
                    .and_then(|r| r.as_i64())
                {
                    reporter.incr_total_rows(loaded_rows);
                }
                // Update filtered row count statistics
                if let Some(filtered_rows) = response_json
                    .get("NumberFilteredRows")
                    .and_then(|r| r.as_i64())
                {
                    if filtered_rows > 0 {
                        reporter.incr_failed_rows(filtered_rows);
                    }
                }
            }
        }
        Ok(())
    }
}
#[derive(Debug, Snafu)]
pub struct DorisResponse {
    pub metadata: RequestMetadata,
    pub http_response: Response<Bytes>,
    pub event_status: EventStatus,
}

impl DriverResponse for DorisResponse {
    fn event_status(&self) -> EventStatus {
        self.event_status
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        self.metadata.events_estimated_json_encoded_byte_size()
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.metadata.request_encoded_size())
    }
}

impl Service<HttpRequest<DorisPartitionKey>> for DorisService {
    type Response = DorisResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: HttpRequest<DorisPartitionKey>) -> Self::Future {
        let log_request = self.log_request;
        let service = self.clone();

        let future = async move {
            let mut request = req;
            let table = request.get_additional_metadata().table.clone();
            let database = request.get_additional_metadata().database.clone();
            let doris_response = service
                .client
                .send_stream_load(database, table, request.take_payload())
                .await?;
            let report_response = doris_response.clone();
            let _ = service.reporter_run(log_request, report_response).await;

            let event_status = if doris_response.stream_load_status == StreamLoadStatus::Successful
            {
                EventStatus::Delivered
            } else {
                EventStatus::Errored
            };

            Ok(DorisResponse {
                metadata: request.get_metadata().clone(),
                http_response: doris_response.response,
                event_status,
            })
        };
        Box::pin(future)
    }
}

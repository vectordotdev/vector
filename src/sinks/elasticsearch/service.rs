use std::{
    sync::Arc,
    task::{Context, Poll},
};

use bytes::Bytes;
use futures::future::BoxFuture;
use http::{Response, Uri};
use hyper::{Body, Request, service::Service};
use tower::ServiceExt;
use vector_lib::{
    ByteSizeOf,
    internal_event::{ComponentEventsDropped, UNINTENTIONAL},
    json_size::JsonSize,
    request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata},
    stream::DriverResponse,
};

use super::{ElasticsearchCommon, ElasticsearchConfig, retry::EsResultResponse};
use crate::{
    event::{Event, EventFinalizers, EventStatus, Finalizable},
    http::HttpClient,
    sinks::{
        elasticsearch::{encoder::ProcessedEvent, request_builder::ElasticsearchRequestBuilder},
        util::{
            Compression, ElementCount, SinkDlq,
            auth::Auth,
            http::{HttpBatchService, RequestConfig},
        },
    },
};

#[derive(Clone, Debug)]
pub struct ElasticsearchRequest {
    pub payload: Bytes,
    pub finalizers: EventFinalizers,
    pub batch_size: usize,
    pub events_byte_size: JsonSize,
    pub metadata: RequestMetadata,
    pub original_events: Arc<Vec<ProcessedEvent>>, // store original_events for reconstruct request when retrying
    pub elasticsearch_request_builder: ElasticsearchRequestBuilder,
}

impl ByteSizeOf for ElasticsearchRequest {
    fn allocated_bytes(&self) -> usize {
        self.payload.allocated_bytes() + self.finalizers.allocated_bytes()
    }
}

impl ElementCount for ElasticsearchRequest {
    fn element_count(&self) -> usize {
        self.batch_size
    }
}

impl Finalizable for ElasticsearchRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl MetaDescriptive for ElasticsearchRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

#[derive(Clone)]
pub struct ElasticsearchService {
    // TODO: `HttpBatchService` has been deprecated for direct use in sinks.
    //       This sink should undergo a refactor to utilize the `HttpService`
    //       instead, which extracts much of the boilerplate code for `Service`.
    batch_service: HttpBatchService<
        BoxFuture<'static, Result<http::Request<Bytes>, crate::Error>>,
        ElasticsearchRequest,
    >,
    dlq: Option<SinkDlq>,
    request_retry_partial: bool,
}

impl ElasticsearchService {
    pub fn new(
        http_client: HttpClient<Body>,
        http_request_builder: HttpRequestBuilder,
        request_retry_partial: bool,
        dlq: Option<SinkDlq>,
    ) -> ElasticsearchService {
        let http_request_builder = Arc::new(http_request_builder);
        let batch_service = HttpBatchService::new(http_client, move |req| {
            let request_builder = Arc::clone(&http_request_builder);
            let future: BoxFuture<'static, Result<http::Request<Bytes>, crate::Error>> =
                Box::pin(async move { request_builder.build_request(req).await });
            future
        });
        ElasticsearchService {
            batch_service,
            dlq,
            request_retry_partial,
        }
    }
}

pub struct HttpRequestBuilder {
    pub bulk_uri: Uri,
    pub auth: Option<Auth>,
    pub service_type: crate::sinks::elasticsearch::OpenSearchServiceType,
    pub compression: Compression,
    pub http_request_config: RequestConfig,
}

#[derive(Clone, Copy)]
struct BulkFailureSummary {
    had_failed_items: bool,
    has_unhandled_failures: bool,
    dlq_eligible_failures: usize,
}

impl BulkFailureSummary {
    const fn handled_all_failures(self) -> bool {
        self.had_failed_items && !self.has_unhandled_failures
    }
}

fn is_status_dlq_eligible(status: http::StatusCode, request_retry_partial: bool) -> (bool, bool) {
    let is_failed = !status.is_success();
    let is_retryable = status == http::StatusCode::TOO_MANY_REQUESTS || status.is_server_error();
    let should_dlq = if request_retry_partial {
        is_failed && !is_retryable
    } else {
        is_failed
    };

    (is_failed, should_dlq)
}

fn summarize_bulk_failures(
    statuses: &[(http::StatusCode, Option<&super::retry::EsErrorDetails>)],
    request_retry_partial: bool,
    original_events_len: usize,
) -> BulkFailureSummary {
    let mut summary = BulkFailureSummary {
        had_failed_items: false,
        has_unhandled_failures: statuses.len() != original_events_len,
        dlq_eligible_failures: 0,
    };

    for (status, _error) in statuses.iter().copied() {
        let (is_failed, should_dlq) = is_status_dlq_eligible(status, request_retry_partial);
        summary.had_failed_items |= is_failed;

        if is_failed && !should_dlq {
            summary.has_unhandled_failures = true;
        }

        if should_dlq {
            summary.dlq_eligible_failures += 1;
        }
    }

    summary
}

impl HttpRequestBuilder {
    pub fn new(common: &ElasticsearchCommon, config: &ElasticsearchConfig) -> HttpRequestBuilder {
        HttpRequestBuilder {
            bulk_uri: common.bulk_uri.clone(),
            auth: common.auth.clone(),
            service_type: common.service_type.clone(),
            compression: config.compression,
            http_request_config: config.request.clone(),
        }
    }

    pub async fn build_request(
        &self,
        es_req: ElasticsearchRequest,
    ) -> Result<Request<Bytes>, crate::Error> {
        let mut builder = Request::post(&self.bulk_uri);

        builder = builder.header("Content-Type", "application/x-ndjson");

        if let Some(ce) = self.compression.content_encoding() {
            builder = builder.header("Content-Encoding", ce);
        }

        if let Some(ae) = self.compression.accept_encoding() {
            builder = builder.header("Accept-Encoding", ae);
        }

        for (header, value) in &self.http_request_config.headers {
            builder = builder.header(&header[..], &value[..]);
        }

        let mut request = builder
            .body(es_req.payload)
            .expect("Invalid http request value used");

        if let Some(auth) = &self.auth {
            match auth {
                Auth::Basic(auth) => {
                    auth.apply(&mut request);
                }
                #[cfg(feature = "aws-core")]
                Auth::Aws {
                    credentials_provider: provider,
                    region,
                } => {
                    crate::sinks::elasticsearch::sign_request(
                        &self.service_type,
                        &mut request,
                        provider,
                        Some(region),
                    )
                    .await?;
                }
            }
        }

        Ok(request)
    }
}

/// Processes a successful bulk response with partial failures, routing DLQ-eligible events to
/// the sink's DLQ output port and returning whether all failures were accounted for.
///
/// Returns `true` if every failure was either sent to the DLQ or is a retryable error that
/// will be handled by the retry logic (e.g. 429 when `request_retry_partial` is enabled).
async fn maybe_emit_dlq(
    dlq: &mut Option<SinkDlq>,
    original_events: &[ProcessedEvent],
    parsed: &EsResultResponse,
    request_retry_partial: bool,
) -> Result<bool, crate::Error> {
    let statuses: Vec<_> = parsed.iter_status().collect();
    let orphan_count = original_events.len().saturating_sub(statuses.len());
    let summary = summarize_bulk_failures(&statuses, request_retry_partial, original_events.len());

    // No DLQ connected: emit a dropped-events metric for any events we can't route elsewhere.
    if dlq.is_none() {
        let dropped = summary.dlq_eligible_failures + orphan_count;
        if dropped > 0 {
            let reason = "elasticsearch partial failures had no connected dlq output";
            emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                count: dropped,
                reason,
            });
            // Return false so get_event_status yields EventStatus::Rejected, making these
            // failures visible as errors in vector top / topology metrics.
            return Ok(false);
        }
        return Ok(summary.handled_all_failures());
    }

    if summary.dlq_eligible_failures == 0 && orphan_count == 0 {
        return Ok(summary.handled_all_failures());
    }

    let d = dlq.as_mut().expect("dlq is Some, checked above");
    let mut dlq_events = Vec::new();

    // Collect events whose ES response status is non-retryable.
    for (event, (status, error)) in original_events.iter().zip(statuses.iter().copied()) {
        let (_, should_dlq) = is_status_dlq_eligible(status, request_retry_partial);
        if should_dlq {
            let mut event = event.clone();
            let mut details = serde_json::Map::new();
            details.insert("status".to_string(), status.as_u16().into());
            details.insert("retry_partial".to_string(), request_retry_partial.into());
            if let Some(error) = error {
                details.insert("error_type".to_string(), error.err_type.clone().into());
                details.insert("error_reason".to_string(), error.reason.clone().into());
            }
            d.annotate_log(&mut event.log, "elasticsearch_bulk_rejected", details);
            dlq_events.push(Event::from(event.log));
        }
    }

    // Collect orphaned events: ES returned fewer items than events sent (malformed response).
    for event in &original_events[statuses.len()..] {
        let mut event = event.clone();
        d.annotate_log(
            &mut event.log,
            "elasticsearch_missing_response_item",
            serde_json::Map::new(),
        );
        dlq_events.push(Event::from(event.log));
    }

    if !dlq_events.is_empty() {
        d.send_events(dlq_events).await?;
    }

    // Return true if the DLQ actually handled something (DLQ-eligible failures or orphans).
    // Retryable failures (429, 5xx when retry_partial=true) are handled independently by the
    // tower retry layer
    Ok(summary.dlq_eligible_failures > 0 || orphan_count > 0)
}

pub struct ElasticsearchResponse {
    pub http_response: Response<Bytes>,
    pub event_status: EventStatus,
    pub events_byte_size: GroupedCountByteSize,
}

impl DriverResponse for ElasticsearchResponse {
    fn event_status(&self) -> EventStatus {
        self.event_status
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.events_byte_size
    }
}

impl Service<ElasticsearchRequest> for ElasticsearchService {
    type Response = ElasticsearchResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    // Emission of an internal event in case of errors is handled upstream by the caller.
    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Emission of internal events for errors and dropped events is handled upstream by the caller.
    fn call(&mut self, mut req: ElasticsearchRequest) -> Self::Future {
        let mut http_service = self.batch_service.clone();
        let mut dlq = self.dlq.clone();
        let request_retry_partial = self.request_retry_partial;
        Box::pin(async move {
            http_service.ready().await?;
            let events_byte_size =
                std::mem::take(req.metadata_mut()).into_events_estimated_json_encoded_byte_size();
            let original_events = Arc::clone(&req.original_events);
            let http_response = http_service.call(req).await?;
            let parsed_bulk_response = if http_response.status().is_success() {
                let body = String::from_utf8_lossy(http_response.body());
                if body.contains("\"errors\":true") {
                    EsResultResponse::parse(&body).ok()
                } else {
                    None
                }
            } else {
                None
            };

            let dlq_handled_all_failures = match &parsed_bulk_response {
                Some(parsed) => {
                    maybe_emit_dlq(&mut dlq, &original_events, parsed, request_retry_partial)
                        .await?
                }
                None => false,
            };

            let event_status = get_event_status(
                &http_response,
                dlq_handled_all_failures,
                parsed_bulk_response.as_ref(),
            );
            Ok(ElasticsearchResponse {
                event_status,
                http_response,
                events_byte_size,
            })
        })
    }
}

// This event is not part of the event framework but is kept because some users were depending on it
// to identify the number of errors returned by Elasticsearch. It can be dropped when we have better
// telemetry. Ref: #15886
fn emit_bad_response_error(response: &Response<Bytes>, parsed: Option<&EsResultResponse>) {
    let error_code = format!("http_response_{}", response.status().as_u16());
    let mut item_status = None;
    let mut error_type = None;
    let mut error_reason = None;

    if let Some(parsed) = parsed
        && let Some((status, details)) = parsed
            .iter_status()
            .find(|(status, _)| !status.is_success())
    {
        item_status = Some(status.as_u16());
        if let Some(details) = details {
            error_type = Some(details.err_type.clone());
            error_reason = Some(details.reason.clone());
        }
    }

    error!(
        message = "Response contained errors.",
        error_code = error_code,
        http_status = response.status().as_u16(),
        body_bytes = response.body().len(),
        item_status,
        error_type,
        error_reason,
    );
}

fn get_event_status(
    response: &Response<Bytes>,
    dlq_handled_all_failures: bool,
    parsed_body: Option<&EsResultResponse>,
) -> EventStatus {
    let status = response.status();
    if status.is_success() {
        if parsed_body.is_some() {
            // `"errors":true` was found and parsed; log error details from the pre-parsed body.
            emit_bad_response_error(response, parsed_body);
            if dlq_handled_all_failures {
                EventStatus::Delivered
            } else {
                EventStatus::Rejected
            }
        } else {
            EventStatus::Delivered
        }
    } else if status.is_server_error() {
        emit_bad_response_error(response, None);
        EventStatus::Errored
    } else {
        emit_bad_response_error(response, None);
        EventStatus::Rejected
    }
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;
    use http::StatusCode;
    use vector_lib::{
        event::into_event_stream, request_metadata::RequestMetadata, source_sender::SourceSender,
    };
    use vrl::event_path;

    use super::*;
    use crate::{
        config::{ComponentKey, SinkContext},
        sinks::elasticsearch::{
            BulkAction,
            encoder::{DocumentMetadata, ElasticsearchEncoder},
        },
    };

    fn processed_event(message: &str) -> ProcessedEvent {
        ProcessedEvent {
            index: "vector".to_string(),
            bulk_action: BulkAction::Index,
            log: crate::event::LogEvent::from(message),
            document_metadata: DocumentMetadata::WithoutId,
        }
    }

    fn request(events: Vec<ProcessedEvent>) -> ElasticsearchRequest {
        ElasticsearchRequest {
            payload: Bytes::new(),
            finalizers: EventFinalizers::default(),
            batch_size: events.len(),
            events_byte_size: JsonSize::zero(),
            metadata: RequestMetadata::default(),
            original_events: Arc::new(events),
            elasticsearch_request_builder: ElasticsearchRequestBuilder {
                compression: Compression::None,
                encoder: ElasticsearchEncoder::default(),
            },
        }
    }

    fn make_dlq(output: SourceSender) -> Option<SinkDlq> {
        SinkDlq::from_context(
            &SinkContext {
                key: Some(ComponentKey::from("elastic")),
                outputs: Some(output),
                ..Default::default()
            },
            "elasticsearch",
        )
    }

    fn dlq_sender() -> (SourceSender, impl futures::Stream<Item = Event> + Unpin) {
        let mut builder = SourceSender::builder().with_buffer(10);
        let rx = builder.add_sink_output(SinkDlq::log_output(), "elastic".into());
        let sender = builder.build();
        let stream = rx.into_stream().flat_map(into_event_stream);
        (sender, stream)
    }

    fn parse_response_body(response: &http::Response<Bytes>) -> EsResultResponse {
        let body = String::from_utf8_lossy(response.body());
        EsResultResponse::parse(&body).expect("test response body must be valid ES response JSON")
    }

    #[tokio::test]
    async fn emits_only_non_retryable_partial_failures_to_dlq() {
        let (output, mut stream) = dlq_sender();
        let mut dlq = make_dlq(output);
        let req = request(vec![processed_event("first"), processed_event("second")]);
        let response = http::Response::builder()
            .status(StatusCode::OK)
            .body(Bytes::from(
                r#"{"errors":true,"items":[{"index":{"status":400,"error":{"type":"mapper_parsing_exception","reason":"bad mapping"}}},{"index":{"status":429}}]}"#,
            ))
            .unwrap();

        let parsed = parse_response_body(&response);
        assert!(
            maybe_emit_dlq(&mut dlq, &req.original_events, &parsed, true)
                .await
                .unwrap()
        );
        drop(dlq);

        let event = stream.next().await.unwrap().into_log();
        assert_eq!(event.get_message().unwrap().to_string_lossy(), "first");
        assert_eq!(
            event.get(event_path!("metadata", "dlq", "reason")).unwrap(),
            &"elasticsearch_bulk_rejected".into()
        );
        assert_eq!(
            event.get(event_path!("metadata", "dlq", "status")).unwrap(),
            &400.into()
        );
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn emits_all_failed_items_to_dlq_when_partial_retry_disabled() {
        let (output, mut stream) = dlq_sender();
        let mut dlq = make_dlq(output);
        let req = request(vec![processed_event("first"), processed_event("second")]);
        let response = http::Response::builder()
            .status(StatusCode::OK)
            .body(Bytes::from(
                r#"{"errors":true,"items":[{"index":{"status":400,"error":{"type":"mapper_parsing_exception","reason":"bad mapping"}}},{"index":{"status":429}}]}"#,
            ))
            .unwrap();

        let parsed = parse_response_body(&response);
        assert!(
            maybe_emit_dlq(&mut dlq, &req.original_events, &parsed, false)
                .await
                .unwrap()
        );
        drop(dlq);

        assert_eq!(
            stream
                .next()
                .await
                .unwrap()
                .into_log()
                .get_message()
                .unwrap()
                .to_string_lossy(),
            "first"
        );
        assert_eq!(
            stream
                .next()
                .await
                .unwrap()
                .into_log()
                .get_message()
                .unwrap()
                .to_string_lossy(),
            "second"
        );
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn does_not_mark_retryable_partial_failures_as_fully_handled() {
        let (output, mut stream) = dlq_sender();
        let mut dlq = make_dlq(output);
        let req = request(vec![processed_event("first")]);
        let response = http::Response::builder()
            .status(StatusCode::OK)
            .body(Bytes::from(
                r#"{"errors":true,"items":[{"index":{"status":429}}]}"#,
            ))
            .unwrap();

        let parsed = parse_response_body(&response);
        assert!(
            !maybe_emit_dlq(&mut dlq, &req.original_events, &parsed, true)
                .await
                .unwrap()
        );
        drop(dlq);
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn routes_orphaned_events_to_dlq_on_truncated_response() {
        let (output, mut stream) = dlq_sender();
        let mut dlq = make_dlq(output);
        // 2 events sent, ES responds with only 1 item (truncated/malformed response).
        let req = request(vec![processed_event("first"), processed_event("orphan")]);
        let response = http::Response::builder()
            .status(StatusCode::OK)
            .body(Bytes::from(
                r#"{"errors":true,"items":[{"index":{"status":400,"error":{"type":"mapper_parsing_exception","reason":"bad mapping"}}}]}"#,
            ))
            .unwrap();

        let parsed = parse_response_body(&response);
        assert!(
            maybe_emit_dlq(&mut dlq, &req.original_events, &parsed, true)
                .await
                .unwrap()
        );
        drop(dlq);

        // First event: DLQ'd due to 400 status
        let event = stream.next().await.unwrap().into_log();
        assert_eq!(event.get_message().unwrap().to_string_lossy(), "first");
        assert_eq!(
            event.get(event_path!("metadata", "dlq", "reason")).unwrap(),
            &"elasticsearch_bulk_rejected".into()
        );

        // Second event: DLQ'd as orphan (no corresponding status in response)
        let event = stream.next().await.unwrap().into_log();
        assert_eq!(event.get_message().unwrap().to_string_lossy(), "orphan");
        assert_eq!(
            event.get(event_path!("metadata", "dlq", "reason")).unwrap(),
            &"elasticsearch_missing_response_item".into()
        );

        assert!(stream.next().await.is_none());
    }
}

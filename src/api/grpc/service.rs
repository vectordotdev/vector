use std::pin::Pin;
use std::time::Duration;

use tokio::time::interval;
use tokio_stream::{wrappers::IntervalStream, Stream, StreamExt};
use tonic::{Request, Response, Status};

use crate::config::Config;
use crate::proto::observability::{self, *};

/// gRPC observability service implementation.
///
/// This service provides real-time monitoring and observability for Vector instances,
/// replacing the previous GraphQL API with a more efficient gRPC interface.
#[derive(Clone)]
pub struct ObservabilityService {
    config: Config,
}

impl ObservabilityService {
    pub fn new(config: Config) -> Self {
        Self { config }
    }
}

#[tonic::async_trait]
impl observability::Service for ObservabilityService {
    // ========== Simple Queries ==========

    async fn health(
        &self,
        _request: Request<HealthRequest>,
    ) -> Result<Response<HealthResponse>, Status> {
        Ok(Response::new(HealthResponse { healthy: true }))
    }

    async fn get_meta(
        &self,
        _request: Request<MetaRequest>,
    ) -> Result<Response<MetaResponse>, Status> {
        let version = crate::get_version().to_string();
        let hostname = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "unknown".to_string());

        Ok(Response::new(MetaResponse { version, hostname }))
    }

    async fn get_components(
        &self,
        _request: Request<ComponentsRequest>,
    ) -> Result<Response<ComponentsResponse>, Status> {
        // TODO: Implement component listing from self.config
        // For now, return empty list
        Ok(Response::new(ComponentsResponse { components: vec![] }))
    }

    // ========== Streaming Metrics ==========

    type StreamHeartbeatStream =
        Pin<Box<dyn Stream<Item = Result<HeartbeatResponse, Status>> + Send>>;

    async fn stream_heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> Result<Response<Self::StreamHeartbeatStream>, Status> {
        let interval_ms = request.into_inner().interval_ms;
        if interval_ms <= 0 {
            return Err(Status::invalid_argument("interval_ms must be positive"));
        }

        let duration = Duration::from_millis(interval_ms as u64);
        let stream = IntervalStream::new(interval(duration))
            .map(|_| {
                let utc = Some(prost_types::Timestamp {
                    seconds: chrono::Utc::now().timestamp(),
                    nanos: 0,
                });
                Ok(HeartbeatResponse { utc })
            });

        Ok(Response::new(Box::pin(stream)))
    }

    type StreamUptimeStream = Pin<Box<dyn Stream<Item = Result<UptimeResponse, Status>> + Send>>;

    async fn stream_uptime(
        &self,
        request: Request<UptimeRequest>,
    ) -> Result<Response<Self::StreamUptimeStream>, Status> {
        let interval_ms = request.into_inner().interval_ms;
        if interval_ms <= 0 {
            return Err(Status::invalid_argument("interval_ms must be positive"));
        }

        let start_time = std::time::Instant::now();
        let duration = Duration::from_millis(interval_ms as u64);
        let stream = IntervalStream::new(interval(duration))
            .map(move |_| {
                let uptime_seconds = start_time.elapsed().as_secs() as i64;
                Ok(UptimeResponse { uptime_seconds })
            });

        Ok(Response::new(Box::pin(stream)))
    }

    type StreamComponentAllocatedBytesStream =
        Pin<Box<dyn Stream<Item = Result<ComponentAllocatedBytesResponse, Status>> + Send>>;

    async fn stream_component_allocated_bytes(
        &self,
        _request: Request<MetricStreamRequest>,
    ) -> Result<Response<Self::StreamComponentAllocatedBytesStream>, Status> {
        // TODO: Implement actual metric streaming
        Err(Status::unimplemented("not yet implemented"))
    }

    type StreamComponentReceivedEventsThroughputStream =
        Pin<Box<dyn Stream<Item = Result<ComponentThroughputResponse, Status>> + Send>>;

    async fn stream_component_received_events_throughput(
        &self,
        _request: Request<MetricStreamRequest>,
    ) -> Result<Response<Self::StreamComponentReceivedEventsThroughputStream>, Status> {
        // TODO: Implement actual metric streaming
        Err(Status::unimplemented("not yet implemented"))
    }

    type StreamComponentSentEventsThroughputStream =
        Pin<Box<dyn Stream<Item = Result<ComponentThroughputResponse, Status>> + Send>>;

    async fn stream_component_sent_events_throughput(
        &self,
        _request: Request<MetricStreamRequest>,
    ) -> Result<Response<Self::StreamComponentSentEventsThroughputStream>, Status> {
        // TODO: Implement actual metric streaming
        Err(Status::unimplemented("not yet implemented"))
    }

    type StreamComponentReceivedBytesThroughputStream =
        Pin<Box<dyn Stream<Item = Result<ComponentThroughputResponse, Status>> + Send>>;

    async fn stream_component_received_bytes_throughput(
        &self,
        _request: Request<MetricStreamRequest>,
    ) -> Result<Response<Self::StreamComponentReceivedBytesThroughputStream>, Status> {
        // TODO: Implement actual metric streaming
        Err(Status::unimplemented("not yet implemented"))
    }

    type StreamComponentSentBytesThroughputStream =
        Pin<Box<dyn Stream<Item = Result<ComponentThroughputResponse, Status>> + Send>>;

    async fn stream_component_sent_bytes_throughput(
        &self,
        _request: Request<MetricStreamRequest>,
    ) -> Result<Response<Self::StreamComponentSentBytesThroughputStream>, Status> {
        // TODO: Implement actual metric streaming
        Err(Status::unimplemented("not yet implemented"))
    }

    type StreamComponentReceivedEventsTotalStream =
        Pin<Box<dyn Stream<Item = Result<ComponentTotalsResponse, Status>> + Send>>;

    async fn stream_component_received_events_total(
        &self,
        _request: Request<MetricStreamRequest>,
    ) -> Result<Response<Self::StreamComponentReceivedEventsTotalStream>, Status> {
        // TODO: Implement actual metric streaming
        Err(Status::unimplemented("not yet implemented"))
    }

    type StreamComponentSentEventsTotalStream =
        Pin<Box<dyn Stream<Item = Result<ComponentTotalsResponse, Status>> + Send>>;

    async fn stream_component_sent_events_total(
        &self,
        _request: Request<MetricStreamRequest>,
    ) -> Result<Response<Self::StreamComponentSentEventsTotalStream>, Status> {
        // TODO: Implement actual metric streaming
        Err(Status::unimplemented("not yet implemented"))
    }

    type StreamComponentReceivedBytesTotalStream =
        Pin<Box<dyn Stream<Item = Result<ComponentTotalsResponse, Status>> + Send>>;

    async fn stream_component_received_bytes_total(
        &self,
        _request: Request<MetricStreamRequest>,
    ) -> Result<Response<Self::StreamComponentReceivedBytesTotalStream>, Status> {
        // TODO: Implement actual metric streaming
        Err(Status::unimplemented("not yet implemented"))
    }

    type StreamComponentSentBytesTotalStream =
        Pin<Box<dyn Stream<Item = Result<ComponentTotalsResponse, Status>> + Send>>;

    async fn stream_component_sent_bytes_total(
        &self,
        _request: Request<MetricStreamRequest>,
    ) -> Result<Response<Self::StreamComponentSentBytesTotalStream>, Status> {
        // TODO: Implement actual metric streaming
        Err(Status::unimplemented("not yet implemented"))
    }

    type StreamComponentErrorsTotalStream =
        Pin<Box<dyn Stream<Item = Result<ComponentTotalsResponse, Status>> + Send>>;

    async fn stream_component_errors_total(
        &self,
        _request: Request<MetricStreamRequest>,
    ) -> Result<Response<Self::StreamComponentErrorsTotalStream>, Status> {
        // TODO: Implement actual metric streaming
        Err(Status::unimplemented("not yet implemented"))
    }

    // ========== Event Tapping ==========

    type StreamOutputEventsStream =
        Pin<Box<dyn Stream<Item = Result<OutputEvent, Status>> + Send>>;

    async fn stream_output_events(
        &self,
        _request: Request<OutputEventsRequest>,
    ) -> Result<Response<Self::StreamOutputEventsStream>, Status> {
        // TODO: Implement event tapping
        Err(Status::unimplemented("not yet implemented"))
    }
}

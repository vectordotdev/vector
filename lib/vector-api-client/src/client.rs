use http::Uri;
use tokio_stream::{Stream, StreamExt};
use tonic::transport::{Channel, Endpoint};
use tonic_health::pb::{
    HealthCheckRequest, health_check_response::ServingStatus, health_client::HealthClient,
};

use crate::{
    error::{Error, Result},
    proto::{
        GetAllocationTracingStatusRequest, GetAllocationTracingStatusResponse,
        GetComponentsRequest, GetComponentsResponse, GetMetaRequest, GetMetaResponse, MetricName,
        StreamComponentAllocatedBytesRequest, StreamComponentAllocatedBytesResponse,
        StreamComponentMetricsRequest, StreamComponentMetricsResponse, StreamHeartbeatRequest,
        StreamHeartbeatResponse, StreamOutputEventsRequest, StreamOutputEventsResponse,
        StreamUptimeRequest, StreamUptimeResponse,
        observability_service_client::ObservabilityServiceClient,
    },
};

/// gRPC client for the Vector observability API
#[derive(Debug, Clone)]
pub struct Client {
    endpoint: Endpoint,
    channel: Option<Channel>,
    client: Option<ObservabilityServiceClient<Channel>>,
}

impl Client {
    /// Create a new gRPC client
    ///
    /// The client is not connected until `connect()` is called.
    ///
    /// # Arguments
    ///
    /// * `uri` - The gRPC server URI (e.g., `"http://localhost:9999".parse().unwrap()`)
    pub fn new(uri: Uri) -> Self {
        Self {
            endpoint: Endpoint::from(uri),
            channel: None,
            client: None,
        }
    }

    /// Connect to the gRPC server
    pub async fn connect(&mut self) -> Result<()> {
        let channel = self.endpoint.connect().await?;
        self.client = Some(ObservabilityServiceClient::new(channel.clone()));
        self.channel = Some(channel);
        Ok(())
    }

    /// Ensure the client is connected
    fn ensure_connected(&mut self) -> Result<&mut ObservabilityServiceClient<Channel>> {
        self.client.as_mut().ok_or(Error::NotConnected)
    }

    /// Get the underlying channel
    fn channel(&self) -> Result<&Channel> {
        self.channel.as_ref().ok_or(Error::NotConnected)
    }

    // ========== Unary RPCs ==========

    /// Check if the API server is healthy using the standard gRPC health check
    /// protocol (grpc.health.v1.Health/Check).
    ///
    /// Queries the empty service name (`""`), which represents whole-server
    /// health. This is the default used by Kubernetes gRPC probes and
    /// `grpc-health-probe`.
    ///
    /// Returns `Ok(())` if the server is `SERVING`, or an error otherwise.
    pub async fn health(&mut self) -> Result<()> {
        let channel = self.channel()?.clone();
        let mut health_client = HealthClient::new(channel);
        let response = health_client
            .check(HealthCheckRequest {
                service: String::new(),
            })
            .await?;
        let status = response.into_inner().status;
        if status != ServingStatus::Serving as i32 {
            return Err(Error::NotServing { status });
        }
        Ok(())
    }

    /// Get metadata about the Vector instance
    pub async fn get_meta(&mut self) -> Result<GetMetaResponse> {
        let client = self.ensure_connected()?;
        let response = client.get_meta(GetMetaRequest {}).await?;
        Ok(response.into_inner())
    }

    /// Get information about configured components
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of components to return (0 = no limit)
    pub async fn get_components(&mut self, limit: i32) -> Result<GetComponentsResponse> {
        let client = self.ensure_connected()?;
        let response = client
            .get_components(GetComponentsRequest { limit })
            .await?;
        Ok(response.into_inner())
    }

    /// Check whether allocation tracing is active on the connected Vector instance
    pub async fn get_allocation_tracing_status(
        &mut self,
    ) -> Result<GetAllocationTracingStatusResponse> {
        let client = self.ensure_connected()?;
        let response = client
            .get_allocation_tracing_status(GetAllocationTracingStatusRequest {})
            .await?;
        Ok(response.into_inner())
    }

    // ========== Streaming RPCs ==========

    /// Stream periodic heartbeat timestamps
    ///
    /// # Arguments
    ///
    /// * `interval_ms` - Update interval in milliseconds
    pub async fn stream_heartbeat(
        &mut self,
        interval_ms: i32,
    ) -> Result<impl Stream<Item = Result<StreamHeartbeatResponse>>> {
        let client = self.ensure_connected()?;
        let response = client
            .stream_heartbeat(StreamHeartbeatRequest { interval_ms })
            .await?;
        Ok(response.into_inner().map(|r| r.map_err(Error::from)))
    }

    /// Stream uptime in seconds
    ///
    /// # Arguments
    ///
    /// * `interval_ms` - Update interval in milliseconds
    pub async fn stream_uptime(
        &mut self,
        interval_ms: i32,
    ) -> Result<impl Stream<Item = Result<StreamUptimeResponse>>> {
        let client = self.ensure_connected()?;
        let response = client
            .stream_uptime(StreamUptimeRequest { interval_ms })
            .await?;
        Ok(response.into_inner().map(|r| r.map_err(Error::from)))
    }

    /// Stream memory allocated per component
    ///
    /// # Arguments
    ///
    /// * `interval_ms` - Update interval in milliseconds
    pub async fn stream_component_allocated_bytes(
        &mut self,
        interval_ms: i32,
    ) -> Result<impl Stream<Item = Result<StreamComponentAllocatedBytesResponse>>> {
        let client = self.ensure_connected()?;
        let response = client
            .stream_component_allocated_bytes(StreamComponentAllocatedBytesRequest { interval_ms })
            .await?;
        Ok(response.into_inner().map(|r| r.map_err(Error::from)))
    }

    /// Stream per-component metrics for a chosen metric name.
    ///
    /// # Arguments
    ///
    /// * `metric` - Which metric to stream
    /// * `interval_ms` - Update interval in milliseconds
    pub async fn stream_component_metrics(
        &mut self,
        metric: MetricName,
        interval_ms: i32,
    ) -> Result<impl Stream<Item = Result<StreamComponentMetricsResponse>>> {
        let client = self.ensure_connected()?;
        let response = client
            .stream_component_metrics(StreamComponentMetricsRequest {
                interval_ms,
                metric: metric as i32,
            })
            .await?;
        Ok(response.into_inner().map(|r| r.map_err(Error::from)))
    }

    /// Stream events from components matching patterns
    ///
    /// This is used by `vector tap` to capture events.
    pub async fn stream_output_events(
        &mut self,
        request: StreamOutputEventsRequest,
    ) -> Result<impl Stream<Item = Result<StreamOutputEventsResponse>> + use<>> {
        let client = self.ensure_connected()?;
        let response = client.stream_output_events(request).await?;
        Ok(response.into_inner().map(|r| r.map_err(Error::from)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_not_connected_error() {
        let mut client = Client::new("http://localhost:9999".parse().unwrap());
        let result = client.health().await;
        assert!(matches!(result, Err(Error::NotConnected)));
    }

    #[test]
    fn test_ensure_connected() {
        let mut client = Client::new("http://localhost:9999".parse().unwrap());
        let result = client.ensure_connected();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::NotConnected));
    }
}

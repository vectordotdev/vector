use tokio_stream::{Stream, StreamExt};
use tonic::transport::{Channel, Endpoint};

use crate::{
    error::{Error, Result},
    proto::{
        ComponentAllocatedBytesResponse, ComponentThroughputResponse, ComponentTotalsResponse,
        ComponentsRequest, ComponentsResponse, HealthRequest, HealthResponse, HeartbeatRequest,
        HeartbeatResponse, MetaRequest, MetaResponse, MetricStreamRequest, OutputEvent,
        OutputEventsRequest, UptimeRequest, UptimeResponse,
        observability_client::ObservabilityClient,
    },
};

/// gRPC client for the Vector observability API
#[derive(Debug, Clone)]
pub struct GrpcClient {
    url: String,
    client: Option<ObservabilityClient<Channel>>,
}

impl GrpcClient {
    /// Create a new gRPC client
    ///
    /// The client is not connected until `connect()` is called.
    ///
    /// # Arguments
    ///
    /// * `url` - The gRPC server URL (e.g., "http://localhost:9999")
    pub async fn new(url: impl Into<String>) -> Result<Self> {
        let url = url.into();
        Ok(Self { url, client: None })
    }

    /// Connect to the gRPC server
    pub async fn connect(&mut self) -> Result<()> {
        let endpoint = Endpoint::from_shared(self.url.clone()).map_err(|e| Error::InvalidUrl {
            message: e.to_string(),
        })?;

        let channel = endpoint.connect().await?;
        self.client = Some(ObservabilityClient::new(channel));
        Ok(())
    }

    /// Ensure the client is connected
    fn ensure_connected(&mut self) -> Result<&mut ObservabilityClient<Channel>> {
        self.client.as_mut().ok_or(Error::NotConnected)
    }

    // ========== Unary RPCs ==========

    /// Check if the API server is healthy
    pub async fn health(&mut self) -> Result<HealthResponse> {
        let client = self.ensure_connected()?;
        let response = client.health(HealthRequest {}).await?;
        Ok(response.into_inner())
    }

    /// Get metadata about the Vector instance
    pub async fn get_meta(&mut self) -> Result<MetaResponse> {
        let client = self.ensure_connected()?;
        let response = client.get_meta(MetaRequest {}).await?;
        Ok(response.into_inner())
    }

    /// Get information about configured components
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of components to return (0 = no limit)
    pub async fn get_components(&mut self, limit: i32) -> Result<ComponentsResponse> {
        let client = self.ensure_connected()?;
        let response = client.get_components(ComponentsRequest { limit }).await?;
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
    ) -> Result<impl Stream<Item = Result<HeartbeatResponse>>> {
        let client = self.ensure_connected()?;
        let response = client
            .stream_heartbeat(HeartbeatRequest { interval_ms })
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
    ) -> Result<impl Stream<Item = Result<UptimeResponse>>> {
        let client = self.ensure_connected()?;
        let response = client.stream_uptime(UptimeRequest { interval_ms }).await?;
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
    ) -> Result<impl Stream<Item = Result<ComponentAllocatedBytesResponse>>> {
        let client = self.ensure_connected()?;
        let response = client
            .stream_component_allocated_bytes(MetricStreamRequest { interval_ms })
            .await?;
        Ok(response.into_inner().map(|r| r.map_err(Error::from)))
    }

    /// Stream received events throughput (events/sec)
    ///
    /// # Arguments
    ///
    /// * `interval_ms` - Update interval in milliseconds
    pub async fn stream_component_received_events_throughput(
        &mut self,
        interval_ms: i32,
    ) -> Result<impl Stream<Item = Result<ComponentThroughputResponse>>> {
        let client = self.ensure_connected()?;
        let response = client
            .stream_component_received_events_throughput(MetricStreamRequest { interval_ms })
            .await?;
        Ok(response.into_inner().map(|r| r.map_err(Error::from)))
    }

    /// Stream sent events throughput (events/sec)
    ///
    /// # Arguments
    ///
    /// * `interval_ms` - Update interval in milliseconds
    pub async fn stream_component_sent_events_throughput(
        &mut self,
        interval_ms: i32,
    ) -> Result<impl Stream<Item = Result<ComponentThroughputResponse>>> {
        let client = self.ensure_connected()?;
        let response = client
            .stream_component_sent_events_throughput(MetricStreamRequest { interval_ms })
            .await?;
        Ok(response.into_inner().map(|r| r.map_err(Error::from)))
    }

    /// Stream received bytes throughput (bytes/sec)
    ///
    /// # Arguments
    ///
    /// * `interval_ms` - Update interval in milliseconds
    pub async fn stream_component_received_bytes_throughput(
        &mut self,
        interval_ms: i32,
    ) -> Result<impl Stream<Item = Result<ComponentThroughputResponse>>> {
        let client = self.ensure_connected()?;
        let response = client
            .stream_component_received_bytes_throughput(MetricStreamRequest { interval_ms })
            .await?;
        Ok(response.into_inner().map(|r| r.map_err(Error::from)))
    }

    /// Stream sent bytes throughput (bytes/sec)
    ///
    /// # Arguments
    ///
    /// * `interval_ms` - Update interval in milliseconds
    pub async fn stream_component_sent_bytes_throughput(
        &mut self,
        interval_ms: i32,
    ) -> Result<impl Stream<Item = Result<ComponentThroughputResponse>>> {
        let client = self.ensure_connected()?;
        let response = client
            .stream_component_sent_bytes_throughput(MetricStreamRequest { interval_ms })
            .await?;
        Ok(response.into_inner().map(|r| r.map_err(Error::from)))
    }

    /// Stream total received events
    ///
    /// # Arguments
    ///
    /// * `interval_ms` - Update interval in milliseconds
    pub async fn stream_component_received_events_total(
        &mut self,
        interval_ms: i32,
    ) -> Result<impl Stream<Item = Result<ComponentTotalsResponse>>> {
        let client = self.ensure_connected()?;
        let response = client
            .stream_component_received_events_total(MetricStreamRequest { interval_ms })
            .await?;
        Ok(response.into_inner().map(|r| r.map_err(Error::from)))
    }

    /// Stream total sent events
    ///
    /// # Arguments
    ///
    /// * `interval_ms` - Update interval in milliseconds
    pub async fn stream_component_sent_events_total(
        &mut self,
        interval_ms: i32,
    ) -> Result<impl Stream<Item = Result<ComponentTotalsResponse>>> {
        let client = self.ensure_connected()?;
        let response = client
            .stream_component_sent_events_total(MetricStreamRequest { interval_ms })
            .await?;
        Ok(response.into_inner().map(|r| r.map_err(Error::from)))
    }

    /// Stream total received bytes
    ///
    /// # Arguments
    ///
    /// * `interval_ms` - Update interval in milliseconds
    pub async fn stream_component_received_bytes_total(
        &mut self,
        interval_ms: i32,
    ) -> Result<impl Stream<Item = Result<ComponentTotalsResponse>>> {
        let client = self.ensure_connected()?;
        let response = client
            .stream_component_received_bytes_total(MetricStreamRequest { interval_ms })
            .await?;
        Ok(response.into_inner().map(|r| r.map_err(Error::from)))
    }

    /// Stream total sent bytes
    ///
    /// # Arguments
    ///
    /// * `interval_ms` - Update interval in milliseconds
    pub async fn stream_component_sent_bytes_total(
        &mut self,
        interval_ms: i32,
    ) -> Result<impl Stream<Item = Result<ComponentTotalsResponse>>> {
        let client = self.ensure_connected()?;
        let response = client
            .stream_component_sent_bytes_total(MetricStreamRequest { interval_ms })
            .await?;
        Ok(response.into_inner().map(|r| r.map_err(Error::from)))
    }

    /// Stream error counts per component
    ///
    /// # Arguments
    ///
    /// * `interval_ms` - Update interval in milliseconds
    pub async fn stream_component_errors_total(
        &mut self,
        interval_ms: i32,
    ) -> Result<impl Stream<Item = Result<ComponentTotalsResponse>>> {
        let client = self.ensure_connected()?;
        let response = client
            .stream_component_errors_total(MetricStreamRequest { interval_ms })
            .await?;
        Ok(response.into_inner().map(|r| r.map_err(Error::from)))
    }

    /// Stream events from components matching patterns
    ///
    /// This is used by `vector tap` to capture events.
    pub async fn stream_output_events(
        &mut self,
        request: OutputEventsRequest,
    ) -> Result<impl Stream<Item = Result<OutputEvent>>> {
        let client = self.ensure_connected()?;
        let response = client.stream_output_events(request).await?;
        Ok(response.into_inner().map(|r| r.map_err(Error::from)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client_creation() {
        let client = GrpcClient::new("http://localhost:9999").await;
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_invalid_url() {
        let result = GrpcClient::new("not a url").await;
        assert!(result.is_ok()); // URL validation happens on connect
    }

    #[tokio::test]
    async fn test_connection_failure() {
        // Test connecting to non-existent server
        let mut client = GrpcClient::new("http://localhost:1").await.unwrap();
        let result = client.connect().await;
        assert!(
            result.is_err(),
            "Should fail to connect to non-existent server"
        );
    }

    #[tokio::test]
    async fn test_not_connected_error() {
        // Test calling RPC before connecting
        let mut client = GrpcClient::new("http://localhost:9999").await.unwrap();
        let result = client.health().await;
        assert!(matches!(result, Err(Error::NotConnected)));
    }

    #[tokio::test]
    async fn test_ensure_connected() {
        let mut client = GrpcClient::new("http://localhost:9999").await.unwrap();

        // Should fail before connection
        let result = client.ensure_connected();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::NotConnected));
    }
}

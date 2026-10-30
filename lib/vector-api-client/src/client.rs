use http::{Uri, uri::Scheme};
use hyper_util::{client::legacy::Builder, rt::TokioExecutor};
use tokio_stream::{Stream, StreamExt};
use tonic::body::BoxBody;
use tonic_health::pb::{
    HealthCheckRequest, health_check_response::ServingStatus, health_client::HealthClient,
};
use vector_common::constants::GRPC_MAX_HEADER_LIST_SIZE;
use vector_core::{
    config::proxy::ProxyConfig,
    tls::{MaybeTlsSettings, TlsConfig},
};
use vector_http::{GrpcTransport, Http2Client};

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

/// Sends tonic's gRPC requests over Vector's shared HTTP/2 client.
type Transport = GrpcTransport<BoxBody>;

/// gRPC client for the Vector observability API
///
/// Clones share one HTTP/2 connection pool, so concurrent RPCs multiplex over one connection.
#[derive(Debug, Clone)]
pub struct Client {
    uri: Uri,
    rpc: Option<RpcClients>,
}

/// The tonic clients that [`Client::connect`] sets up over one shared transport.
#[derive(Debug, Clone)]
struct RpcClients {
    health: HealthClient<Transport>,
    observability: ObservabilityServiceClient<Transport>,
}

impl Client {
    /// Create a new gRPC client
    ///
    /// RPCs fail with [`Error::NotConnected`] until `connect()` is called.
    ///
    /// # Arguments
    ///
    /// * `uri` - The gRPC server URI (e.g., `"http://localhost:9999".parse().unwrap()`)
    pub fn new(uri: Uri) -> Self {
        Self { uri, rpc: None }
    }

    /// Set up the HTTP/2 transport to the gRPC server
    ///
    /// This opens no connection: the first RPC connects, and fails if the server isn't reachable.
    /// Call [`Client::health`] to check that the server is reachable and serving.
    ///
    /// RPCs go to the scheme and authority of the URI; its path is ignored. `https` URIs use TLS
    /// with the system root certificates and offer the `h2` ALPN protocol. No proxy is used.
    ///
    /// # Errors
    ///
    /// Returns an error if the URI, TLS settings, or HTTP/2 client cannot be set up.
    pub fn connect(&mut self) -> Result<()> {
        let origin = grpc_origin(&self.uri)?;
        let tls = if origin.scheme() == Some(&Scheme::HTTPS) {
            // Offer `h2` like tonic's TLS client did, so TLS frontends that route by ALPN select
            // HTTP/2.
            MaybeTlsSettings::tls_client(Some(&TlsConfig {
                alpn_protocols: Some(vec!["h2".to_owned()]),
                ..TlsConfig::default()
            }))
            .map_err(|error| Error::Connection {
                source: error.into(),
            })?
        } else {
            MaybeTlsSettings::Raw(())
        };

        let mut builder = Builder::new(TokioExecutor::new());
        builder.http2_max_header_list_size(GRPC_MAX_HEADER_LIST_SIZE);
        let transport = GrpcTransport(
            Http2Client::new(tls, &ProxyConfig::default(), builder)
                .map_err(|source| Error::Connection { source })?,
        );

        self.rpc = Some(RpcClients {
            health: HealthClient::with_origin(transport.clone(), origin.clone()),
            observability: ObservabilityServiceClient::with_origin(transport, origin),
        });
        Ok(())
    }

    /// Ensure the client is connected
    fn ensure_connected(&mut self) -> Result<&mut ObservabilityServiceClient<Transport>> {
        Ok(&mut self.rpc_clients()?.observability)
    }

    /// Get the tonic clients set up by `connect()`
    fn rpc_clients(&mut self) -> Result<&mut RpcClients> {
        self.rpc.as_mut().ok_or(Error::NotConnected)
    }

    // ========== Unary RPCs ==========

    /// Check if the API server is healthy using the standard gRPC health check
    /// protocol (grpc.health.v1.Health/Check).
    ///
    /// Queries the empty service name (`""`), which represents whole-server
    /// health. This is the default used by Kubernetes gRPC probes and
    /// `grpc-health-probe`.
    ///
    /// Returns `Ok(())` if the server is `SERVING`, or an error otherwise. `connect()` opens no
    /// connection, so call this after it to check that the server is reachable.
    ///
    /// # Errors
    ///
    /// Returns an error if the client is not connected, the health RPC fails,
    /// or the server reports a status other than `SERVING`.
    pub async fn health(&mut self) -> Result<()> {
        let response = self
            .rpc_clients()?
            .health
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
    ///
    /// # Errors
    ///
    /// Returns an error if the client is not connected or the RPC fails.
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
    ///
    /// # Errors
    ///
    /// Returns an error if the client is not connected or the RPC fails.
    pub async fn get_components(&mut self, limit: i32) -> Result<GetComponentsResponse> {
        let client = self.ensure_connected()?;
        let response = client
            .get_components(GetComponentsRequest { limit })
            .await?;
        Ok(response.into_inner())
    }

    /// Check whether allocation tracing is active on the connected Vector instance
    ///
    /// # Errors
    ///
    /// Returns an error if the client is not connected or the RPC fails.
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
    ///
    /// # Errors
    ///
    /// Returns an error if the client is not connected or the streaming RPC
    /// cannot be started. The returned stream yields errors if receiving a response fails.
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
    ///
    /// # Errors
    ///
    /// Returns an error if the client is not connected or the streaming RPC
    /// cannot be started. The returned stream yields errors if receiving a response fails.
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
    ///
    /// # Errors
    ///
    /// Returns an error if the client is not connected or the streaming RPC
    /// cannot be started. The returned stream yields errors if receiving a response fails.
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
    ///
    /// # Errors
    ///
    /// Returns an error if the client is not connected or the streaming RPC
    /// cannot be started. The returned stream yields errors if receiving a response fails.
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
    ///
    /// # Errors
    ///
    /// Returns an error if the client is not connected or the streaming RPC
    /// cannot be started. The returned stream yields errors if receiving a response fails.
    pub async fn stream_output_events(
        &mut self,
        request: StreamOutputEventsRequest,
    ) -> Result<impl Stream<Item = Result<StreamOutputEventsResponse>> + use<>> {
        let client = self.ensure_connected()?;
        let response = client.stream_output_events(request).await?;
        Ok(response.into_inner().map(|r| r.map_err(Error::from)))
    }
}

/// Returns the `scheme://authority/` origin that tonic prefixes to each gRPC method path.
fn grpc_origin(uri: &Uri) -> Result<Uri> {
    let (Some(scheme), Some(authority)) = (uri.scheme(), uri.authority()) else {
        return Err(Error::Connection {
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("{uri} must contain a scheme and an authority"),
            )
            .into(),
        });
    };

    Uri::builder()
        .scheme(scheme.clone())
        .authority(authority.clone())
        .path_and_query("/")
        .build()
        .map_err(|source| Error::Connection {
            source: source.into(),
        })
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

    #[tokio::test]
    async fn rpc_reports_connection_failure_after_connect() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut client = Client::new(
            format!("http://{}", listener.local_addr().unwrap())
                .parse()
                .unwrap(),
        );
        client.connect().unwrap();

        let ((), result) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            tokio::join!(
                async {
                    let (stream, _) = listener.accept().await.unwrap();
                    drop(stream);
                },
                client.health()
            )
        })
        .await
        .unwrap();
        let error = result.unwrap_err();
        assert!(matches!(error, Error::Grpc { .. }), "{error}");
        // `vector top` and `vector tap` reconnect after transport failures.
        assert!(!error.is_fatal(), "{error}");
    }

    #[tokio::test]
    async fn https_rpc_uses_tls_with_h2_alpn() {
        use tokio::io::AsyncReadExt as _;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut client = Client::new(
            format!("https://{}", listener.local_addr().unwrap())
                .parse()
                .unwrap(),
        );
        client.connect().unwrap();
        let (client_hello, result) =
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                tokio::join!(
                    async {
                        let (mut stream, _) = listener.accept().await.unwrap();
                        let mut header = [0u8; 5];
                        stream.read_exact(&mut header).await.unwrap();
                        // 0x16 is the TLS handshake record type that starts a ClientHello.
                        assert_eq!(header[0], 0x16);
                        let mut record =
                            vec![0; usize::from(u16::from_be_bytes([header[3], header[4]]))];
                        stream.read_exact(&mut record).await.unwrap();
                        record
                    },
                    client.health()
                )
            })
            .await
            .unwrap();
        // The server closes after the ClientHello, so the TLS handshake fails.
        assert!(result.is_err());
        // The ALPN extension (type 16) offers only `h2`, as tonic's TLS client did.
        let alpn_h2 = [0, 16, 0, 5, 0, 3, 2, b'h', b'2'];
        assert!(
            client_hello
                .windows(alpn_h2.len())
                .any(|bytes| bytes == alpn_h2)
        );
    }
}

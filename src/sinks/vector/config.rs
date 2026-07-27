use std::{
    num::NonZeroU64,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};

use futures::{FutureExt, TryFutureExt, future::BoxFuture};
use http::Uri;
use hyper::client::HttpConnector;
use hyper_openssl::HttpsConnector;
use hyper_proxy::ProxyConnector;
use tokio::sync::Semaphore;
use tonic::body::BoxBody;
use tower::{Service, ServiceBuilder};
use vector_lib::configurable::configurable_component;

use super::{
    VectorSinkError,
    compression::VectorCompression,
    service::{VectorRequest, VectorResponse, VectorService},
    sink::VectorSink,
};
use crate::{
    config::{
        AcknowledgementsConfig, GenerateConfig, Input, ProxyConfig, SinkConfig, SinkContext,
        SinkHealthcheckOptions,
    },
    http::build_proxy_connector,
    proto::vector as proto,
    sinks::{
        Healthcheck, VectorSink as VectorSinkType,
        util::{
            BatchConfig, RealtimeEventBasedDefaultBatchSettings, TowerRequestConfig,
            retries::RetryLogic,
            service::{HealthConfig, HealthLogic, ServiceBuilderExt, TowerRequestSettings},
        },
    },
    tls::{MaybeTlsSettings, TlsEnableableConfig},
};

/// Configuration for the `vector` sink.
#[configurable_component(sink("vector", "Relay observability data to a Vector instance."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct VectorConfig {
    /// Version of the configuration.
    // NOTE: this option is deprecated and has already been removed from the "old" docs.
    // At some point in the future we will remove it entirely as a breaking change.
    #[configurable(metadata(docs::hidden))]
    version: Option<super::VectorConfigVersion>,

    /// The downstream Vector address to which to connect.
    ///
    /// Both IP address and hostname are accepted formats.
    ///
    /// The address _must_ include a port.
    ///
    /// This option is mutually exclusive with `routing`. Set exactly one of
    /// `address` or `routing`.
    ///
    /// This option has been deprecated, use `routing.endpoints` instead.
    #[configurable(validation(format = "uri"))]
    #[configurable(
        deprecated = "This option has been deprecated, use `routing.endpoints` instead."
    )]
    #[configurable(metadata(docs::examples = "92.12.333.224:6000"))]
    #[configurable(metadata(docs::examples = "https://somehost:6000"))]
    #[serde(default)]
    address: Option<String>,

    /// Routing options for sending requests to one or more downstream Vector endpoints.
    ///
    /// This option is mutually exclusive with `address`. Set exactly one of
    /// `address` or `routing`.
    #[serde(default)]
    #[configurable(derived)]
    routing: Option<RoutingConfig>,

    /// Compression algorithm for requests.
    ///
    /// Supports `"none"`, `"gzip"`, or `"zstd"`.
    ///
    /// For backward compatibility, boolean values are still accepted:
    /// - `true` defaults to gzip compression
    /// - `false` disables compression (deprecated syntax)
    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "super::compression::bool_or_vector_compression"
    )]
    compression: VectorCompression,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<RealtimeEventBasedDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    #[serde(default)]
    tls: Option<TlsEnableableConfig>,

    /// HTTP/2 keepalive settings for the sink's gRPC connections.
    ///
    /// Keepalive is disabled unless this is configured. When enabled, the sink sends HTTP/2 PING
    /// frames on idle connections so that a pooled connection to a downstream Vector instance that
    /// has gone away (crashed, restarted, or cut off by a network partition) is detected and evicted
    /// before it is reused, ensuring retries always go to a live connection.
    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    keepalive: Option<VectorKeepaliveConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub(in crate::sinks::vector) acknowledgements: AcknowledgementsConfig,
}

/// HTTP/2 keepalive configuration for the `vector` sink's gRPC connections.
#[configurable_component]
#[derive(Clone, Copy, Debug)]
#[serde(deny_unknown_fields)]
pub struct VectorKeepaliveConfig {
    /// How often, in seconds, to send a keepalive PING on idle connections.
    ///
    /// Shorter intervals detect dead connections faster at the cost of additional traffic.
    /// gRPC guidance recommends no less than 60 seconds to avoid tripping `too_many_pings`
    /// policies on servers or proxies between source and destination.
    #[serde(default = "default_keepalive_interval_secs")]
    #[configurable(metadata(docs::human_name = "Keepalive Interval"))]
    pub interval_secs: NonZeroU64,

    /// How long, in seconds, to wait for a keepalive PING acknowledgement before treating
    /// the connection as dead and closing it.
    #[serde(default = "default_keepalive_timeout_secs")]
    #[configurable(metadata(docs::human_name = "Keepalive Timeout"))]
    pub timeout_secs: NonZeroU64,
}

const fn default_keepalive_interval_secs() -> NonZeroU64 {
    // Aligned with gRPC keepalive guidance, which recommends no less than one minute to avoid
    // tripping `too_many_pings` policies on proxies between the sink and downstream.
    NonZeroU64::new(60).expect("keepalive interval default must be nonzero")
}

const fn default_keepalive_timeout_secs() -> NonZeroU64 {
    // Matches hyper's default keepalive timeout.
    NonZeroU64::new(20).expect("keepalive timeout default must be nonzero")
}

/// Routing options for sending requests to downstream Vector endpoints.
///
/// Load-balanced sinks healthcheck all configured endpoints on startup.
/// Failover sinks healthcheck only the initially active endpoint by default,
/// which is the first configured endpoint, unless `healthcheck.uri` is set.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
struct RoutingConfig {
    /// The downstream Vector endpoints to which to connect.
    ///
    /// Both IP addresses and hostnames are accepted formats.
    ///
    /// Each endpoint _must_ include a port.
    #[configurable(validation(format = "uri"))]
    #[configurable(metadata(docs::examples = "92.12.333.224:6000"))]
    #[configurable(metadata(docs::examples = "https://somehost:6000"))]
    #[serde(default)]
    endpoints: Vec<String>,

    /// Strategy for routing requests across configured endpoints.
    ///
    /// When only one endpoint is configured, the sink uses the standard
    /// single-endpoint service path and strategy-specific routing semantics are
    /// not applied.
    #[serde(default)]
    strategy: EndpointStrategy,

    /// Options for determining the health and backoff behavior of
    /// load-balanced Vector endpoints.
    ///
    /// This option is only used when `strategy` is set to `load_balance`.
    #[serde(default)]
    #[configurable(derived)]
    health: Option<HealthConfig>,
}

impl VectorConfig {
    /// Creates a `VectorConfig` with the given address.
    pub fn from_address(addr: Uri) -> Self {
        let addr = addr.to_string();
        default_config(addr.as_str())
    }
}

impl GenerateConfig for VectorConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(default_config("127.0.0.1:6000")).unwrap()
    }
}

fn default_config(address: &str) -> VectorConfig {
    VectorConfig {
        version: None,
        address: Some(address.to_owned()),
        routing: None,
        compression: VectorCompression::None,
        batch: BatchConfig::default(),
        request: TowerRequestConfig::default(),
        tls: None,
        keepalive: None,
        acknowledgements: Default::default(),
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "vector")]
impl SinkConfig for VectorConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSinkType, Healthcheck)> {
        let tls = MaybeTlsSettings::from_config(self.tls.as_ref(), false)?;
        let uris = self.uris(tls.is_tls())?;
        let endpoint_strategy = self
            .routing
            .as_ref()
            .map_or_else(EndpointStrategy::default, |routing| routing.strategy);

        let client = new_client(&tls, cx.proxy(), self.keepalive)?;

        let healthcheck = healthchecks(client.clone(), &uris, cx.healthcheck, endpoint_strategy);
        let request_settings = self.request.into_settings();
        let batch_settings = self.batch.into_batcher_settings()?;

        let services = uris
            .into_iter()
            .map(|uri| {
                let endpoint = uri.to_string();
                let service = VectorService::new(client.clone(), uri, self.compression);
                (endpoint, service)
            })
            .collect::<Vec<_>>();

        let sink = match endpoint_strategy {
            _ if services.len() == 1 => {
                let service = ServiceBuilder::new()
                    .settings(request_settings, VectorGrpcRetryLogic)
                    .service(services.into_iter().next().expect("one service").1);

                VectorSinkType::from_event_streamsink(VectorSink {
                    batch_settings,
                    service,
                })
            }
            EndpointStrategy::LoadBalance => {
                let service = request_settings.distributed_service(
                    VectorGrpcRetryLogic,
                    services,
                    self.routing
                        .as_ref()
                        .and_then(|routing| routing.health.clone())
                        .unwrap_or_else(default_endpoint_health_config),
                    VectorGrpcHealthLogic,
                    1,
                );

                VectorSinkType::from_event_streamsink(VectorSink {
                    batch_settings,
                    service,
                })
            }
            EndpointStrategy::Failover | EndpointStrategy::FailoverPrimary => {
                let endpoint_timeout = request_settings.timeout;
                let max_endpoint_attempts = match endpoint_strategy {
                    EndpointStrategy::Failover => services.len(),
                    EndpointStrategy::FailoverPrimary => services.len() + 1,
                    EndpointStrategy::LoadBalance => {
                        unreachable!("load balancing uses a different service")
                    }
                };
                let failover_request_settings = failover_request_settings(
                    request_settings,
                    endpoint_timeout,
                    max_endpoint_attempts,
                );

                let service = ServiceBuilder::new()
                    .settings(failover_request_settings, VectorGrpcRetryLogic)
                    .service(FailoverVectorService::new(
                        services
                            .into_iter()
                            .map(|(_endpoint, service)| service)
                            .collect(),
                        endpoint_timeout,
                        endpoint_strategy,
                    ));

                VectorSinkType::from_event_streamsink(VectorSink {
                    batch_settings,
                    service,
                })
            }
        };

        Ok((sink, Box::pin(healthcheck)))
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

/// Strategy for routing requests across multiple Vector endpoints.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EndpointStrategy {
    /// Distribute requests across healthy endpoints using Vector's existing
    /// Tower distributed service. Endpoint health is tracked using
    /// `routing.health`, and unhealthy endpoints are backed off and probed
    /// according to that configuration. This mode does not preserve a single
    /// active endpoint or prefer the first configured endpoint.
    #[default]
    LoadBalance,
    /// Use one endpoint at a time. When the active endpoint fails, continue
    /// through the configured endpoints from the next endpoint.
    ///
    /// This mode keeps using the last successful endpoint until it fails. Use
    /// `failover_primary` instead when retriable failures should re-check the
    /// first configured endpoint before trying secondary endpoints.
    ///
    /// Requests are serialized for this strategy, regardless of the configured
    /// request concurrency, to preserve one active endpoint at a time.
    Failover,
    /// Use one endpoint at a time. When the active endpoint fails, retry from
    /// the configured endpoint order so the sink can return to its configured
    /// primary endpoint.
    ///
    /// This is useful when receiver-side connection recycling, such as
    /// `max_connection_age_secs`, should converge the sink back to the first
    /// configured endpoint when it is available.
    ///
    /// Requests are serialized for this strategy, regardless of the configured
    /// request concurrency, to preserve one active endpoint at a time.
    FailoverPrimary,
}

#[derive(Clone)]
struct FailoverVectorService {
    services: Vec<VectorService>,
    state: Arc<AtomicUsize>,
    in_flight: Arc<Semaphore>,
    endpoint_timeout: std::time::Duration,
    endpoint_strategy: EndpointStrategy,
}

impl FailoverVectorService {
    fn new(
        services: Vec<VectorService>,
        endpoint_timeout: std::time::Duration,
        endpoint_strategy: EndpointStrategy,
    ) -> Self {
        Self {
            services,
            state: Arc::new(AtomicUsize::new(0)),
            in_flight: Arc::new(Semaphore::new(1)),
            endpoint_timeout,
            endpoint_strategy,
        }
    }
}

impl Service<VectorRequest> for FailoverVectorService {
    type Response = VectorResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: VectorRequest) -> Self::Future {
        let services = self.services.clone();
        let state = Arc::clone(&self.state);
        let in_flight = Arc::clone(&self.in_flight);
        let endpoint_timeout = self.endpoint_timeout;
        let endpoint_strategy = self.endpoint_strategy;

        Box::pin(async move {
            let _permit = in_flight
                .acquire_owned()
                .await
                .expect("failover service semaphore should not be closed");
            let mut expected_state = state.load(Ordering::Acquire);
            let start = failover_state_index(expected_state, services.len());
            let mut last_error = None;
            let mut attempts = failover_attempt_indices(endpoint_strategy, start, services.len());
            let mut attempt = 0;
            let mut remaining_attempts = attempts.len();
            let mut tried = Vec::new();

            while remaining_attempts > 0 && attempt < attempts.len() {
                let index = attempts[attempt];
                let mut service = services[index].clone();
                tried.push(index);
                remaining_attempts -= 1;

                match tokio::time::timeout(endpoint_timeout, service.call(request.clone())).await {
                    Ok(Ok(response)) => {
                        return Ok(response);
                    }
                    Ok(Err(error)) => {
                        if !is_retriable_vector_error(&error) {
                            return Err(error);
                        }

                        let advance = failover_advance_if_current(
                            &state,
                            expected_state,
                            index,
                            failover_next_index(
                                endpoint_strategy,
                                attempts.as_slice(),
                                attempt,
                                services.len(),
                            ),
                            services.len(),
                        );
                        let next_attempts = failover_next_attempts(
                            endpoint_strategy,
                            services.len(),
                            attempts.as_mut(),
                            &mut attempt,
                            expected_state,
                            advance,
                            &tried,
                        );
                        expected_state = next_attempts.state;
                        if next_attempts.rebuilt {
                            remaining_attempts = attempts.len();
                        }
                        last_error = Some(error);
                    }
                    Err(_elapsed) => {
                        let advance = failover_advance_if_current(
                            &state,
                            expected_state,
                            index,
                            failover_next_index(
                                endpoint_strategy,
                                attempts.as_slice(),
                                attempt,
                                services.len(),
                            ),
                            services.len(),
                        );
                        let next_attempts = failover_next_attempts(
                            endpoint_strategy,
                            services.len(),
                            attempts.as_mut(),
                            &mut attempt,
                            expected_state,
                            advance,
                            &tried,
                        );
                        expected_state = next_attempts.state;
                        if next_attempts.rebuilt {
                            remaining_attempts = attempts.len();
                        }
                        last_error = Some(Box::new(VectorSinkError::Request {
                            source: tonic::Status::deadline_exceeded(
                                "vector endpoint request timed out",
                            ),
                        }) as crate::Error);
                    }
                }
            }

            Err(last_error.expect("failover service should have at least one endpoint"))
        })
    }
}

fn failover_request_settings(
    mut request_settings: TowerRequestSettings,
    endpoint_timeout: Duration,
    max_endpoint_attempts: usize,
) -> TowerRequestSettings {
    request_settings.concurrency = Some(1);
    // The outer Tower timeout wraps the whole failover loop. Add one endpoint
    // timeout of slack so the final endpoint attempt is not aborted by
    // scheduling overhead after earlier attempts consume their per-endpoint
    // timeouts.
    request_settings.timeout = endpoint_timeout
        .checked_mul((max_endpoint_attempts + 1) as u32)
        .unwrap_or(endpoint_timeout);
    request_settings
}

fn failover_attempt_indices(
    endpoint_strategy: EndpointStrategy,
    start: usize,
    endpoints: usize,
) -> Vec<usize> {
    match endpoint_strategy {
        EndpointStrategy::Failover => failover_ring_attempt_indices(start, endpoints),
        EndpointStrategy::FailoverPrimary => failover_primary_attempt_indices(start, endpoints),
        EndpointStrategy::LoadBalance => unreachable!("load balancing uses a different service"),
    }
}

const fn failover_state_index(state: usize, endpoints: usize) -> usize {
    state % endpoints
}

const fn failover_next_state(state: usize, next_index: usize, endpoints: usize) -> usize {
    let generation = state / endpoints;
    (generation + 1) * endpoints + next_index
}

fn failover_primary_attempt_indices(start: usize, endpoints: usize) -> Vec<usize> {
    std::iter::once(start).chain(0..endpoints).collect()
}

fn failover_ring_attempt_indices(start: usize, endpoints: usize) -> Vec<usize> {
    (0..endpoints)
        .map(|offset| (start + offset) % endpoints)
        .collect()
}

fn failover_next_index(
    endpoint_strategy: EndpointStrategy,
    attempts: &[usize],
    attempt: usize,
    endpoint_count: usize,
) -> Option<usize> {
    attempts
        .get(attempt + 1)
        .copied()
        .or(match endpoint_strategy {
            EndpointStrategy::FailoverPrimary => Some(0),
            EndpointStrategy::Failover if endpoint_count > 0 => attempts
                .get(attempt)
                .map(|index| (index + 1) % endpoint_count),
            EndpointStrategy::Failover | EndpointStrategy::LoadBalance => None,
        })
}

#[derive(Debug, Eq, PartialEq)]
struct FailoverAdvance {
    state: usize,
    advanced: bool,
}

#[derive(Debug, Eq, PartialEq)]
struct FailoverNextAttempts {
    state: usize,
    rebuilt: bool,
}

fn failover_next_attempts(
    endpoint_strategy: EndpointStrategy,
    endpoints: usize,
    attempts: &mut Vec<usize>,
    attempt: &mut usize,
    expected_state: usize,
    advance: FailoverAdvance,
    tried: &[usize],
) -> FailoverNextAttempts {
    if advance.advanced || advance.state == expected_state {
        *attempt += 1;
        return FailoverNextAttempts {
            state: advance.state,
            rebuilt: false,
        };
    } else {
        *attempts = stale_failover_attempt_indices(
            endpoint_strategy,
            failover_state_index(advance.state, endpoints),
            endpoints,
            tried,
        );
        *attempt = 0;
    }

    FailoverNextAttempts {
        state: advance.state,
        rebuilt: true,
    }
}

fn stale_failover_attempt_indices(
    endpoint_strategy: EndpointStrategy,
    start: usize,
    endpoints: usize,
    tried: &[usize],
) -> Vec<usize> {
    let active_endpoint = start;
    let filter_tried = endpoint_strategy != EndpointStrategy::FailoverPrimary;
    std::iter::once(active_endpoint)
        .chain(
            failover_attempt_indices(endpoint_strategy, start, endpoints)
                .into_iter()
                .filter(move |index| {
                    *index != active_endpoint && (!filter_tried || !tried.contains(index))
                }),
        )
        .collect()
}

fn failover_advance_if_current(
    state: &AtomicUsize,
    expected_state: usize,
    index: usize,
    next_index: Option<usize>,
    endpoints: usize,
) -> FailoverAdvance {
    let Some(next_index) = next_index else {
        return FailoverAdvance {
            state: state.load(Ordering::Acquire),
            advanced: false,
        };
    };

    if failover_state_index(expected_state, endpoints) != index {
        return FailoverAdvance {
            state: state.load(Ordering::Acquire),
            advanced: false,
        };
    }

    let next_state = failover_next_state(expected_state, next_index, endpoints);
    match state.compare_exchange(
        expected_state,
        next_state,
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        Ok(_) => FailoverAdvance {
            state: next_state,
            advanced: true,
        },
        Err(actual) => FailoverAdvance {
            state: actual,
            advanced: false,
        },
    }
}

fn is_retriable_vector_error(error: &crate::Error) -> bool {
    error
        .downcast_ref::<VectorSinkError>()
        .is_none_or(|error| VectorGrpcRetryLogic.is_retriable_error(error))
}

impl VectorConfig {
    fn validate_endpoint_options(&self) -> crate::Result<()> {
        match (self.address.as_ref(), self.routing.as_ref()) {
            (Some(_), Some(_)) => Err(
                "`address` and `routing` options are mutually exclusive. Please use `routing.endpoints` for multiple Vector endpoints."
                    .into(),
            ),
            (None, None) => {
                Err("No Vector endpoint configured. Please set `address` or `routing.endpoints`.".into())
            }
            (None, Some(routing)) if routing.endpoints.is_empty() => {
                Err("`routing.endpoints` must contain at least one endpoint.".into())
            }
            (Some(_), None) | (None, Some(_)) => Ok(()),
        }
    }

    fn uris(&self, tls: bool) -> crate::Result<Vec<Uri>> {
        self.validate_endpoint_options()?;

        if let Some(address) = self.address.as_ref() {
            Ok(vec![with_default_scheme(address, tls)?])
        } else {
            self.routing
                .as_ref()
                .expect("routing must be present after validation")
                .endpoints
                .iter()
                .map(|endpoint| with_default_scheme(endpoint, tls))
                .collect()
        }
    }
}

/// Check to see if the remote service accepts new events.
async fn healthcheck(
    mut service: VectorService,
    options: SinkHealthcheckOptions,
) -> crate::Result<()> {
    if !options.enabled {
        return Ok(());
    }

    // Use the custom Vector health check
    // Note: Both custom and standard health checks behave identically - they just
    // return serving status without actual health validation. The Vector source
    // implements both protocols now for compatibility.
    let request = service.client.health_check(proto::HealthCheckRequest {});
    match request.await {
        Ok(response) => match proto::ServingStatus::try_from(response.into_inner().status) {
            Ok(proto::ServingStatus::Serving) => Ok(()),
            Ok(status) => Err(Box::new(VectorSinkError::Health {
                status: Some(status.as_str_name()),
            })),
            Err(_) => Err(Box::new(VectorSinkError::Health { status: None })),
        },
        Err(source) => Err(Box::new(VectorSinkError::Request { source })),
    }
}

fn healthchecks(
    client: hyper::Client<ProxyConnector<HttpsConnector<HttpConnector>>, BoxBody>,
    uris: &[Uri],
    options: SinkHealthcheckOptions,
    endpoint_strategy: EndpointStrategy,
) -> Healthcheck {
    if !options.enabled {
        return Box::pin(futures::future::ok(()));
    }

    let healthcheck_uris = healthcheck_uris_for_strategy(uris, &options, endpoint_strategy);

    let healthchecks = healthcheck_uris.into_iter().map(move |uri| {
        let service = VectorService::new(client.clone(), uri, VectorCompression::None);
        let timeout = options.timeout;
        healthcheck(
            service,
            SinkHealthcheckOptions {
                enabled: true,
                uri: None,
                timeout,
            },
        )
        .boxed()
    });

    Box::pin(futures::future::try_join_all(healthchecks).map_ok(|_| ()))
}

const fn requires_all_endpoint_healthchecks(
    endpoint_strategy: EndpointStrategy,
    endpoint_count: usize,
) -> bool {
    matches!(endpoint_strategy, EndpointStrategy::LoadBalance) && endpoint_count > 1
}

fn healthcheck_uris_for_strategy(
    uris: &[Uri],
    options: &SinkHealthcheckOptions,
    endpoint_strategy: EndpointStrategy,
) -> Vec<Uri> {
    if requires_all_endpoint_healthchecks(endpoint_strategy, uris.len()) {
        return uris.to_vec();
    }

    if let Some(uri) = options.uri.clone() {
        return vec![uri.uri];
    }

    match endpoint_strategy {
        EndpointStrategy::Failover | EndpointStrategy::FailoverPrimary => {
            uris.first().cloned().into_iter().collect()
        }
        EndpointStrategy::LoadBalance => uris.to_vec(),
    }
}

const fn default_endpoint_health_config() -> HealthConfig {
    HealthConfig {
        retry_initial_backoff_secs: 1,
        retry_max_duration_secs: Duration::from_secs(60 * 60),
    }
}

/// grpc doesn't like an address without a scheme, so we default to http or https if one isn't
/// specified in the address.
pub fn with_default_scheme(address: &str, tls: bool) -> crate::Result<Uri> {
    let uri: Uri = address.parse()?;
    if uri.scheme().is_none() {
        // Default the scheme to http or https.
        let mut parts = uri.into_parts();

        parts.scheme = if tls {
            Some(
                "https"
                    .parse()
                    .unwrap_or_else(|_| unreachable!("https should be valid")),
            )
        } else {
            Some(
                "http"
                    .parse()
                    .unwrap_or_else(|_| unreachable!("http should be valid")),
            )
        };

        if parts.path_and_query.is_none() {
            parts.path_and_query = Some(
                "/".parse()
                    .unwrap_or_else(|_| unreachable!("root should be valid")),
            );
        }
        Ok(Uri::from_parts(parts)?)
    } else {
        Ok(uri)
    }
}

fn new_client(
    tls_settings: &MaybeTlsSettings,
    proxy_config: &ProxyConfig,
    keepalive: Option<VectorKeepaliveConfig>,
) -> crate::Result<hyper::Client<ProxyConnector<HttpsConnector<HttpConnector>>, BoxBody>> {
    let proxy = build_proxy_connector(tls_settings.clone(), proxy_config)?;

    let mut builder = hyper::Client::builder();
    builder.http2_only(true);

    // Keepalive is opt-in. When enabled, PINGs are sent on idle connections so dead connections
    // are detected and evicted before they are reused, not during a request.
    if let Some(keepalive) = keepalive {
        builder
            .http2_keep_alive_interval(Duration::from_secs(keepalive.interval_secs.get()))
            .http2_keep_alive_timeout(Duration::from_secs(keepalive.timeout_secs.get()))
            // Always ping idle connections: the downstream is always a Vector instance, which
            // won't reject pings without active calls, so idle-keepalive is always safe here.
            .http2_keep_alive_while_idle(true);
    }

    Ok(builder.build(proxy))
}

#[derive(Debug, Clone)]
struct VectorGrpcRetryLogic;

impl RetryLogic for VectorGrpcRetryLogic {
    type Error = VectorSinkError;
    type Request = VectorRequest;
    type Response = VectorResponse;

    fn is_retriable_error(&self, err: &Self::Error) -> bool {
        use tonic::Code::*;

        match err {
            VectorSinkError::Request { source } => !matches!(
                source.code(),
                // List taken from
                //
                // <https://github.com/grpc/grpc/blob/ed1b20777c69bd47e730a63271eafc1b299f6ca0/doc/statuscodes.md>
                NotFound
                    | InvalidArgument
                    | AlreadyExists
                    | PermissionDenied
                    | OutOfRange
                    | Unimplemented
                    | Unauthenticated
                    | DataLoss
            ),
            _ => true,
        }
    }
}

#[derive(Debug, Clone)]
struct VectorGrpcHealthLogic;

impl HealthLogic for VectorGrpcHealthLogic {
    type Error = crate::Error;
    type Response = VectorResponse;

    fn is_healthy(&self, response: &Result<Self::Response, Self::Error>) -> Option<bool> {
        match response {
            Ok(_) => Some(true),
            Err(error) if is_retriable_vector_error(error) => Some(false),
            Err(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinks::util::UriSerde;

    #[test]
    fn failover_request_settings_force_serial_concurrency() {
        let mut settings = TowerRequestConfig::<
            crate::sinks::util::service::GlobalTowerRequestConfigDefaults,
        >::default()
        .into_settings();
        settings.concurrency = Some(8);
        settings.timeout = Duration::from_secs(5);

        let settings = failover_request_settings(settings, Duration::from_secs(5), 3);

        assert_eq!(settings.concurrency, Some(1));
        assert_eq!(settings.timeout, Duration::from_secs(20));
    }

    #[test]
    fn failover_service_clones_share_single_in_flight_permit() {
        let service = FailoverVectorService::new(
            Vec::new(),
            Duration::from_secs(1),
            EndpointStrategy::Failover,
        );
        let cloned = service.clone();

        let permit = Arc::clone(&service.in_flight).try_acquire_owned().unwrap();

        assert!(
            Arc::clone(&cloned.in_flight).try_acquire_owned().is_err(),
            "cloned failover services must share one request permit"
        );

        drop(permit);

        assert!(Arc::clone(&cloned.in_flight).try_acquire_owned().is_ok());
    }

    #[test]
    fn health_logic_ignores_non_retriable_vector_errors() {
        let response = Err(Box::new(VectorSinkError::Request {
            source: tonic::Status::data_loss("batch rejected"),
        }) as crate::Error);

        assert_eq!(VectorGrpcHealthLogic.is_healthy(&response), None);
    }

    #[test]
    fn health_logic_marks_retriable_vector_errors_unhealthy() {
        let response = Err(Box::new(VectorSinkError::Request {
            source: tonic::Status::unavailable("endpoint unavailable"),
        }) as crate::Error);

        assert_eq!(VectorGrpcHealthLogic.is_healthy(&response), Some(false));
    }

    #[test]
    fn parse_routing_health_config() {
        let config: VectorConfig = toml::from_str(
            r#"
                [routing]
                endpoints = ["http://127.0.0.1:6000", "http://127.0.0.1:6001"]

                [routing.health]
                retry_initial_backoff_secs = 2
                retry_max_duration_secs = 30
            "#,
        )
        .unwrap();

        let health = config
            .routing
            .as_ref()
            .and_then(|routing| routing.health.as_ref())
            .expect("routing.health should parse");

        assert_eq!(health.retry_initial_backoff_secs, 2);
        assert_eq!(health.retry_max_duration_secs, Duration::from_secs(30));
    }

    #[test]
    fn failover_advance_ignores_stale_generation() {
        let endpoints = 2;
        let state = AtomicUsize::new(failover_next_state(
            failover_next_state(0, 1, endpoints),
            0,
            endpoints,
        ));

        let observed = failover_advance_if_current(&state, 0, 0, Some(1), endpoints);

        assert_eq!(
            observed,
            FailoverAdvance {
                state: 4,
                advanced: false,
            }
        );
        assert_eq!(state.load(Ordering::Acquire), 4);
    }

    #[test]
    fn failover_advance_ignores_stale_mismatched_state() {
        let endpoints = 3;
        let shared_state = failover_next_state(failover_next_state(0, 1, endpoints), 0, endpoints);
        let stale_state = 1;
        let state = AtomicUsize::new(shared_state);

        let observed = failover_advance_if_current(&state, stale_state, 0, Some(1), endpoints);

        assert_eq!(
            observed,
            FailoverAdvance {
                state: shared_state,
                advanced: false,
            }
        );
        assert_eq!(state.load(Ordering::Acquire), shared_state);
    }

    #[test]
    fn failover_primary_attempts_current_then_configured_order() {
        assert_eq!(failover_primary_attempt_indices(1, 3), vec![1, 0, 1, 2]);
    }

    #[test]
    fn failover_attempts_current_then_ring_order() {
        assert_eq!(failover_ring_attempt_indices(1, 3), vec![1, 2, 0]);
    }

    #[test]
    fn failover_advance_ignores_current_non_matching_endpoint() {
        let endpoints = 3;
        let state = AtomicUsize::new(5);

        let observed = failover_advance_if_current(&state, 0, 0, Some(1), endpoints);

        assert_eq!(
            observed,
            FailoverAdvance {
                state: 5,
                advanced: false,
            }
        );
        assert_eq!(state.load(Ordering::Acquire), 5);
    }

    #[test]
    fn failover_advance_ignores_missing_next_endpoint() {
        let state = AtomicUsize::new(0);

        let observed = failover_advance_if_current(&state, 0, 0, None, 2);

        assert_eq!(
            observed,
            FailoverAdvance {
                state: 0,
                advanced: false,
            }
        );
        assert_eq!(state.load(Ordering::Acquire), 0);
    }

    #[test]
    fn failover_primary_final_attempt_wraps_state_to_primary() {
        let attempts = failover_primary_attempt_indices(2, 3);
        let state = AtomicUsize::new(5);

        let observed = failover_advance_if_current(
            &state,
            5,
            2,
            failover_next_index(
                EndpointStrategy::FailoverPrimary,
                &attempts,
                attempts.len() - 1,
                3,
            ),
            3,
        );

        assert_eq!(
            observed,
            FailoverAdvance {
                state: 6,
                advanced: true,
            }
        );
        assert_eq!(state.load(Ordering::Acquire), 6);
    }

    #[test]
    fn failover_final_attempt_wraps_state_to_next_pass_start() {
        let attempts = failover_ring_attempt_indices(0, 3);
        let state = AtomicUsize::new(2);

        let observed = failover_advance_if_current(
            &state,
            2,
            2,
            failover_next_index(EndpointStrategy::Failover, &attempts, attempts.len() - 1, 3),
            3,
        );

        assert_eq!(
            observed,
            FailoverAdvance {
                state: 3,
                advanced: true,
            }
        );
        assert_eq!(state.load(Ordering::Acquire), 3);
    }

    #[test]
    fn failover_next_attempts_recomputes_after_concurrent_advance() {
        let mut attempts = failover_ring_attempt_indices(0, 3);
        let mut attempt = 0;
        let mut remaining_attempts = 2;

        let observed = failover_next_attempts(
            EndpointStrategy::Failover,
            3,
            &mut attempts,
            &mut attempt,
            0,
            FailoverAdvance {
                state: 5,
                advanced: false,
            },
            &[0],
        );
        if observed.rebuilt {
            remaining_attempts = attempts.len();
        }

        assert_eq!(observed.state, 5);
        assert!(observed.rebuilt);
        assert_eq!(attempt, 0);
        assert_eq!(attempts, vec![2, 1]);
        assert_eq!(remaining_attempts, attempts.len());
    }

    #[test]
    fn failover_next_attempts_restarts_after_stale_same_endpoint_generation() {
        let mut attempts = failover_ring_attempt_indices(0, 2);
        let mut attempt = 0;
        let mut remaining_attempts = 1;

        let observed = failover_next_attempts(
            EndpointStrategy::Failover,
            2,
            &mut attempts,
            &mut attempt,
            0,
            FailoverAdvance {
                state: 4,
                advanced: false,
            },
            &[0],
        );
        if observed.rebuilt {
            remaining_attempts = attempts.len();
        }

        assert_eq!(observed.state, 4);
        assert!(observed.rebuilt);
        assert_eq!(attempt, 0);
        assert_eq!(attempts, vec![0, 1]);
        assert_eq!(remaining_attempts, attempts.len());
    }

    #[test]
    fn failover_next_attempts_restarts_after_stale_wrapped_generation() {
        let mut attempts = failover_ring_attempt_indices(0, 3);
        let mut attempt = 0;
        let mut remaining_attempts = 1;

        let observed = failover_next_attempts(
            EndpointStrategy::Failover,
            3,
            &mut attempts,
            &mut attempt,
            0,
            FailoverAdvance {
                state: 6,
                advanced: false,
            },
            &[0],
        );
        if observed.rebuilt {
            remaining_attempts = attempts.len();
        }

        assert_eq!(observed.state, 6);
        assert!(observed.rebuilt);
        assert_eq!(attempt, 0);
        assert_eq!(attempts, vec![0, 1, 2]);
        assert_eq!(remaining_attempts, attempts.len());
    }

    #[test]
    fn failover_next_attempts_preserves_failover_primary_after_duplicate_primary_advance() {
        let mut attempts = failover_primary_attempt_indices(0, 3);
        let mut attempt = 0;
        let mut remaining_attempts = 1;

        let observed = failover_next_attempts(
            EndpointStrategy::FailoverPrimary,
            3,
            &mut attempts,
            &mut attempt,
            0,
            FailoverAdvance {
                state: 3,
                advanced: false,
            },
            &[0],
        );
        if observed.rebuilt {
            remaining_attempts = attempts.len();
        }

        assert_eq!(observed.state, 3);
        assert!(observed.rebuilt);
        assert_eq!(attempt, 0);
        assert_eq!(attempts, vec![0, 1, 2]);
        assert_eq!(remaining_attempts, attempts.len());
    }

    #[test]
    fn failover_primary_stale_rebuild_rechecks_primary_before_secondaries() {
        let mut attempts = failover_primary_attempt_indices(0, 3);
        let mut attempt = 0;
        let mut remaining_attempts = 2;

        let observed = failover_next_attempts(
            EndpointStrategy::FailoverPrimary,
            3,
            &mut attempts,
            &mut attempt,
            0,
            FailoverAdvance {
                state: 5,
                advanced: false,
            },
            &[0, 1],
        );
        if observed.rebuilt {
            remaining_attempts = attempts.len();
        }

        assert_eq!(observed.state, 5);
        assert!(observed.rebuilt);
        assert_eq!(attempt, 0);
        assert_eq!(attempts, vec![2, 0, 1]);
        assert_eq!(remaining_attempts, attempts.len());
    }

    #[test]
    fn failover_next_attempts_keeps_shared_active_endpoint_after_stale_wrap() {
        let mut attempts = failover_ring_attempt_indices(0, 3);
        let mut attempt = 1;
        let mut remaining_attempts = 1;

        let observed = failover_next_attempts(
            EndpointStrategy::Failover,
            3,
            &mut attempts,
            &mut attempt,
            1,
            FailoverAdvance {
                state: 6,
                advanced: false,
            },
            &[0, 1],
        );
        if observed.rebuilt {
            remaining_attempts = attempts.len();
        }

        assert_eq!(observed.state, 6);
        assert!(observed.rebuilt);
        assert_eq!(attempt, 0);
        assert_eq!(attempts, vec![0, 2]);
        assert_eq!(remaining_attempts, attempts.len());
    }

    #[test]
    fn failover_next_attempts_continues_after_local_advance() {
        let mut attempts = failover_primary_attempt_indices(1, 3);
        let mut attempt = 0;
        let remaining_attempts = 3;

        let observed = failover_next_attempts(
            EndpointStrategy::FailoverPrimary,
            3,
            &mut attempts,
            &mut attempt,
            1,
            FailoverAdvance {
                state: 3,
                advanced: true,
            },
            &[1],
        );

        assert_eq!(observed.state, 3);
        assert!(!observed.rebuilt);
        assert_eq!(attempt, 1);
        assert_eq!(attempts, vec![1, 0, 1, 2]);
        assert_eq!(remaining_attempts, 3);
    }

    #[test]
    fn only_load_balancing_requires_all_endpoint_healthchecks() {
        assert!(requires_all_endpoint_healthchecks(
            EndpointStrategy::LoadBalance,
            2
        ));
        assert!(!requires_all_endpoint_healthchecks(
            EndpointStrategy::LoadBalance,
            1
        ));
        assert!(!requires_all_endpoint_healthchecks(
            EndpointStrategy::Failover,
            2
        ));
        assert!(!requires_all_endpoint_healthchecks(
            EndpointStrategy::FailoverPrimary,
            2
        ));
    }

    #[test]
    fn load_balancing_healthchecks_all_configured_endpoints_even_with_override_uri() {
        let endpoints = vec![
            "http://endpoint-a.example.com".parse().unwrap(),
            "http://endpoint-b.example.com".parse().unwrap(),
        ];
        let options = SinkHealthcheckOptions {
            uri: Some("http://health.example.com".parse::<UriSerde>().unwrap()),
            ..Default::default()
        };

        assert_eq!(
            healthcheck_uris_for_strategy(&endpoints, &options, EndpointStrategy::LoadBalance),
            endpoints
        );
    }

    #[test]
    fn single_endpoint_load_balancing_healthcheck_can_use_override_uri() {
        let endpoints = vec!["http://endpoint-a.example.com".parse().unwrap()];
        let override_uri = "http://health.example.com".parse::<UriSerde>().unwrap().uri;
        let options = SinkHealthcheckOptions {
            uri: Some(UriSerde {
                uri: override_uri.clone(),
                auth: None,
            }),
            ..Default::default()
        };

        assert_eq!(
            healthcheck_uris_for_strategy(&endpoints, &options, EndpointStrategy::LoadBalance),
            vec![override_uri]
        );
    }

    #[test]
    fn failover_healthchecks_can_use_override_uri() {
        let endpoints = vec![
            "http://endpoint-a.example.com".parse().unwrap(),
            "http://endpoint-b.example.com".parse().unwrap(),
        ];
        let override_uri = "http://health.example.com".parse::<UriSerde>().unwrap().uri;
        let options = SinkHealthcheckOptions {
            uri: Some(UriSerde {
                uri: override_uri.clone(),
                auth: None,
            }),
            ..Default::default()
        };

        assert_eq!(
            healthcheck_uris_for_strategy(&endpoints, &options, EndpointStrategy::Failover),
            vec![override_uri]
        );
    }

    #[test]
    fn failover_healthchecks_active_endpoint_without_override_uri() {
        let endpoints = vec![
            "http://endpoint-a.example.com".parse().unwrap(),
            "http://endpoint-b.example.com".parse().unwrap(),
        ];
        let options = SinkHealthcheckOptions::default();

        assert_eq!(
            healthcheck_uris_for_strategy(&endpoints, &options, EndpointStrategy::Failover),
            vec![endpoints[0].clone()]
        );
    }

    #[test]
    fn failover_primary_healthchecks_can_use_override_uri() {
        let endpoints = vec![
            "http://endpoint-a.example.com".parse().unwrap(),
            "http://endpoint-b.example.com".parse().unwrap(),
        ];
        let override_uri = "http://health.example.com".parse::<UriSerde>().unwrap().uri;
        let options = SinkHealthcheckOptions {
            uri: Some(UriSerde {
                uri: override_uri.clone(),
                auth: None,
            }),
            ..Default::default()
        };

        assert_eq!(
            healthcheck_uris_for_strategy(&endpoints, &options, EndpointStrategy::FailoverPrimary),
            vec![override_uri]
        );
    }

    #[test]
    fn failover_primary_healthchecks_primary_without_override_uri() {
        let endpoints = vec![
            "http://endpoint-a.example.com".parse().unwrap(),
            "http://endpoint-b.example.com".parse().unwrap(),
        ];
        let options = SinkHealthcheckOptions::default();

        assert_eq!(
            healthcheck_uris_for_strategy(&endpoints, &options, EndpointStrategy::FailoverPrimary),
            vec![endpoints[0].clone()]
        );
    }
}

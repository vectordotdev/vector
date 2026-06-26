use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
};

use futures::{FutureExt, TryFutureExt, future::BoxFuture};
use http::Uri;
use hyper::client::HttpConnector;
use hyper_openssl::HttpsConnector;
use hyper_proxy::ProxyConnector;
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
            service::{HealthConfig, HealthLogic, ServiceBuilderExt},
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
    /// This option is mutually exclusive with `addresses`. Set exactly one of
    /// `address` or `addresses`.
    #[configurable(validation(format = "uri"))]
    #[configurable(metadata(docs::examples = "92.12.333.224:6000"))]
    #[configurable(metadata(docs::examples = "https://somehost:6000"))]
    #[serde(default)]
    address: Option<String>,

    /// The downstream Vector addresses to which to connect.
    ///
    /// Both IP addresses and hostnames are accepted formats.
    ///
    /// Each address _must_ include a port.
    ///
    /// This option is mutually exclusive with `address`. Set exactly one of
    /// `address` or `addresses`.
    #[configurable(validation(format = "uri"))]
    #[configurable(metadata(docs::examples = "92.12.333.224:6000"))]
    #[configurable(metadata(docs::examples = "https://somehost:6000"))]
    #[serde(default)]
    addresses: Vec<String>,

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

    /// Options for determining the health of Vector endpoints.
    #[serde(default)]
    #[configurable(derived)]
    pub endpoint_health: Option<HealthConfig>,

    /// Strategy for routing requests across multiple configured addresses.
    ///
    /// This option is only used when `addresses` is configured.
    #[serde(default)]
    pub endpoint_strategy: EndpointStrategy,

    #[configurable(derived)]
    #[serde(default)]
    tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub(in crate::sinks::vector) acknowledgements: AcknowledgementsConfig,
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
        addresses: Vec::new(),
        compression: VectorCompression::None,
        batch: BatchConfig::default(),
        request: TowerRequestConfig::default(),
        endpoint_health: None,
        endpoint_strategy: EndpointStrategy::default(),
        tls: None,
        acknowledgements: Default::default(),
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "vector")]
impl SinkConfig for VectorConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSinkType, Healthcheck)> {
        let tls = MaybeTlsSettings::from_config(self.tls.as_ref(), false)?;
        let uris = self.uris(tls.is_tls())?;

        let client = new_client(&tls, cx.proxy())?;

        let healthcheck = healthchecks(client.clone(), &uris, cx.healthcheck);
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

        let sink = match self.endpoint_strategy {
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
                    self.endpoint_health.clone().unwrap_or_default(),
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
                let max_endpoint_attempts = match self.endpoint_strategy {
                    EndpointStrategy::Failover => services.len(),
                    EndpointStrategy::FailoverPrimary => services.len() + 1,
                    EndpointStrategy::LoadBalance => {
                        unreachable!("load balancing uses a different service")
                    }
                };
                let mut failover_request_settings = request_settings;
                // The outer Tower timeout wraps the whole failover loop. Add one
                // endpoint timeout of slack so the final endpoint attempt is not
                // aborted by scheduling overhead after earlier attempts consume
                // their per-endpoint timeouts.
                failover_request_settings.timeout = endpoint_timeout
                    .checked_mul((max_endpoint_attempts + 1) as u32)
                    .unwrap_or(endpoint_timeout);

                let service = ServiceBuilder::new()
                    .settings(failover_request_settings, VectorGrpcRetryLogic)
                    .service(FailoverVectorService::new(
                        services
                            .into_iter()
                            .map(|(_endpoint, service)| service)
                            .collect(),
                        endpoint_timeout,
                        self.endpoint_strategy,
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
    /// Distribute requests across healthy endpoints.
    #[default]
    LoadBalance,
    /// Use one endpoint at a time. When the active endpoint fails, continue
    /// through the configured addresses from the next endpoint.
    ///
    /// This mode keeps using the last successful endpoint until it fails. Use
    /// `failover_primary` instead when retriable failures should re-check the
    /// first configured address before trying secondary endpoints.
    Failover,
    /// Use one endpoint at a time. When the active endpoint fails, retry from
    /// the configured address order so the sink can return to its configured
    /// primary endpoint.
    ///
    /// This is useful when receiver-side connection recycling, such as
    /// `max_connection_age_secs`, should converge the sink back to the first
    /// configured address when it is available.
    FailoverPrimary,
}

#[derive(Clone)]
struct FailoverVectorService {
    services: Vec<VectorService>,
    state: Arc<AtomicUsize>,
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
        let endpoint_timeout = self.endpoint_timeout;
        let endpoint_strategy = self.endpoint_strategy;

        Box::pin(async move {
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
                            attempts.get(attempt + 1).copied(),
                            services.len(),
                        );
                        expected_state = failover_next_attempts(
                            endpoint_strategy,
                            services.len(),
                            attempts.as_mut(),
                            &mut attempt,
                            expected_state,
                            advance,
                            &tried,
                        );
                        last_error = Some(error);
                    }
                    Err(_elapsed) => {
                        let advance = failover_advance_if_current(
                            &state,
                            expected_state,
                            index,
                            attempts.get(attempt + 1).copied(),
                            services.len(),
                        );
                        expected_state = failover_next_attempts(
                            endpoint_strategy,
                            services.len(),
                            attempts.as_mut(),
                            &mut attempt,
                            expected_state,
                            advance,
                            &tried,
                        );
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

#[derive(Debug, Eq, PartialEq)]
struct FailoverAdvance {
    state: usize,
    advanced: bool,
}

fn failover_next_attempts(
    endpoint_strategy: EndpointStrategy,
    endpoints: usize,
    attempts: &mut Vec<usize>,
    attempt: &mut usize,
    expected_state: usize,
    advance: FailoverAdvance,
    tried: &[usize],
) -> usize {
    if advance.advanced
        || advance.state == expected_state
        || failover_state_index(advance.state, endpoints)
            == failover_state_index(expected_state, endpoints)
    {
        *attempt += 1;
    } else {
        *attempts = failover_attempt_indices(
            endpoint_strategy,
            failover_state_index(advance.state, endpoints),
            endpoints,
        )
        .into_iter()
        .filter(|index| !tried.contains(index))
        .collect();
        *attempt = 0;
    }

    advance.state
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
    fn uris(&self, tls: bool) -> crate::Result<Vec<Uri>> {
        match (self.address.as_ref(), self.addresses.as_slice()) {
            (Some(_), [_first, ..]) => Err(
                "`address` and `addresses` options are mutually exclusive. Please use `addresses` for multiple Vector endpoints."
                    .into(),
            ),
            (None, []) => Err("No Vector endpoint configured. Please set `address` or `addresses`.".into()),
            (Some(address), []) => Ok(vec![with_default_scheme(address, tls)?]),
            (None, addresses) => addresses
                .iter()
                .map(|address| with_default_scheme(address, tls))
                .collect(),
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
) -> Healthcheck {
    if !options.enabled {
        return Box::pin(futures::future::ok(()));
    }

    let healthcheck_uris = options
        .uri
        .clone()
        .map(|uri| vec![uri.uri])
        .unwrap_or_else(|| uris.to_vec());

    Box::pin(
        futures::future::select_ok(healthcheck_uris.into_iter().map(move |uri| {
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
        }))
        .map_ok(|((), _)| ()),
    )
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
) -> crate::Result<hyper::Client<ProxyConnector<HttpsConnector<HttpConnector>>, BoxBody>> {
    let proxy = build_proxy_connector(tls_settings.clone(), proxy_config)?;

    Ok(hyper::Client::builder().http2_only(true).build(proxy))
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
    fn failover_next_attempts_recomputes_after_concurrent_advance() {
        let mut attempts = failover_ring_attempt_indices(0, 3);
        let mut attempt = 0;

        let observed_state = failover_next_attempts(
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

        assert_eq!(observed_state, 5);
        assert_eq!(attempt, 0);
        assert_eq!(attempts, vec![2, 1]);
    }

    #[test]
    fn failover_next_attempts_continues_after_stale_same_endpoint_generation() {
        let mut attempts = failover_ring_attempt_indices(0, 2);
        let mut attempt = 0;

        let observed_state = failover_next_attempts(
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

        assert_eq!(observed_state, 4);
        assert_eq!(attempt, 1);
        assert_eq!(attempts, vec![0, 1]);
    }

    #[test]
    fn failover_next_attempts_continues_after_local_advance() {
        let mut attempts = failover_primary_attempt_indices(1, 3);
        let mut attempt = 0;

        let observed_state = failover_next_attempts(
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

        assert_eq!(observed_state, 3);
        assert_eq!(attempt, 1);
        assert_eq!(attempts, vec![1, 0, 1, 2]);
    }
}

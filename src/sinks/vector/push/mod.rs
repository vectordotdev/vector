pub mod compression;
pub mod retry_logic;
pub mod service;
pub mod sink;
pub mod sink_error;
use crate::sinks::util::service::ServiceBuilderExt;
use futures_util::{FutureExt, TryFutureExt};

#[derive(Clone)]
struct FailoverVectorService {
    services: Vec<service::VectorService>,
    state: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    in_flight: std::sync::Arc<tokio::sync::Semaphore>,
    endpoint_timeout: std::time::Duration,
    endpoint_strategy: EndpointStrategy,
}

impl FailoverVectorService {
    fn new(
        services: Vec<service::VectorService>,
        endpoint_timeout: std::time::Duration,
        endpoint_strategy: EndpointStrategy,
    ) -> Self {
        Self {
            services,
            state: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            in_flight: std::sync::Arc::new(tokio::sync::Semaphore::new(1)),
            endpoint_timeout,
            endpoint_strategy,
        }
    }
}

impl tower::Service<service::VectorRequest> for FailoverVectorService {
    type Response = service::VectorResponse;
    type Error = crate::Error;
    type Future = futures::future::BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: service::VectorRequest) -> Self::Future {
        let services = self.services.clone();
        let state = std::sync::Arc::clone(&self.state);
        let in_flight = std::sync::Arc::clone(&self.in_flight);
        let endpoint_timeout = self.endpoint_timeout;
        let endpoint_strategy = self.endpoint_strategy;

        Box::pin(async move {
            let _permit = in_flight
                .acquire_owned()
                .await
                .expect("failover service semaphore should not be closed");
            let mut expected_state = state.load(std::sync::atomic::Ordering::Acquire);
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
                        if !retry_logic::is_retriable_vector_error(&error) {
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
                        last_error = Some(Box::new(sink_error::VectorSinkError::Request {
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
    mut request_settings: crate::sinks::util::TowerRequestSettings,
    endpoint_timeout: std::time::Duration,
    max_endpoint_attempts: usize,
) -> crate::sinks::util::TowerRequestSettings {
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
    state: &std::sync::atomic::AtomicUsize,
    expected_state: usize,
    index: usize,
    next_index: Option<usize>,
    endpoints: usize,
) -> FailoverAdvance {
    let Some(next_index) = next_index else {
        return FailoverAdvance {
            state: state.load(std::sync::atomic::Ordering::Acquire),
            advanced: false,
        };
    };

    if failover_state_index(expected_state, endpoints) != index {
        return FailoverAdvance {
            state: state.load(std::sync::atomic::Ordering::Acquire),
            advanced: false,
        };
    }

    let next_state = failover_next_state(expected_state, next_index, endpoints);
    match state.compare_exchange(
        expected_state,
        next_state,
        std::sync::atomic::Ordering::AcqRel,
        std::sync::atomic::Ordering::Acquire,
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

#[derive(Debug, Clone)]
struct VectorGrpcHealthLogic;

impl crate::sinks::util::service::HealthLogic for VectorGrpcHealthLogic {
    type Error = crate::Error;
    type Response = service::VectorResponse;

    fn is_healthy(&self, response: &Result<Self::Response, Self::Error>) -> Option<bool> {
        match response {
            Ok(_) => Some(true),
            Err(error) if retry_logic::is_retriable_vector_error(error) => Some(false),
            Err(_) => None,
        }
    }
}

fn new_client(
    tls_settings: &vector_lib::tls::MaybeTlsSettings,
    proxy_config: &vector_lib::config::proxy::ProxyConfig,
    keepalive: Option<super::config::VectorKeepaliveConfig>,
) -> crate::Result<
    hyper::Client<
        hyper_proxy::ProxyConnector<hyper_openssl::HttpsConnector<hyper::client::HttpConnector>>,
        tonic::body::BoxBody,
    >,
> {
    let proxy = crate::http::build_proxy_connector(tls_settings.clone(), proxy_config)?;

    let mut builder = hyper::Client::builder();
    builder.http2_only(true);

    // Keepalive is opt-in. When enabled, PINGs are sent on idle connections so dead connections
    // are detected and evicted before they are reused, not during a request.
    if let Some(keepalive) = keepalive {
        builder
            .http2_keep_alive_interval(std::time::Duration::from_secs(
                keepalive.interval_secs.get(),
            ))
            .http2_keep_alive_timeout(std::time::Duration::from_secs(keepalive.timeout_secs.get()))
            // Always ping idle connections: the downstream is always a Vector instance, which
            // won't reject pings without active calls, so idle-keepalive is always safe here.
            .http2_keep_alive_while_idle(true);
    }

    Ok(builder.build(proxy))
}

const fn requires_all_endpoint_healthchecks(
    endpoint_strategy: EndpointStrategy,
    endpoint_count: usize,
) -> bool {
    matches!(endpoint_strategy, EndpointStrategy::LoadBalance) && endpoint_count > 1
}

fn healthcheck_uris_for_strategy(
    uris: &[http::Uri],
    options: &crate::config::SinkHealthcheckOptions,
    endpoint_strategy: EndpointStrategy,
) -> Vec<http::Uri> {
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

const fn default_endpoint_health_config() -> crate::sinks::util::service::HealthConfig {
    crate::sinks::util::service::HealthConfig {
        retry_initial_backoff_secs: 1,
        retry_max_duration_secs: std::time::Duration::from_secs(60 * 60),
    }
}

/// Check to see if the remote service accepts new events.
async fn healthcheck(
    mut service: service::VectorService,
    options: crate::config::SinkHealthcheckOptions,
) -> crate::Result<()> {
    if !options.enabled {
        return Ok(());
    }

    // Use the custom Vector health check
    // Note: Both custom and standard health checks behave identically - they just
    // return serving status without actual health validation. The Vector source
    // implements both protocols now for compatibility.
    let request = service
        .client
        .health_check(crate::proto::vector::HealthCheckRequest {});
    match request.await {
        Ok(response) => {
            match crate::proto::vector::ServingStatus::try_from(response.into_inner().status) {
                Ok(crate::proto::vector::ServingStatus::Serving) => Ok(()),
                Ok(status) => Err(Box::new(sink_error::VectorSinkError::Health {
                    status: Some(status.as_str_name()),
                })),
                Err(_) => Err(Box::new(sink_error::VectorSinkError::Health {
                    status: None,
                })),
            }
        }
        Err(source) => Err(Box::new(sink_error::VectorSinkError::Request { source })),
    }
}

fn healthchecks(
    client: hyper::Client<
        hyper_proxy::ProxyConnector<hyper_openssl::HttpsConnector<hyper::client::HttpConnector>>,
        tonic::body::BoxBody,
    >,
    uris: &[http::Uri],
    options: crate::config::SinkHealthcheckOptions,
    endpoint_strategy: EndpointStrategy,
) -> crate::sinks::Healthcheck {
    if !options.enabled {
        return Box::pin(futures::future::ok(()));
    }

    let healthcheck_uris = healthcheck_uris_for_strategy(uris, &options, endpoint_strategy);

    let healthchecks = healthcheck_uris.into_iter().map(move |uri| {
        let service =
            service::VectorService::new(client.clone(), uri, compression::VectorCompression::None);
        let timeout = options.timeout;
        healthcheck(
            service,
            crate::config::SinkHealthcheckOptions {
                enabled: true,
                uri: None,
                timeout,
            },
        )
        .boxed()
    });

    Box::pin(futures::future::try_join_all(healthchecks).map_ok(|_| ()))
}

pub fn config_to_push_sink(
    config: &super::config::VectorConfig,
    cx: crate::config::SinkContext,
) -> crate::Result<(crate::sinks::VectorSink, crate::sinks::Healthcheck)> {
    let tls = vector_lib::tls::MaybeTlsSettings::from_config(config.tls.as_ref(), false)?;
    let uris = config.uris(tls.is_tls())?;
    let endpoint_strategy = config
        .routing
        .as_ref()
        .map_or_else(EndpointStrategy::default, |routing| routing.strategy);

    let client = new_client(&tls, cx.proxy(), config.keepalive)?;

    let healthcheck = healthchecks(client.clone(), &uris, cx.healthcheck, endpoint_strategy);
    let request_settings = config.request.into_settings();
    let batch_settings = config.batch.into_batcher_settings()?;

    let services = uris
        .into_iter()
        .map(|uri| {
            let endpoint = uri.to_string();
            let service = service::VectorService::new(client.clone(), uri, config.compression);
            (endpoint, service)
        })
        .collect::<Vec<_>>();

    let sink = match endpoint_strategy {
        _ if services.len() == 1 => {
            let service = tower::ServiceBuilder::new()
                .settings(request_settings, retry_logic::VectorGrpcRetryLogic)
                .service(services.into_iter().next().expect("one service").1);

            crate::sinks::VectorSink::from_event_streamsink(sink::VectorSink {
                batch_settings,
                service,
            })
        }
        EndpointStrategy::LoadBalance => {
            let service = request_settings.distributed_service(
                retry_logic::VectorGrpcRetryLogic,
                services,
                config
                    .routing
                    .as_ref()
                    .and_then(|routing| routing.health.clone())
                    .unwrap_or_else(default_endpoint_health_config),
                VectorGrpcHealthLogic,
                1,
            );

            crate::sinks::VectorSink::from_event_streamsink(sink::VectorSink {
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

            let service = tower::ServiceBuilder::new()
                .settings(failover_request_settings, retry_logic::VectorGrpcRetryLogic)
                .service(FailoverVectorService::new(
                    services
                        .into_iter()
                        .map(|(_endpoint, service)| service)
                        .collect(),
                    endpoint_timeout,
                    endpoint_strategy,
                ));

            crate::sinks::VectorSink::from_event_streamsink(sink::VectorSink {
                batch_settings,
                service,
            })
        }
    };

    Ok((sink, Box::pin(healthcheck)))
}

/// Strategy for routing requests across multiple Vector endpoints.
#[vector_config::configurable_component]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SinkHealthcheckOptions;
    use crate::sinks::util::TowerRequestConfig;
    use crate::sinks::util::UriSerde;
    use crate::sinks::vector::config::VectorConfig;
    use sink_error::VectorSinkError;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::time::Duration;
    use crate::sinks::util::service::HealthLogic;

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

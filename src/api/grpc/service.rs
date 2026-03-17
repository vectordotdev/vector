use std::collections::{HashMap, HashSet};
use std::pin::Pin;
// std::sync::Mutex is intentional: the lock is never held across an .await point
// (only used in synchronous map updates inside IntervalStream closures), so the
// cheaper std mutex is correct here. tokio::sync::Mutex is only needed when the
// critical section itself contains .await.
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures::{StreamExt as FuturesStreamExt, stream};
use rand::{Rng, SeedableRng, rngs::SmallRng};
use tokio::select;
use tokio::sync::mpsc;
use tokio::time::{self, interval};
use tokio_stream::{
    Stream,
    wrappers::{IntervalStream, ReceiverStream},
};
use tonic::{Request, Response, Status};
use vector_lib::tap::{
    controller::{TapController, TapPatterns, TapPayload},
    topology::WatchRx,
};

use crate::event::{Metric, MetricValue};
use crate::metrics::Controller;
use crate::proto::observability::{
    self, Component as ProtoComponent, ComponentType, EventNotification, TappedEvent, *,
};

/// Helper function to extract metric value as f64
fn get_metric_value(metric: &Metric) -> Option<f64> {
    match metric.value() {
        MetricValue::Counter { value } => Some(*value),
        MetricValue::Gauge { value } => Some(*value),
        _ => None,
    }
}

/// Helper function to filter metrics by name and group by component_id tag
fn filter_and_group_metrics(metrics: &[Metric], metric_name: &str) -> HashMap<String, f64> {
    let mut result = HashMap::new();

    for metric in metrics.iter().filter(|m| m.name() == metric_name) {
        if let Some(tags) = metric.tags()
            && let Some(component_id) = tags.get("component_id")
            && let Some(value) = get_metric_value(metric)
        {
            *result.entry(component_id.to_string()).or_insert(0.0) += value;
        }
    }

    result
}

/// Extract all component metrics and group by component_id
fn extract_component_metrics(metrics: &[Metric]) -> HashMap<String, ComponentMetrics> {
    let received_bytes = filter_and_group_metrics(metrics, "component_received_bytes_total");
    let received_events = filter_and_group_metrics(metrics, "component_received_events_total");
    let sent_bytes = filter_and_group_metrics(metrics, "component_sent_bytes_total");
    let sent_events = filter_and_group_metrics(metrics, "component_sent_events_total");

    let mut all_component_ids = HashSet::new();
    all_component_ids.extend(received_bytes.keys().cloned());
    all_component_ids.extend(received_events.keys().cloned());
    all_component_ids.extend(sent_bytes.keys().cloned());
    all_component_ids.extend(sent_events.keys().cloned());

    let mut result = HashMap::new();
    for component_id in all_component_ids {
        result.insert(
            component_id.clone(),
            ComponentMetrics {
                received_bytes_total: received_bytes.get(&component_id).map(|v| *v as i64),
                received_events_total: received_events.get(&component_id).map(|v| *v as i64),
                sent_bytes_total: sent_bytes.get(&component_id).map(|v| *v as i64),
                sent_events_total: sent_events.get(&component_id).map(|v| *v as i64),
            },
        );
    }

    result
}

/// Helper function to calculate throughput by comparing current and previous values
fn calculate_throughput(
    current: &HashMap<String, f64>,
    previous: &HashMap<String, f64>,
    interval_secs: f64,
) -> HashMap<String, f64> {
    let mut result = HashMap::new();

    for (component_id, current_value) in current {
        if let Some(previous_value) = previous.get(component_id) {
            let delta = current_value - previous_value;
            let throughput = if interval_secs > 0.0 {
                delta / interval_secs
            } else {
                0.0
            };
            result.insert(component_id.clone(), throughput.max(0.0));
        }
    }

    result
}

/// Converts a component's output port names into proto `Output` messages.
///
/// `None` port means the default output (represented as `"_default"`).
/// `sent_events_total` is left at 0; per-output event counts are not available
/// in the snapshot and are tracked via the streaming metrics endpoints.
fn ports_to_proto_outputs(ports: &[Option<&str>]) -> Vec<Output> {
    ports
        .iter()
        .map(|port| Output {
            output_id: port.unwrap_or("_default").to_string(),
            sent_events_total: 0,
        })
        .collect()
}

/// gRPC observability service implementation.
///
/// This service provides real-time monitoring and observability for Vector instances,
/// replacing the previous GraphQL API with a more efficient gRPC interface.
pub struct ObservabilityService {
    watch_rx: WatchRx,
}

impl ObservabilityService {
    pub const fn new(watch_rx: WatchRx) -> Self {
        Self { watch_rx }
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
        request: Request<ComponentsRequest>,
    ) -> Result<Response<ComponentsResponse>, Status> {
        let limit = request.into_inner().limit;

        // Get the current topology snapshot
        let tap_resource = self.watch_rx.borrow().clone();

        // Get metrics for all components
        let controller =
            Controller::get().map_err(|_| Status::internal("Metrics system not initialized"))?;
        let metrics = controller.capture_metrics();
        let component_metrics_map = extract_component_metrics(&metrics);

        let type_map = &tap_resource.type_names;

        // `tap_resource.outputs` and `tap_resource.inputs` are both full topology snapshots,
        // maintained incrementally on RunningTopology. `source_keys`/`sink_keys` are diff-only
        // (changed/added in the last reload) and must not be used for enumeration here.
        //
        // Component kind is determined by set membership:
        //   source    = has outputs, no inputs
        //   transform = has outputs AND inputs
        //   sink      = has inputs, no outputs
        let output_ports = tap_resource.output_ports_by_component();

        let mut components = Vec::new();

        // Sources: present in `output_ports` but not in `inputs`.
        // `output_ports` is keyed by component, so no duplicates.
        for (key, ports) in &output_ports {
            if tap_resource.inputs.contains_key(*key) {
                continue; // transform or sink, handled below
            }
            let key_str = key.to_string();
            let on_type = type_map
                .get(&key_str)
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());
            components.push(ProtoComponent {
                component_id: key_str.clone(),
                component_type: ComponentType::Source as i32,
                on_type,
                outputs: ports_to_proto_outputs(ports),
                metrics: component_metrics_map.get(&key_str).cloned(),
            });
        }

        // Transforms and sinks: present in `inputs` (full snapshot)
        for component_key in tap_resource.inputs.keys() {
            let key_str = component_key.to_string();
            let (component_type, outputs) = if let Some(ports) = output_ports.get(component_key) {
                (ComponentType::Transform, ports_to_proto_outputs(ports))
            } else {
                (ComponentType::Sink, vec![])
            };
            let on_type = type_map
                .get(&key_str)
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());
            components.push(ProtoComponent {
                component_id: key_str.clone(),
                component_type: component_type as i32,
                on_type,
                outputs,
                metrics: component_metrics_map.get(&key_str).cloned(),
            });
        }

        // Apply limit if specified
        if limit > 0 {
            components.truncate(limit as usize);
        }

        Ok(Response::new(ComponentsResponse { components }))
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
        let stream = tokio_stream::StreamExt::map(IntervalStream::new(interval(duration)), |_| {
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

        let controller =
            Controller::get().map_err(|_| Status::internal("Metrics system not initialized"))?;

        let duration = Duration::from_millis(interval_ms as u64);
        let stream =
            tokio_stream::StreamExt::map(IntervalStream::new(interval(duration)), move |_| {
                // Query the actual Vector uptime from the metrics system
                let metrics = controller.capture_metrics();
                let uptime_seconds = metrics
                    .iter()
                    .find(|m| m.name() == "uptime_seconds")
                    .and_then(get_metric_value)
                    .unwrap_or(0.0) as i64;

                Ok(UptimeResponse { uptime_seconds })
            });

        Ok(Response::new(Box::pin(stream)))
    }

    type StreamComponentAllocatedBytesStream =
        Pin<Box<dyn Stream<Item = Result<ComponentAllocatedBytesResponse, Status>> + Send>>;

    async fn stream_component_allocated_bytes(
        &self,
        request: Request<MetricStreamRequest>,
    ) -> Result<Response<Self::StreamComponentAllocatedBytesStream>, Status> {
        let interval_ms = request.into_inner().interval_ms;
        if interval_ms <= 0 {
            return Err(Status::invalid_argument("interval_ms must be positive"));
        }

        let controller =
            Controller::get().map_err(|_| Status::internal("Metrics system not initialized"))?;

        let duration = Duration::from_millis(interval_ms as u64);
        let stream =
            tokio_stream::StreamExt::map(IntervalStream::new(interval(duration)), move |_| {
                let metrics = controller.capture_metrics();
                let component_metrics =
                    filter_and_group_metrics(&metrics, "component_allocated_bytes");

                tokio_stream::iter(
                    component_metrics
                        .into_iter()
                        .map(|(component_id, allocated_bytes)| {
                            Ok(ComponentAllocatedBytesResponse {
                                component_id,
                                allocated_bytes: allocated_bytes as i64,
                            })
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .flatten();

        Ok(Response::new(Box::pin(stream)))
    }

    type StreamComponentReceivedEventsThroughputStream =
        Pin<Box<dyn Stream<Item = Result<ComponentThroughputResponse, Status>> + Send>>;

    async fn stream_component_received_events_throughput(
        &self,
        request: Request<MetricStreamRequest>,
    ) -> Result<Response<Self::StreamComponentReceivedEventsThroughputStream>, Status> {
        let interval_ms = request.into_inner().interval_ms;
        if interval_ms <= 0 {
            return Err(Status::invalid_argument("interval_ms must be positive"));
        }

        let controller =
            Controller::get().map_err(|_| Status::internal("Metrics system not initialized"))?;

        let duration = Duration::from_millis(interval_ms as u64);
        let interval_secs = interval_ms as f64 / 1000.0;
        let previous_values = Arc::new(Mutex::new(HashMap::new()));

        let stream =
            tokio_stream::StreamExt::map(IntervalStream::new(interval(duration)), move |_| {
                let metrics = controller.capture_metrics();
                let current_values =
                    filter_and_group_metrics(&metrics, "component_received_events_total");

                let mut prev = match previous_values.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        error!("Mutex poisoned for metric throughput, recovering.");
                        poisoned.into_inner()
                    }
                };
                let throughputs = calculate_throughput(&current_values, &prev, interval_secs);
                *prev = current_values;

                tokio_stream::iter(
                    throughputs
                        .into_iter()
                        .map(|(component_id, throughput)| {
                            Ok(ComponentThroughputResponse {
                                component_id,
                                throughput,
                            })
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .flatten();

        Ok(Response::new(Box::pin(stream)))
    }

    type StreamComponentSentEventsThroughputStream =
        Pin<Box<dyn Stream<Item = Result<ComponentThroughputResponse, Status>> + Send>>;

    async fn stream_component_sent_events_throughput(
        &self,
        request: Request<MetricStreamRequest>,
    ) -> Result<Response<Self::StreamComponentSentEventsThroughputStream>, Status> {
        let interval_ms = request.into_inner().interval_ms;
        if interval_ms <= 0 {
            return Err(Status::invalid_argument("interval_ms must be positive"));
        }

        let controller =
            Controller::get().map_err(|_| Status::internal("Metrics system not initialized"))?;

        let duration = Duration::from_millis(interval_ms as u64);
        let interval_secs = interval_ms as f64 / 1000.0;
        let previous_values = Arc::new(Mutex::new(HashMap::new()));

        let stream =
            tokio_stream::StreamExt::map(IntervalStream::new(interval(duration)), move |_| {
                let metrics = controller.capture_metrics();
                let current_values =
                    filter_and_group_metrics(&metrics, "component_sent_events_total");

                let mut prev = match previous_values.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        error!("Mutex poisoned for metric throughput, recovering.");
                        poisoned.into_inner()
                    }
                };
                let throughputs = calculate_throughput(&current_values, &prev, interval_secs);
                *prev = current_values;

                tokio_stream::iter(
                    throughputs
                        .into_iter()
                        .map(|(component_id, throughput)| {
                            Ok(ComponentThroughputResponse {
                                component_id,
                                throughput,
                            })
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .flatten();

        Ok(Response::new(Box::pin(stream)))
    }

    type StreamComponentReceivedBytesThroughputStream =
        Pin<Box<dyn Stream<Item = Result<ComponentThroughputResponse, Status>> + Send>>;

    async fn stream_component_received_bytes_throughput(
        &self,
        request: Request<MetricStreamRequest>,
    ) -> Result<Response<Self::StreamComponentReceivedBytesThroughputStream>, Status> {
        let interval_ms = request.into_inner().interval_ms;
        if interval_ms <= 0 {
            return Err(Status::invalid_argument("interval_ms must be positive"));
        }

        let controller =
            Controller::get().map_err(|_| Status::internal("Metrics system not initialized"))?;

        let duration = Duration::from_millis(interval_ms as u64);
        let interval_secs = interval_ms as f64 / 1000.0;
        let previous_values = Arc::new(Mutex::new(HashMap::new()));

        let stream =
            tokio_stream::StreamExt::map(IntervalStream::new(interval(duration)), move |_| {
                let metrics = controller.capture_metrics();
                let current_values =
                    filter_and_group_metrics(&metrics, "component_received_bytes_total");

                let mut prev = match previous_values.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        error!("Mutex poisoned for metric throughput, recovering.");
                        poisoned.into_inner()
                    }
                };
                let throughputs = calculate_throughput(&current_values, &prev, interval_secs);
                *prev = current_values;

                tokio_stream::iter(
                    throughputs
                        .into_iter()
                        .map(|(component_id, throughput)| {
                            Ok(ComponentThroughputResponse {
                                component_id,
                                throughput,
                            })
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .flatten();

        Ok(Response::new(Box::pin(stream)))
    }

    type StreamComponentSentBytesThroughputStream =
        Pin<Box<dyn Stream<Item = Result<ComponentThroughputResponse, Status>> + Send>>;

    async fn stream_component_sent_bytes_throughput(
        &self,
        request: Request<MetricStreamRequest>,
    ) -> Result<Response<Self::StreamComponentSentBytesThroughputStream>, Status> {
        let interval_ms = request.into_inner().interval_ms;
        if interval_ms <= 0 {
            return Err(Status::invalid_argument("interval_ms must be positive"));
        }

        let controller =
            Controller::get().map_err(|_| Status::internal("Metrics system not initialized"))?;

        let duration = Duration::from_millis(interval_ms as u64);
        let interval_secs = interval_ms as f64 / 1000.0;
        let previous_values = Arc::new(Mutex::new(HashMap::new()));

        let stream =
            tokio_stream::StreamExt::map(IntervalStream::new(interval(duration)), move |_| {
                let metrics = controller.capture_metrics();
                let current_values =
                    filter_and_group_metrics(&metrics, "component_sent_bytes_total");

                let mut prev = match previous_values.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        error!("Mutex poisoned for metric throughput, recovering.");
                        poisoned.into_inner()
                    }
                };
                let throughputs = calculate_throughput(&current_values, &prev, interval_secs);
                *prev = current_values;

                tokio_stream::iter(
                    throughputs
                        .into_iter()
                        .map(|(component_id, throughput)| {
                            Ok(ComponentThroughputResponse {
                                component_id,
                                throughput,
                            })
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .flatten();

        Ok(Response::new(Box::pin(stream)))
    }

    type StreamComponentReceivedEventsTotalStream =
        Pin<Box<dyn Stream<Item = Result<ComponentTotalsResponse, Status>> + Send>>;

    async fn stream_component_received_events_total(
        &self,
        request: Request<MetricStreamRequest>,
    ) -> Result<Response<Self::StreamComponentReceivedEventsTotalStream>, Status> {
        let interval_ms = request.into_inner().interval_ms;
        if interval_ms <= 0 {
            return Err(Status::invalid_argument("interval_ms must be positive"));
        }

        let controller =
            Controller::get().map_err(|_| Status::internal("Metrics system not initialized"))?;

        let duration = Duration::from_millis(interval_ms as u64);
        let stream =
            tokio_stream::StreamExt::map(IntervalStream::new(interval(duration)), move |_| {
                let metrics = controller.capture_metrics();
                let component_metrics =
                    filter_and_group_metrics(&metrics, "component_received_events_total");

                tokio_stream::iter(
                    component_metrics
                        .into_iter()
                        .map(|(component_id, total)| {
                            Ok(ComponentTotalsResponse {
                                component_id,
                                total: total as i64,
                            })
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .flatten();

        Ok(Response::new(Box::pin(stream)))
    }

    type StreamComponentSentEventsTotalStream =
        Pin<Box<dyn Stream<Item = Result<ComponentTotalsResponse, Status>> + Send>>;

    async fn stream_component_sent_events_total(
        &self,
        request: Request<MetricStreamRequest>,
    ) -> Result<Response<Self::StreamComponentSentEventsTotalStream>, Status> {
        let interval_ms = request.into_inner().interval_ms;
        if interval_ms <= 0 {
            return Err(Status::invalid_argument("interval_ms must be positive"));
        }

        let controller =
            Controller::get().map_err(|_| Status::internal("Metrics system not initialized"))?;

        let duration = Duration::from_millis(interval_ms as u64);
        let stream =
            tokio_stream::StreamExt::map(IntervalStream::new(interval(duration)), move |_| {
                let metrics = controller.capture_metrics();
                let component_metrics =
                    filter_and_group_metrics(&metrics, "component_sent_events_total");

                tokio_stream::iter(
                    component_metrics
                        .into_iter()
                        .map(|(component_id, total)| {
                            Ok(ComponentTotalsResponse {
                                component_id,
                                total: total as i64,
                            })
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .flatten();

        Ok(Response::new(Box::pin(stream)))
    }

    type StreamComponentReceivedBytesTotalStream =
        Pin<Box<dyn Stream<Item = Result<ComponentTotalsResponse, Status>> + Send>>;

    async fn stream_component_received_bytes_total(
        &self,
        request: Request<MetricStreamRequest>,
    ) -> Result<Response<Self::StreamComponentReceivedBytesTotalStream>, Status> {
        let interval_ms = request.into_inner().interval_ms;
        if interval_ms <= 0 {
            return Err(Status::invalid_argument("interval_ms must be positive"));
        }

        let controller =
            Controller::get().map_err(|_| Status::internal("Metrics system not initialized"))?;

        let duration = Duration::from_millis(interval_ms as u64);
        let stream =
            tokio_stream::StreamExt::map(IntervalStream::new(interval(duration)), move |_| {
                let metrics = controller.capture_metrics();
                let component_metrics =
                    filter_and_group_metrics(&metrics, "component_received_bytes_total");

                tokio_stream::iter(
                    component_metrics
                        .into_iter()
                        .map(|(component_id, total)| {
                            Ok(ComponentTotalsResponse {
                                component_id,
                                total: total as i64,
                            })
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .flatten();

        Ok(Response::new(Box::pin(stream)))
    }

    type StreamComponentSentBytesTotalStream =
        Pin<Box<dyn Stream<Item = Result<ComponentTotalsResponse, Status>> + Send>>;

    async fn stream_component_sent_bytes_total(
        &self,
        request: Request<MetricStreamRequest>,
    ) -> Result<Response<Self::StreamComponentSentBytesTotalStream>, Status> {
        let interval_ms = request.into_inner().interval_ms;
        if interval_ms <= 0 {
            return Err(Status::invalid_argument("interval_ms must be positive"));
        }

        let controller =
            Controller::get().map_err(|_| Status::internal("Metrics system not initialized"))?;

        let duration = Duration::from_millis(interval_ms as u64);
        let stream =
            tokio_stream::StreamExt::map(IntervalStream::new(interval(duration)), move |_| {
                let metrics = controller.capture_metrics();
                let component_metrics =
                    filter_and_group_metrics(&metrics, "component_sent_bytes_total");

                tokio_stream::iter(
                    component_metrics
                        .into_iter()
                        .map(|(component_id, total)| {
                            Ok(ComponentTotalsResponse {
                                component_id,
                                total: total as i64,
                            })
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .flatten();

        Ok(Response::new(Box::pin(stream)))
    }

    type StreamComponentErrorsTotalStream =
        Pin<Box<dyn Stream<Item = Result<ComponentTotalsResponse, Status>> + Send>>;

    async fn stream_component_errors_total(
        &self,
        request: Request<MetricStreamRequest>,
    ) -> Result<Response<Self::StreamComponentErrorsTotalStream>, Status> {
        let interval_ms = request.into_inner().interval_ms;
        if interval_ms <= 0 {
            return Err(Status::invalid_argument("interval_ms must be positive"));
        }

        let controller =
            Controller::get().map_err(|_| Status::internal("Metrics system not initialized"))?;

        let duration = Duration::from_millis(interval_ms as u64);
        let stream =
            tokio_stream::StreamExt::map(IntervalStream::new(interval(duration)), move |_| {
                let metrics = controller.capture_metrics();
                let component_metrics =
                    filter_and_group_metrics(&metrics, "component_errors_total");

                tokio_stream::iter(
                    component_metrics
                        .into_iter()
                        .map(|(component_id, total)| {
                            Ok(ComponentTotalsResponse {
                                component_id,
                                total: total as i64,
                            })
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .flatten();

        Ok(Response::new(Box::pin(stream)))
    }

    // ========== Event Tapping ==========

    type StreamOutputEventsStream = Pin<Box<dyn Stream<Item = Result<OutputEvent, Status>> + Send>>;

    async fn stream_output_events(
        &self,
        request: Request<OutputEventsRequest>,
    ) -> Result<Response<Self::StreamOutputEventsStream>, Status> {
        let req = request.into_inner();

        // Validate before casting to prevent negative values from becoming large positive values
        if req.interval_ms <= 0 {
            return Err(Status::invalid_argument("interval_ms must be positive"));
        }

        if req.limit <= 0 {
            return Err(Status::invalid_argument(
                "limit must be >= 1 (controls reservoir size and channel capacity)",
            ));
        }

        let interval_ms = req.interval_ms as u64;
        let limit = req.limit as usize;

        let patterns = TapPatterns {
            for_outputs: req.outputs_patterns.into_iter().collect(),
            for_inputs: req.inputs_patterns.into_iter().collect(),
        };

        // Channel for receiving tap payloads
        let (tap_tx, tap_rx) = mpsc::channel(limit);

        // Channel for sending events to the client
        let (event_tx, event_rx) = mpsc::channel::<Vec<OutputEvent>>(10);

        let watch_rx = self.watch_rx.clone();

        tokio::spawn(async move {
            let _tap_controller = TapController::new(watch_rx, tap_tx, patterns);
            let mut tap_rx = ReceiverStream::new(tap_rx);
            let mut interval = time::interval(time::Duration::from_millis(interval_ms));
            let mut reservoir = Reservoir::new(limit);

            loop {
                select! {
                    Some(tap_payload) = tokio_stream::StreamExt::next(&mut tap_rx) => {
                        if reservoir.handle_payload(tap_payload, &event_tx).await.is_err() {
                            break;
                        }
                    }
                    _ = interval.tick() => {
                        if reservoir.flush(&event_tx).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });

        let stream = FuturesStreamExt::flat_map(ReceiverStream::new(event_rx), |events| {
            stream::iter(events.into_iter().map(Ok))
        });

        Ok(Response::new(Box::pin(stream)))
    }
}

/// Reservoir sampler for tap events, batched and flushed on an interval.
struct Reservoir {
    events: Vec<(usize, OutputEvent)>,
    rng: SmallRng,
    batch: usize,
    limit: usize,
}

impl Reservoir {
    fn new(limit: usize) -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| {
                warn!("System clock is before Unix epoch, using fallback seed.");
                Duration::from_secs(0)
            })
            .as_nanos() as u64;

        Self {
            events: Vec::with_capacity(limit),
            rng: SmallRng::seed_from_u64(seed),
            batch: 0,
            limit,
        }
    }

    /// Process a tap payload: notifications are forwarded immediately; data events
    /// are reservoir-sampled (Algorithm R) for the next flush.
    async fn handle_payload(
        &mut self,
        payload: TapPayload,
        tx: &mpsc::Sender<Vec<OutputEvent>>,
    ) -> Result<(), ()> {
        for event in tap_payload_to_output_events(payload) {
            if matches!(event.event, Some(output_event::Event::Notification(_))) {
                tx.send(vec![event]).await.map_err(|err| {
                    debug!(message = "Couldn't send notification.", error = ?err);
                })?;
            } else {
                // Reservoir sampling (Algorithm R).
                // Draw from 0..=batch (inclusive) so that event i has
                // exactly limit/(i+1) probability of entering the reservoir.
                // Using 0..batch (exclusive) would guarantee replacement on
                // the first post-fill event (100% instead of limit/(limit+1)).
                if self.limit > self.events.len() {
                    self.events.push((self.batch, event));
                } else {
                    let idx = self.rng.random_range(0..=self.batch);
                    if idx < self.events.len() {
                        self.events[idx] = (self.batch, event);
                    }
                }
                self.batch += 1;
            }
        }
        Ok(())
    }

    /// Flush sampled events to the client, sorted by arrival order.
    async fn flush(&mut self, tx: &mpsc::Sender<Vec<OutputEvent>>) -> Result<(), ()> {
        if self.events.is_empty() {
            return Ok(());
        }
        self.batch = 0;
        self.events.sort_by_key(|(batch, _)| *batch);
        let events = self.events.drain(..).map(|(_, e)| e).collect();
        tx.send(events).await.map_err(|err| {
            debug!(message = "Couldn't send events.", error = ?err);
        })
    }
}

/// Convert TapPayload to gRPC OutputEvent(s)
fn tap_payload_to_output_events(payload: TapPayload) -> Vec<OutputEvent> {
    use crate::event::proto::{Event, EventWrapper};

    match payload {
        TapPayload::Log(output, log_array) => log_array
            .into_iter()
            .map(|log_event| {
                // Convert Vector's internal LogEvent to proto Log (metadata is preserved in Log.metadata_full)
                let proto_log: crate::event::proto::Log = log_event.into();
                let event_wrapper = Some(EventWrapper {
                    event: Some(Event::Log(proto_log)),
                });

                OutputEvent {
                    event: Some(output_event::Event::TappedEvent(TappedEvent {
                        component_id: output.output_id.component.id().to_string(),
                        component_type: output.component_type.to_string(),
                        component_kind: output.component_kind.to_string(),
                        event: event_wrapper,
                    })),
                }
            })
            .collect(),
        TapPayload::Metric(output, metric_array) => metric_array
            .into_iter()
            .map(|metric_event| {
                // Convert Vector's internal Metric to proto Metric
                let proto_metric: crate::event::proto::Metric = metric_event.into();
                let event_wrapper = Some(EventWrapper {
                    event: Some(Event::Metric(proto_metric)),
                });

                OutputEvent {
                    event: Some(output_event::Event::TappedEvent(TappedEvent {
                        component_id: output.output_id.component.id().to_string(),
                        component_type: output.component_type.to_string(),
                        component_kind: output.component_kind.to_string(),
                        event: event_wrapper,
                    })),
                }
            })
            .collect(),
        TapPayload::Trace(output, trace_array) => trace_array
            .into_iter()
            .map(|trace_event| {
                // Convert Vector's internal TraceEvent to proto Trace
                let proto_trace: crate::event::proto::Trace = trace_event.into();
                let event_wrapper = Some(EventWrapper {
                    event: Some(Event::Trace(proto_trace)),
                });

                OutputEvent {
                    event: Some(output_event::Event::TappedEvent(TappedEvent {
                        component_id: output.output_id.component.id().to_string(),
                        component_type: output.component_type.to_string(),
                        component_kind: output.component_kind.to_string(),
                        event: event_wrapper,
                    })),
                }
            })
            .collect(),
        TapPayload::Notification(notification) => {
            vec![create_notification_event(notification.as_str())]
        }
    }
}

fn create_notification_event(message: &str) -> OutputEvent {
    OutputEvent {
        event: Some(output_event::Event::Notification(EventNotification {
            message: message.to_string(),
        })),
    }
}

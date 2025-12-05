use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures::{StreamExt as FuturesStreamExt, stream};
use prost_types::Timestamp;
use rand::{Rng, SeedableRng, rngs::SmallRng};
use tokio::select;
use tokio::sync::mpsc;
use tokio::time::{self, interval};
use tokio_stream::{
    Stream,
    wrappers::{IntervalStream, ReceiverStream},
};
use tonic::{Request, Response, Status};
use vector_lib::{
    encode_logfmt,
    event::{LogEvent as VectorLogEvent, Metric as VectorMetric, TraceEvent as VectorTraceEvent},
    tap::{
        controller::{TapController, TapPatterns, TapPayload},
        topology::WatchRx,
    },
};
use vrl::event_path;

use crate::event::{Metric, MetricValue};
use crate::metrics::Controller;
use crate::proto::observability::{
    self, Component as ProtoComponent, ComponentType, EventNotification, LogEvent, MetricEvent,
    TraceEvent, *,
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
            result.insert(component_id.to_string(), value);
        }
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

        // Collect all component keys and their types
        let mut components = Vec::new();
        let mut seen_keys = HashSet::new();

        // Add sources
        for source_key in &tap_resource.source_keys {
            if seen_keys.insert(source_key.clone()) {
                components.push(ProtoComponent {
                    component_id: source_key.clone(),
                    component_type: ComponentType::Source as i32,
                    on_type: "source".to_string(), // Generic type, actual type not available from TapResource
                    outputs: vec![],
                    metrics: None,
                });
            }
        }

        // Add transforms and sinks (components with inputs)
        for component_key in tap_resource.inputs.keys() {
            let key_str = component_key.to_string();
            if seen_keys.insert(key_str.clone()) {
                // Check if this component also has outputs (transform) or not (sink)
                let has_outputs = tap_resource
                    .outputs
                    .keys()
                    .any(|tap_output| tap_output.output_id.component == *component_key);

                let component_type = if has_outputs {
                    ComponentType::Transform
                } else {
                    ComponentType::Sink
                };

                components.push(ProtoComponent {
                    component_id: key_str,
                    component_type: component_type as i32,
                    on_type: if has_outputs { "transform" } else { "sink" }.to_string(),
                    outputs: vec![],
                    metrics: None,
                });
            }
        }

        // Also explicitly add sinks from sink_keys if they weren't in inputs
        for sink_key in &tap_resource.sink_keys {
            if seen_keys.insert(sink_key.clone()) {
                components.push(ProtoComponent {
                    component_id: sink_key.clone(),
                    component_type: ComponentType::Sink as i32,
                    on_type: "sink".to_string(),
                    outputs: vec![],
                    metrics: None,
                });
            }
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

        let start_time = std::time::Instant::now();
        let duration = Duration::from_millis(interval_ms as u64);
        let stream =
            tokio_stream::StreamExt::map(IntervalStream::new(interval(duration)), move |_| {
                let uptime_seconds = start_time.elapsed().as_secs() as i64;
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
        let interval_ms = req.interval_ms as u64;
        let limit = req.limit as usize;
        let encoding = req.encoding();

        if interval_ms == 0 {
            return Err(Status::invalid_argument("interval_ms must be positive"));
        }

        if limit == 0 {
            return Err(Status::invalid_argument("limit must be positive"));
        }

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

            // Tick interval for batching
            let mut interval = time::interval(time::Duration::from_millis(interval_ms));

            // Structure to hold sortable events
            struct SortableEvent {
                batch: usize,
                event: OutputEvent,
            }

            let mut results = Vec::<SortableEvent>::with_capacity(limit);

            // Random number generator for reservoir sampling
            let seed = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_else(|_| {
                    warn!("System clock is before Unix epoch, using fallback seed.");
                    Duration::from_secs(0)
                })
                .as_nanos() as u64;
            let mut rng = SmallRng::seed_from_u64(seed);
            let mut batch = 0;

            loop {
                select! {
                    Some(tap_payload) = tokio_stream::StreamExt::next(&mut tap_rx) => {
                        // Convert TapPayload to OutputEvent(s)
                        let events = tap_payload_to_output_events(tap_payload, encoding);

                        for event in events {
                            // Handle notifications immediately
                            if matches!(event.event, Some(output_event::Event::Notification(_))) {
                                if let Err(err) = event_tx.send(vec![event]).await {
                                    debug!(message = "Couldn't send notification.", error = ?err);
                                    break;
                                }
                            } else {
                                // Reservoir sampling (Algorithm R)
                                let sortable_event = SortableEvent { batch, event };

                                if limit > results.len() {
                                    results.push(sortable_event);
                                } else {
                                    let random_number = rng.random_range(0..batch);
                                    if random_number < results.len() {
                                        results[random_number] = sortable_event;
                                    }
                                }
                                batch += 1;
                            }
                        }
                    }
                    _ = interval.tick() => {
                        if !results.is_empty() {
                            batch = 0;

                            // Sort by batch order and drain
                            results.sort_by_key(|r| r.batch);
                            let events = results.drain(..).map(|r| r.event).collect();

                            if let Err(err) = event_tx.send(events).await {
                                debug!(message = "Couldn't send events.", error = ?err);
                                break;
                            }
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

/// Convert TapPayload to gRPC OutputEvent(s)
fn tap_payload_to_output_events(payload: TapPayload, encoding: EventEncoding) -> Vec<OutputEvent> {
    match payload {
        TapPayload::Log(output, log_array) => log_array
            .into_iter()
            .map(|log_event| {
                let message = log_event
                    .get(event_path!("message"))
                    .map(|v| v.to_string_lossy().into_owned())
                    .unwrap_or_default();

                let timestamp = log_event
                    .get(event_path!("timestamp"))
                    .and_then(|v| v.as_timestamp())
                    .map(|dt| Timestamp {
                        seconds: dt.timestamp(),
                        nanos: dt.timestamp_subsec_nanos() as i32,
                    });

                let encoded_string = encode_log_event(&log_event, encoding);

                OutputEvent {
                    event: Some(output_event::Event::Log(LogEvent {
                        component_id: output.output_id.component.id().to_string(),
                        component_type: output.component_type.to_string(),
                        component_kind: output.component_kind.to_string(),
                        message,
                        timestamp,
                        encoded_string,
                    })),
                }
            })
            .collect(),
        TapPayload::Metric(output, metric_array) => metric_array
            .into_iter()
            .map(|metric_event| {
                let timestamp = metric_event.timestamp().map(|dt| Timestamp {
                    seconds: dt.timestamp(),
                    nanos: dt.timestamp_subsec_nanos() as i32,
                });

                let encoded_string = encode_metric_event(&metric_event, encoding);

                OutputEvent {
                    event: Some(output_event::Event::Metric(MetricEvent {
                        component_id: output.output_id.component.id().to_string(),
                        component_type: output.component_type.to_string(),
                        component_kind: output.component_kind.to_string(),
                        timestamp,
                        encoded_string,
                    })),
                }
            })
            .collect(),
        TapPayload::Trace(output, trace_array) => trace_array
            .into_iter()
            .map(|trace_event| {
                let encoded_string = encode_trace_event(&trace_event, encoding);

                OutputEvent {
                    event: Some(output_event::Event::Trace(TraceEvent {
                        component_id: output.output_id.component.id().to_string(),
                        component_type: output.component_type.to_string(),
                        component_kind: output.component_kind.to_string(),
                        encoded_string,
                    })),
                }
            })
            .collect(),
        TapPayload::Notification(notification) => {
            vec![create_notification_event(notification.as_str())]
        }
    }
}

fn encode_log_event(log_event: &VectorLogEvent, encoding: EventEncoding) -> String {
    match encoding {
        EventEncoding::Json => serde_json::to_string(log_event)
            .unwrap_or_else(|_| "JSON serialization failed".to_string()),
        EventEncoding::Yaml => serde_yaml::to_string(log_event)
            .unwrap_or_else(|_| "YAML serialization failed".to_string()),
        EventEncoding::Logfmt => encode_logfmt::encode_value(log_event.value())
            .unwrap_or_else(|_| "logfmt serialization failed".to_string()),
    }
}

fn encode_metric_event(metric_event: &VectorMetric, encoding: EventEncoding) -> String {
    match encoding {
        EventEncoding::Json => serde_json::to_string(metric_event)
            .unwrap_or_else(|_| "JSON serialization failed".to_string()),
        EventEncoding::Yaml => serde_yaml::to_string(metric_event)
            .unwrap_or_else(|_| "YAML serialization failed".to_string()),
        EventEncoding::Logfmt => {
            // Metrics don't have logfmt encoding, fall back to JSON
            serde_json::to_string(metric_event)
                .unwrap_or_else(|_| "JSON serialization failed".to_string())
        }
    }
}

fn encode_trace_event(trace_event: &VectorTraceEvent, encoding: EventEncoding) -> String {
    match encoding {
        EventEncoding::Json => serde_json::to_string(trace_event)
            .unwrap_or_else(|_| "JSON serialization failed".to_string()),
        EventEncoding::Yaml => serde_yaml::to_string(trace_event)
            .unwrap_or_else(|_| "YAML serialization failed".to_string()),
        EventEncoding::Logfmt => {
            // Traces don't have logfmt encoding, fall back to JSON
            serde_json::to_string(trace_event)
                .unwrap_or_else(|_| "JSON serialization failed".to_string())
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

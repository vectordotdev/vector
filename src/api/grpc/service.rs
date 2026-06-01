use std::collections::{HashMap, HashSet};
use std::pin::Pin;
// std::sync::Mutex is intentional: the lock is never held across an .await point
// (only used in synchronous map updates inside IntervalStream closures), so the
// cheaper std mutex is correct here. tokio::sync::Mutex is only needed when the
// critical section itself contains .await.
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::{StreamExt as FuturesStreamExt, stream};
use rand::{Rng, SeedableRng as _, rngs::SmallRng};
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

type BoxStream<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>;

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

/// Filter metrics by name and group by (component_id, output) tag pair.
/// Used to populate per-output metrics in GetComponents responses.
fn filter_and_group_metrics_by_output(
    metrics: &[Metric],
    metric_name: &str,
) -> HashMap<(String, String), f64> {
    let mut result = HashMap::new();

    for metric in metrics.iter().filter(|m| m.name() == metric_name) {
        if let Some(tags) = metric.tags()
            && let Some(component_id) = tags.get("component_id")
            && let Some(value) = get_metric_value(metric)
        {
            let output = tags.get("output").unwrap_or("_default").to_string();
            *result
                .entry((component_id.to_string(), output))
                .or_insert(0.0) += value;
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

/// Helper function to calculate throughput for per-output metrics by comparing
/// current and previous values keyed by `(component_id, output)`.
fn calculate_throughput_by_output(
    current: &HashMap<(String, String), f64>,
    previous: &HashMap<(String, String), f64>,
    interval_secs: f64,
) -> HashMap<(String, String), f64> {
    let mut result = HashMap::new();

    for (key, current_value) in current {
        if let Some(previous_value) = previous.get(key) {
            let delta = current_value - previous_value;
            let throughput = if interval_secs > 0.0 {
                delta / interval_secs
            } else {
                0.0
            };
            result.insert(key.clone(), throughput.max(0.0));
        }
    }

    result
}

/// Minimum allowed polling interval for metric streams (100 ms).
///
/// Prevents clients from setting `interval_ms=1` and hammering `capture_metrics()`
/// at 1 kHz, which would cause high CPU load across concurrent subscriptions.
const MIN_INTERVAL_MS: i32 = 100;

/// Validates `interval_ms` from a streaming request, returning the value as `u64`
/// or a gRPC error if it is out of range.
fn validate_interval_ms(interval_ms: i32) -> Result<u64, Status> {
    if interval_ms < MIN_INTERVAL_MS {
        return Err(Status::invalid_argument(format!(
            "interval_ms must be >= {MIN_INTERVAL_MS}"
        )));
    }
    Ok(interval_ms as u64)
}

fn get_controller() -> Result<&'static Controller, Status> {
    Controller::get().map_err(|_| Status::internal("Metrics system not initialized"))
}

/// Builds a stream that emits per-component totals for `metric_name` every `duration`.
fn metric_totals_stream(
    duration: Duration,
    metric_name: &'static str,
) -> Result<BoxStream<StreamComponentMetricsResponse>, Status> {
    let controller = get_controller()?;
    Ok(Box::pin(
        tokio_stream::StreamExt::map(IntervalStream::new(interval(duration)), move |_| {
            let metrics = controller.capture_metrics();
            let component_metrics = filter_and_group_metrics(&metrics, metric_name);
            tokio_stream::iter(
                component_metrics
                    .into_iter()
                    .map(|(component_id, total)| {
                        Ok(StreamComponentMetricsResponse {
                            component_id,
                            value: Some(stream_component_metrics_response::Value::Total(
                                TotalMetric {
                                    value: total as i64,
                                    output_totals: Default::default(),
                                },
                            )),
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .flatten(),
    ))
}

/// Builds a stream that emits per-component throughputs for `metric_name` every `duration`.
fn metric_throughput_stream(
    duration: Duration,
    metric_name: &'static str,
) -> Result<BoxStream<StreamComponentMetricsResponse>, Status> {
    let controller = get_controller()?;
    let interval_secs = duration.as_secs_f64();
    let previous_values = Arc::new(Mutex::new(HashMap::new()));
    Ok(Box::pin(
        tokio_stream::StreamExt::map(IntervalStream::new(interval(duration)), move |_| {
            let metrics = controller.capture_metrics();
            let current_values = filter_and_group_metrics(&metrics, metric_name);
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
                        Ok(StreamComponentMetricsResponse {
                            component_id,
                            value: Some(stream_component_metrics_response::Value::Throughput(
                                ThroughputMetric {
                                    value: throughput,
                                    output_throughputs: Default::default(),
                                },
                            )),
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .flatten(),
    ))
}

/// Builds a stream that emits per-component sent_events totals with per-output breakdown.
fn sent_events_totals_stream(
    duration: Duration,
) -> Result<BoxStream<StreamComponentMetricsResponse>, Status> {
    let controller = get_controller()?;
    Ok(Box::pin(
        tokio_stream::StreamExt::map(IntervalStream::new(interval(duration)), move |_| {
            let metrics = controller.capture_metrics();
            let component_totals =
                filter_and_group_metrics(&metrics, "component_sent_events_total");
            let by_output =
                filter_and_group_metrics_by_output(&metrics, "component_sent_events_total");

            // Group per-output values by component_id
            let mut output_by_component: HashMap<String, HashMap<String, i64>> = HashMap::new();
            for ((component_id, output), value) in by_output {
                output_by_component
                    .entry(component_id)
                    .or_default()
                    .insert(output, value as i64);
            }

            tokio_stream::iter(
                component_totals
                    .into_iter()
                    .map(|(component_id, total)| {
                        let output_totals = output_by_component
                            .remove(&component_id)
                            .unwrap_or_default()
                            .into_iter()
                            .collect();
                        Ok(StreamComponentMetricsResponse {
                            component_id,
                            value: Some(stream_component_metrics_response::Value::Total(
                                TotalMetric {
                                    value: total as i64,
                                    output_totals,
                                },
                            )),
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .flatten(),
    ))
}

/// Builds a stream that emits per-component sent_events throughputs with per-output breakdown.
fn sent_events_throughput_stream(
    duration: Duration,
) -> Result<BoxStream<StreamComponentMetricsResponse>, Status> {
    let controller = get_controller()?;
    let interval_secs = duration.as_secs_f64();
    let previous_totals = Arc::new(Mutex::new(HashMap::<String, f64>::new()));
    let previous_outputs = Arc::new(Mutex::new(HashMap::<(String, String), f64>::new()));
    Ok(Box::pin(
        tokio_stream::StreamExt::map(IntervalStream::new(interval(duration)), move |_| {
            let metrics = controller.capture_metrics();
            let current_totals = filter_and_group_metrics(&metrics, "component_sent_events_total");
            let current_outputs =
                filter_and_group_metrics_by_output(&metrics, "component_sent_events_total");

            let mut prev_t = match previous_totals.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    error!("Mutex poisoned for sent_events throughput totals, recovering.");
                    poisoned.into_inner()
                }
            };
            let mut prev_o = match previous_outputs.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    error!("Mutex poisoned for sent_events throughput outputs, recovering.");
                    poisoned.into_inner()
                }
            };

            let throughputs = calculate_throughput(&current_totals, &prev_t, interval_secs);
            let output_throughputs_flat =
                calculate_throughput_by_output(&current_outputs, &prev_o, interval_secs);

            *prev_t = current_totals;
            *prev_o = current_outputs;

            // Group per-output throughputs by component_id
            let mut output_by_component: HashMap<String, HashMap<String, f64>> = HashMap::new();
            for ((component_id, output), tp) in output_throughputs_flat {
                output_by_component
                    .entry(component_id)
                    .or_default()
                    .insert(output, tp);
            }

            tokio_stream::iter(
                throughputs
                    .into_iter()
                    .map(|(component_id, throughput)| {
                        let output_throughputs = output_by_component
                            .remove(&component_id)
                            .unwrap_or_default()
                            .into_iter()
                            .collect();
                        Ok(StreamComponentMetricsResponse {
                            component_id,
                            value: Some(stream_component_metrics_response::Value::Throughput(
                                ThroughputMetric {
                                    value: throughput,
                                    output_throughputs,
                                },
                            )),
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .flatten(),
    ))
}

/// Converts a component's output port names into proto `Output` messages,
/// populating `sent_events_total` from the per-output metric snapshot.
///
/// `None` port means the default output (represented as `"_default"`).
fn ports_to_proto_outputs(
    ports: &[Option<&str>],
    component_id: &str,
    sent_events_by_output: &HashMap<(String, String), f64>,
) -> Vec<Output> {
    ports
        .iter()
        .map(|port| {
            let output_id = port.unwrap_or("_default").to_string();
            let sent_events_total = sent_events_by_output
                .get(&(component_id.to_string(), output_id.clone()))
                .copied()
                .unwrap_or(0.0) as i64;
            Output {
                output_id,
                sent_events_total,
            }
        })
        .collect()
}

/// gRPC observability service implementation.
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

    async fn get_meta(
        &self,
        _request: Request<GetMetaRequest>,
    ) -> Result<Response<GetMetaResponse>, Status> {
        let version = crate::get_version().to_string();
        let hostname = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "unknown".to_string());

        Ok(Response::new(GetMetaResponse { version, hostname }))
    }

    async fn get_allocation_tracing_status(
        &self,
        _request: Request<GetAllocationTracingStatusRequest>,
    ) -> Result<Response<GetAllocationTracingStatusResponse>, Status> {
        #[cfg(feature = "allocation-tracing")]
        let enabled = crate::internal_telemetry::allocations::is_allocation_tracing_enabled();
        #[cfg(not(feature = "allocation-tracing"))]
        let enabled = false;
        Ok(Response::new(GetAllocationTracingStatusResponse {
            enabled,
        }))
    }

    async fn get_components(
        &self,
        request: Request<GetComponentsRequest>,
    ) -> Result<Response<GetComponentsResponse>, Status> {
        let limit = request.into_inner().limit;

        // Get the current topology snapshot
        let tap_resource = self.watch_rx.borrow().clone();

        // Get metrics for all components
        let controller =
            Controller::get().map_err(|_| Status::internal("Metrics system not initialized"))?;
        let metrics = controller.capture_metrics();
        let component_metrics_map = extract_component_metrics(&metrics);
        let sent_events_by_output =
            filter_and_group_metrics_by_output(&metrics, "component_sent_events_total");

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
                outputs: ports_to_proto_outputs(ports, &key_str, &sent_events_by_output),
                metrics: component_metrics_map.get(&key_str).cloned(),
            });
        }

        // Transforms and sinks: present in `inputs` (full snapshot)
        for component_key in tap_resource.inputs.keys() {
            let key_str = component_key.to_string();
            let (component_type, outputs) = if let Some(ports) = output_ports.get(component_key) {
                (
                    ComponentType::Transform,
                    ports_to_proto_outputs(ports, &key_str, &sent_events_by_output),
                )
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

        // Sort alphabetically for stable, deterministic ordering before applying limit
        components.sort_unstable_by(|a, b| a.component_id.cmp(&b.component_id));
        if limit > 0 {
            components.truncate(limit as usize);
        }

        Ok(Response::new(GetComponentsResponse { components }))
    }

    // ========== Streaming Metrics ==========

    type StreamHeartbeatStream = BoxStream<StreamHeartbeatResponse>;

    async fn stream_heartbeat(
        &self,
        request: Request<StreamHeartbeatRequest>,
    ) -> Result<Response<Self::StreamHeartbeatStream>, Status> {
        let duration =
            Duration::from_millis(validate_interval_ms(request.into_inner().interval_ms)?);
        let stream = tokio_stream::StreamExt::map(IntervalStream::new(interval(duration)), |_| {
            let utc = Some(prost_types::Timestamp {
                seconds: chrono::Utc::now().timestamp(),
                nanos: 0,
            });
            Ok(StreamHeartbeatResponse { utc })
        });

        Ok(Response::new(Box::pin(stream)))
    }

    type StreamUptimeStream = BoxStream<StreamUptimeResponse>;

    async fn stream_uptime(
        &self,
        request: Request<StreamUptimeRequest>,
    ) -> Result<Response<Self::StreamUptimeStream>, Status> {
        let duration =
            Duration::from_millis(validate_interval_ms(request.into_inner().interval_ms)?);
        let controller = get_controller()?;
        let stream =
            tokio_stream::StreamExt::map(IntervalStream::new(interval(duration)), move |_| {
                let metrics = controller.capture_metrics();
                let uptime_seconds = metrics
                    .iter()
                    .find(|m| m.name() == "uptime_seconds")
                    .and_then(get_metric_value)
                    .unwrap_or(0.0) as i64;

                Ok(StreamUptimeResponse { uptime_seconds })
            });

        Ok(Response::new(Box::pin(stream)))
    }

    type StreamComponentAllocatedBytesStream = BoxStream<StreamComponentAllocatedBytesResponse>;

    async fn stream_component_allocated_bytes(
        &self,
        request: Request<StreamComponentAllocatedBytesRequest>,
    ) -> Result<Response<Self::StreamComponentAllocatedBytesStream>, Status> {
        let duration =
            Duration::from_millis(validate_interval_ms(request.into_inner().interval_ms)?);
        let controller = get_controller()?;
        let stream =
            tokio_stream::StreamExt::map(IntervalStream::new(interval(duration)), move |_| {
                let metrics = controller.capture_metrics();
                let component_metrics =
                    filter_and_group_metrics(&metrics, "component_allocated_bytes");

                tokio_stream::iter(
                    component_metrics
                        .into_iter()
                        .map(|(component_id, allocated_bytes)| {
                            Ok(StreamComponentAllocatedBytesResponse {
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

    type StreamComponentMetricsStream = BoxStream<StreamComponentMetricsResponse>;

    async fn stream_component_metrics(
        &self,
        request: Request<StreamComponentMetricsRequest>,
    ) -> Result<Response<Self::StreamComponentMetricsStream>, Status> {
        let req = request.into_inner();
        let duration = Duration::from_millis(validate_interval_ms(req.interval_ms)?);
        let metric = MetricName::try_from(req.metric)
            .map_err(|_| Status::invalid_argument("Unknown metric value"))?;

        let stream: BoxStream<StreamComponentMetricsResponse> = match metric {
            MetricName::Unspecified => {
                return Err(Status::invalid_argument("metric must be specified"));
            }
            MetricName::ReceivedEventsTotal => {
                metric_totals_stream(duration, "component_received_events_total")?
            }
            MetricName::SentEventsTotal => sent_events_totals_stream(duration)?,
            MetricName::ReceivedBytesTotal => {
                metric_totals_stream(duration, "component_received_bytes_total")?
            }
            MetricName::SentBytesTotal => {
                metric_totals_stream(duration, "component_sent_bytes_total")?
            }
            MetricName::ErrorsTotal => metric_totals_stream(duration, "component_errors_total")?,
            MetricName::ReceivedEventsThroughput => {
                metric_throughput_stream(duration, "component_received_events_total")?
            }
            MetricName::SentEventsThroughput => sent_events_throughput_stream(duration)?,
            MetricName::ReceivedBytesThroughput => {
                metric_throughput_stream(duration, "component_received_bytes_total")?
            }
            MetricName::SentBytesThroughput => {
                metric_throughput_stream(duration, "component_sent_bytes_total")?
            }
        };

        Ok(Response::new(stream))
    }

    // ========== Event Tapping ==========

    type StreamOutputEventsStream = BoxStream<StreamOutputEventsResponse>;

    async fn stream_output_events(
        &self,
        request: Request<StreamOutputEventsRequest>,
    ) -> Result<Response<Self::StreamOutputEventsStream>, Status> {
        let req = request.into_inner();

        // Validate before casting to prevent negative values from becoming large positive values
        if req.limit <= 0 {
            return Err(Status::invalid_argument(
                "limit must be >= 1 (controls reservoir size and channel capacity)",
            ));
        }

        const MAX_TAP_LIMIT: i32 = 100_000;
        if req.limit > MAX_TAP_LIMIT {
            return Err(Status::invalid_argument(format!(
                "limit must be <= {MAX_TAP_LIMIT}"
            )));
        }

        let interval_ms = validate_interval_ms(req.interval_ms)?;
        let limit = req.limit as usize;

        let patterns = TapPatterns {
            for_outputs: req.outputs_patterns.into_iter().collect(),
            for_inputs: req.inputs_patterns.into_iter().collect(),
        };

        // Channel for receiving tap payloads
        let (tap_tx, tap_rx) = mpsc::channel(limit);

        // Channel for sending events to the client
        let (event_tx, event_rx) = mpsc::channel::<Vec<StreamOutputEventsResponse>>(10);

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
                        if event_tx.is_closed() || reservoir.flush(&event_tx).await.is_err() {
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
    events: Vec<(usize, StreamOutputEventsResponse)>,
    rng: SmallRng,
    batch: usize,
    limit: usize,
}

impl Reservoir {
    fn new(limit: usize) -> Self {
        Self {
            events: Vec::with_capacity(limit),
            rng: SmallRng::from_rng(&mut rand::rng()),
            batch: 0,
            limit,
        }
    }

    /// Process a tap payload: notifications are forwarded immediately; data events
    /// are reservoir-sampled (Algorithm R) for the next flush.
    async fn handle_payload(
        &mut self,
        payload: TapPayload,
        tx: &mpsc::Sender<Vec<StreamOutputEventsResponse>>,
    ) -> Result<(), ()> {
        for event in tap_payload_to_output_events(payload) {
            if matches!(
                event.event,
                Some(stream_output_events_response::Event::Notification(_))
            ) {
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
    async fn flush(
        &mut self,
        tx: &mpsc::Sender<Vec<StreamOutputEventsResponse>>,
    ) -> Result<(), ()> {
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

/// Convert TapPayload to gRPC StreamOutputEventsResponse(s)
fn tap_payload_to_output_events(payload: TapPayload) -> Vec<StreamOutputEventsResponse> {
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

                StreamOutputEventsResponse {
                    event: Some(stream_output_events_response::Event::TappedEvent(
                        TappedEvent {
                            component_id: output.output_id.component.id().to_string(),
                            component_type: output.component_type.to_string(),
                            component_kind: output.component_kind.to_string(),
                            event: event_wrapper,
                        },
                    )),
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

                StreamOutputEventsResponse {
                    event: Some(stream_output_events_response::Event::TappedEvent(
                        TappedEvent {
                            component_id: output.output_id.component.id().to_string(),
                            component_type: output.component_type.to_string(),
                            component_kind: output.component_kind.to_string(),
                            event: event_wrapper,
                        },
                    )),
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

                StreamOutputEventsResponse {
                    event: Some(stream_output_events_response::Event::TappedEvent(
                        TappedEvent {
                            component_id: output.output_id.component.id().to_string(),
                            component_type: output.component_type.to_string(),
                            component_kind: output.component_kind.to_string(),
                            event: event_wrapper,
                        },
                    )),
                }
            })
            .collect(),
        TapPayload::Notification(notification) => {
            vec![create_notification_event(notification.as_str())]
        }
    }
}

fn create_notification_event(message: &str) -> StreamOutputEventsResponse {
    StreamOutputEventsResponse {
        event: Some(stream_output_events_response::Event::Notification(
            EventNotification {
                message: message.to_string(),
            },
        )),
    }
}

use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
    time::Duration,
};

use glob::Pattern;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;
use vector_api_client::{
    Client,
    proto::{Component, ComponentType},
};

use crate::state::{self, OutputMetrics, SentEventsMetric};
use vector_common::config::ComponentKey;

fn component_matches_patterns(component_id: &str, components_patterns: &[Pattern]) -> bool {
    if components_patterns.is_empty() {
        return true;
    }

    components_patterns
        .iter()
        .any(|pattern| pattern.matches(component_id))
}

/// Component polling task
///
/// Polls for component changes every interval. gRPC doesn't have real-time component
/// add/remove subscriptions like GraphQL did, so we poll and diff.
async fn poll_components(
    mut client: Client,
    tx: state::EventTx,
    interval_ms: i64,
    components_patterns: Arc<Vec<Pattern>>,
    initial_components: HashSet<String>,
) {
    let mut known_components = initial_components;
    let poll_interval = Duration::from_millis(interval_ms as u64);
    let mut consecutive_errors = 0;
    const MAX_CONSECUTIVE_ERRORS: u32 = 3;

    loop {
        tokio::time::sleep(poll_interval).await;

        // Fetch current components
        let Ok(response) = client.get_components(0).await else {
            consecutive_errors += 1;
            if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                // Exit to trigger reconnection after sustained failures
                return;
            }
            continue;
        };
        consecutive_errors = 0;

        let current_components: std::collections::HashSet<String> = response
            .components
            .iter()
            .map(|c| c.component_id.clone())
            .collect();

        // Detect added components
        for component in &response.components {
            let component_id = &component.component_id;
            if !known_components.contains(component_id)
                && component_matches_patterns(component_id, &components_patterns)
            {
                let row = component_to_row(component);
                _ = tx.send(state::EventType::ComponentAdded(row)).await;
            }
        }

        // Detect removed components
        for old_id in &known_components {
            if !current_components.contains(old_id)
                && component_matches_patterns(old_id, &components_patterns)
            {
                let key = ComponentKey::from(old_id.as_str());
                _ = tx.send(state::EventType::ComponentRemoved(key)).await;
            }
        }

        known_components = current_components;

    }
}

fn component_to_row(component: &Component) -> state::ComponentRow {
    let key = ComponentKey::from(component.component_id.as_str());
    let metrics = component.metrics.as_ref();

    state::ComponentRow {
        key: key.clone(),
        kind: match component.component_type() {
            ComponentType::Source => "source",
            ComponentType::Transform => "transform",
            ComponentType::Sink => "sink",
        }
        .to_string(),
        component_type: component.on_type.clone(), // actual plugin type e.g. "demo_logs", "kafka"
        outputs: component
            .outputs
            .iter()
            .map(|o| {
                (
                    o.output_id.clone(),
                    OutputMetrics::from(o.sent_events_total),
                )
            })
            .collect(),
        received_bytes_total: metrics.and_then(|m| m.received_bytes_total).unwrap_or(0),
        received_bytes_throughput_sec: 0,
        received_events_total: metrics.and_then(|m| m.received_events_total).unwrap_or(0),
        received_events_throughput_sec: 0,
        sent_bytes_total: metrics.and_then(|m| m.sent_bytes_total).unwrap_or(0),
        sent_bytes_throughput_sec: 0,
        sent_events_total: metrics.and_then(|m| m.sent_events_total).unwrap_or(0),
        sent_events_throughput_sec: 0,
        #[cfg(feature = "allocation-tracing")]
        allocated_bytes: 0,
        errors: 0,
    }
}

/// Allocated bytes per component
#[cfg(feature = "allocation-tracing")]
async fn allocated_bytes(
    mut client: Client,
    tx: state::EventTx,
    interval: i64,
    components_patterns: Arc<Vec<Pattern>>,
) {
    let Ok(mut stream) = client
        .stream_component_allocated_bytes(interval as i32)
        .await
    else {
        // Failed to establish stream, will retry on reconnection
        return;
    };

    while let Some(Ok(response)) = stream.next().await {
        let component_id = &response.component_id;
        if !component_matches_patterns(component_id, &components_patterns) {
            continue;
        }

        _ = tx
            .send(state::EventType::AllocatedBytes(vec![(
                ComponentKey::from(component_id.as_str()),
                response.allocated_bytes,
            )]))
            .await;
    }
}

async fn received_bytes_totals(
    mut client: Client,
    tx: state::EventTx,
    interval: i64,
    components_patterns: Arc<Vec<Pattern>>,
) {
    let Ok(mut stream) = client
        .stream_component_received_bytes_total(interval as i32)
        .await
    else {
        // Failed to establish stream, will retry on reconnection
        return;
    };

    while let Some(Ok(response)) = stream.next().await {
        let component_id = &response.component_id;
        if !component_matches_patterns(component_id, &components_patterns) {
            continue;
        }

        // Send immediately to ensure live updates for systems with <10 components
        _ = tx
            .send(state::EventType::ReceivedBytesTotals(vec![(
                ComponentKey::from(component_id.as_str()),
                response.total,
            )]))
            .await;
    }
}

async fn received_bytes_throughputs(
    mut client: Client,
    tx: state::EventTx,
    interval: i64,
    components_patterns: Arc<Vec<Pattern>>,
) {
    let Ok(mut stream) = client
        .stream_component_received_bytes_throughput(interval as i32)
        .await
    else {
        // Failed to establish stream, will retry on reconnection
        return;
    };

    while let Some(Ok(response)) = stream.next().await {
        let component_id = &response.component_id;
        if !component_matches_patterns(component_id, &components_patterns) {
            continue;
        }

        // Send immediately to ensure live updates for systems with <10 components
        _ = tx
            .send(state::EventType::ReceivedBytesThroughputs(
                interval,
                vec![(
                    ComponentKey::from(component_id.as_str()),
                    response.throughput as i64,
                )],
            ))
            .await;
    }
}

async fn received_events_totals(
    mut client: Client,
    tx: state::EventTx,
    interval: i64,
    components_patterns: Arc<Vec<Pattern>>,
) {
    let Ok(mut stream) = client
        .stream_component_received_events_total(interval as i32)
        .await
    else {
        // Failed to establish stream, will retry on reconnection
        return;
    };

    while let Some(Ok(response)) = stream.next().await {
        let component_id = &response.component_id;
        if !component_matches_patterns(component_id, &components_patterns) {
            continue;
        }

        // Send immediately to ensure live updates for systems with <10 components
        _ = tx
            .send(state::EventType::ReceivedEventsTotals(vec![(
                ComponentKey::from(component_id.as_str()),
                response.total,
            )]))
            .await;
    }
}

async fn received_events_throughputs(
    mut client: Client,
    tx: state::EventTx,
    interval: i64,
    components_patterns: Arc<Vec<Pattern>>,
) {
    let Ok(mut stream) = client
        .stream_component_received_events_throughput(interval as i32)
        .await
    else {
        // Failed to establish stream, will retry on reconnection
        return;
    };

    while let Some(Ok(response)) = stream.next().await {
        let component_id = &response.component_id;
        if !component_matches_patterns(component_id, &components_patterns) {
            continue;
        }

        // Send immediately to ensure live updates for systems with <10 components
        _ = tx
            .send(state::EventType::ReceivedEventsThroughputs(
                interval,
                vec![(
                    ComponentKey::from(component_id.as_str()),
                    response.throughput as i64,
                )],
            ))
            .await;
    }
}

async fn sent_bytes_totals(
    mut client: Client,
    tx: state::EventTx,
    interval: i64,
    components_patterns: Arc<Vec<Pattern>>,
) {
    let Ok(mut stream) = client
        .stream_component_sent_bytes_total(interval as i32)
        .await
    else {
        // Failed to establish stream, will retry on reconnection
        return;
    };

    while let Some(Ok(response)) = stream.next().await {
        let component_id = &response.component_id;
        if !component_matches_patterns(component_id, &components_patterns) {
            continue;
        }

        // Send immediately to ensure live updates for systems with <10 components
        _ = tx
            .send(state::EventType::SentBytesTotals(vec![(
                ComponentKey::from(component_id.as_str()),
                response.total,
            )]))
            .await;
    }
}

async fn sent_bytes_throughputs(
    mut client: Client,
    tx: state::EventTx,
    interval: i64,
    components_patterns: Arc<Vec<Pattern>>,
) {
    let Ok(mut stream) = client
        .stream_component_sent_bytes_throughput(interval as i32)
        .await
    else {
        // Failed to establish stream, will retry on reconnection
        return;
    };

    while let Some(Ok(response)) = stream.next().await {
        let component_id = &response.component_id;
        if !component_matches_patterns(component_id, &components_patterns) {
            continue;
        }

        // Send immediately to ensure live updates for systems with <10 components
        _ = tx
            .send(state::EventType::SentBytesThroughputs(
                interval,
                vec![(
                    ComponentKey::from(component_id.as_str()),
                    response.throughput as i64,
                )],
            ))
            .await;
    }
}

async fn sent_events_totals(
    mut client: Client,
    tx: state::EventTx,
    interval: i64,
    components_patterns: Arc<Vec<Pattern>>,
) {
    let Ok(mut stream) = client
        .stream_component_sent_events_total(interval as i32)
        .await
    else {
        // Failed to establish stream, will retry on reconnection
        return;
    };

    while let Some(Ok(response)) = stream.next().await {
        let component_id = &response.component_id;
        if !component_matches_patterns(component_id, &components_patterns) {
            continue;
        }

        _ = tx
            .send(state::EventType::SentEventsTotals(vec![SentEventsMetric {
                key: ComponentKey::from(component_id.as_str()),
                total: response.total,
                outputs: response.output_totals.into_iter().collect(),
            }]))
            .await;
    }
}

async fn sent_events_throughputs(
    mut client: Client,
    tx: state::EventTx,
    interval: i64,
    components_patterns: Arc<Vec<Pattern>>,
) {
    let Ok(mut stream) = client
        .stream_component_sent_events_throughput(interval as i32)
        .await
    else {
        // Failed to establish stream, will retry on reconnection
        return;
    };

    while let Some(Ok(response)) = stream.next().await {
        let component_id = &response.component_id;
        if !component_matches_patterns(component_id, &components_patterns) {
            continue;
        }

        _ = tx
            .send(state::EventType::SentEventsThroughputs(
                interval,
                vec![SentEventsMetric {
                    key: ComponentKey::from(component_id.as_str()),
                    total: response.throughput as i64,
                    outputs: response
                        .output_throughputs
                        .into_iter()
                        .map(|(k, v)| (k, v as i64))
                        .collect(),
                }],
            ))
            .await;
    }
}

async fn errors_totals(
    mut client: Client,
    tx: state::EventTx,
    interval: i64,
    components_patterns: Arc<Vec<Pattern>>,
) {
    let Ok(mut stream) = client.stream_component_errors_total(interval as i32).await else {
        return;
    };

    while let Some(Ok(response)) = stream.next().await {
        let component_id = &response.component_id;
        if !component_matches_patterns(component_id, &components_patterns) {
            continue;
        }

        // Send immediately to ensure live updates for systems with <10 components
        _ = tx
            .send(state::EventType::ErrorsTotals(vec![(
                ComponentKey::from(component_id.as_str()),
                response.total,
            )]))
            .await;
    }
}

async fn uptime_changed(mut client: Client, tx: state::EventTx, interval: i64) {
    let Ok(mut stream) = client.stream_uptime(interval as i32).await else {
        return;
    };

    while let Some(Ok(response)) = stream.next().await {
        _ = tx
            .send(state::EventType::UptimeChanged(
                response.uptime_seconds as f64,
            ))
            .await;
    }
}

/// Handles returned by [`subscribe`], split by lifecycle.
pub struct SubscribeHandles {
    /// Metric stream tasks — exit when a gRPC stream closes or errors.
    /// `cmd.rs` joins these to detect connection loss and trigger reconnection.
    pub metric_handles: Vec<JoinHandle<()>>,
    /// Polls `get_components` for topology changes. Designed to run indefinitely
    /// while the server is healthy, so it must be aborted separately rather than
    /// joined alongside the metric handles.
    pub poll_handle: JoinHandle<()>,
}

/// Subscribe to each metrics stream, all sharing a single underlying gRPC connection.
/// HTTP/2 multiplexes the concurrent streams — cloning a connected `Client` is cheap
/// (the tonic `Channel` is Arc-backed) and avoids redundant TCP/HTTP2 handshakes.
pub async fn subscribe(
    url: String,
    tx: state::EventTx,
    interval: i64,
    components_patterns: Vec<Pattern>,
    initial_components: HashSet<String>,
) -> Result<SubscribeHandles, vector_api_client::Error> {
    let components_patterns = Arc::new(components_patterns);

    let mut client = Client::new(url.as_str());
    client.connect().await?;

    let poll_handle = tokio::spawn(poll_components(
        client.clone(),
        tx.clone(),
        interval,
        Arc::clone(&components_patterns),
        initial_components,
    ));

    #[cfg_attr(not(feature = "allocation-tracing"), allow(unused_mut))]
    let mut metric_handles = vec![
        tokio::spawn(received_bytes_totals(
            client.clone(),
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(received_bytes_throughputs(
            client.clone(),
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(received_events_totals(
            client.clone(),
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(received_events_throughputs(
            client.clone(),
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(sent_bytes_totals(
            client.clone(),
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(sent_bytes_throughputs(
            client.clone(),
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(sent_events_totals(
            client.clone(),
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(sent_events_throughputs(
            client.clone(),
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(errors_totals(
            client.clone(),
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(uptime_changed(client.clone(), tx.clone(), interval)),
    ];

    #[cfg(feature = "allocation-tracing")]
    metric_handles.push(tokio::spawn(allocated_bytes(
        client,
        tx,
        interval,
        Arc::clone(&components_patterns),
    )));

    Ok(SubscribeHandles {
        metric_handles,
        poll_handle,
    })
}

/// Retrieve the initial components/metrics for first paint. Further updating the metrics
/// will be handled by subscriptions.
pub async fn init_components(
    url: &str,
    components_patterns: &[Pattern],
) -> Result<state::State, vector_api_client::Error> {
    let mut client = Client::new(url);
    client.connect().await?;

    // Get all components
    let response = client.get_components(0).await?;

    let rows = response
        .components
        .into_iter()
        .filter(|component| {
            component_matches_patterns(&component.component_id, components_patterns)
        })
        .map(|component| {
            let row = component_to_row(&component);
            (row.key.clone(), row)
        })
        .collect::<BTreeMap<_, _>>();

    Ok(state::State::new(rows))
}

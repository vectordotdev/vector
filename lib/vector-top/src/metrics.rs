use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
    time::Duration,
};

use glob::Pattern;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;
use vector_api_client::{Client, proto::Component};

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
) {
    let mut known_components: std::collections::HashSet<String> = std::collections::HashSet::new();
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
        kind: component.on_type.clone(),
        component_type: format!("{:?}", component.component_type()),
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

    let mut batch = Vec::new();
    while let Some(Ok(response)) = stream.next().await {
        let component_id = &response.component_id;
        if !component_matches_patterns(component_id, &components_patterns) {
            continue;
        }

        batch.push((ComponentKey::from(component_id.as_str()), response.total));

        // Send in batches (gRPC streams one at a time, but we want to batch)
        if batch.len() >= 10 {
            _ = tx
                .send(state::EventType::ReceivedBytesTotals(batch.clone()))
                .await;
            batch.clear();
        }
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

    let mut batch = Vec::new();
    while let Some(Ok(response)) = stream.next().await {
        let component_id = &response.component_id;
        if !component_matches_patterns(component_id, &components_patterns) {
            continue;
        }

        batch.push((
            ComponentKey::from(component_id.as_str()),
            response.throughput as i64,
        ));

        if batch.len() >= 10 {
            _ = tx
                .send(state::EventType::ReceivedBytesThroughputs(
                    interval,
                    batch.clone(),
                ))
                .await;
            batch.clear();
        }
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

    let mut batch = Vec::new();
    while let Some(Ok(response)) = stream.next().await {
        let component_id = &response.component_id;
        if !component_matches_patterns(component_id, &components_patterns) {
            continue;
        }

        batch.push((ComponentKey::from(component_id.as_str()), response.total));

        if batch.len() >= 10 {
            _ = tx
                .send(state::EventType::ReceivedEventsTotals(batch.clone()))
                .await;
            batch.clear();
        }
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

    let mut batch = Vec::new();
    while let Some(Ok(response)) = stream.next().await {
        let component_id = &response.component_id;
        if !component_matches_patterns(component_id, &components_patterns) {
            continue;
        }

        batch.push((
            ComponentKey::from(component_id.as_str()),
            response.throughput as i64,
        ));

        if batch.len() >= 10 {
            _ = tx
                .send(state::EventType::ReceivedEventsThroughputs(
                    interval,
                    batch.clone(),
                ))
                .await;
            batch.clear();
        }
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

    let mut batch = Vec::new();
    while let Some(Ok(response)) = stream.next().await {
        let component_id = &response.component_id;
        if !component_matches_patterns(component_id, &components_patterns) {
            continue;
        }

        batch.push((ComponentKey::from(component_id.as_str()), response.total));

        if batch.len() >= 10 {
            _ = tx
                .send(state::EventType::SentBytesTotals(batch.clone()))
                .await;
            batch.clear();
        }
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

    let mut batch = Vec::new();
    while let Some(Ok(response)) = stream.next().await {
        let component_id = &response.component_id;
        if !component_matches_patterns(component_id, &components_patterns) {
            continue;
        }

        batch.push((
            ComponentKey::from(component_id.as_str()),
            response.throughput as i64,
        ));

        if batch.len() >= 10 {
            _ = tx
                .send(state::EventType::SentBytesThroughputs(
                    interval,
                    batch.clone(),
                ))
                .await;
            batch.clear();
        }
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

    let mut batch = Vec::new();
    while let Some(Ok(response)) = stream.next().await {
        let component_id = &response.component_id;
        if !component_matches_patterns(component_id, &components_patterns) {
            continue;
        }

        // Note: gRPC doesn't have output-level metrics in the streaming response
        // We only have component-level totals
        batch.push(SentEventsMetric {
            key: ComponentKey::from(component_id.as_str()),
            total: response.total,
            outputs: HashMap::new(), // No per-output data in gRPC streams
        });

        if batch.len() >= 10 {
            _ = tx
                .send(state::EventType::SentEventsTotals(batch.clone()))
                .await;
            batch.clear();
        }
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

    let mut batch = Vec::new();
    while let Some(Ok(response)) = stream.next().await {
        let component_id = &response.component_id;
        if !component_matches_patterns(component_id, &components_patterns) {
            continue;
        }

        batch.push(SentEventsMetric {
            key: ComponentKey::from(component_id.as_str()),
            total: response.throughput as i64,
            outputs: HashMap::new(), // No per-output data in gRPC streams
        });

        if batch.len() >= 10 {
            _ = tx
                .send(state::EventType::SentEventsThroughputs(
                    interval,
                    batch.clone(),
                ))
                .await;
            batch.clear();
        }
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

    let mut batch = Vec::new();
    while let Some(Ok(response)) = stream.next().await {
        let component_id = &response.component_id;
        if !component_matches_patterns(component_id, &components_patterns) {
            continue;
        }

        batch.push((ComponentKey::from(component_id.as_str()), response.total));

        if batch.len() >= 10 {
            _ = tx.send(state::EventType::ErrorsTotals(batch.clone())).await;
            batch.clear();
        }
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

/// Subscribe to each metrics stream through separate gRPC client connections.
/// Each subscription gets its own client to allow concurrent streaming.
pub async fn subscribe(
    url: String,
    tx: state::EventTx,
    interval: i64,
    components_patterns: Vec<Pattern>,
) -> Result<Vec<JoinHandle<()>>, vector_api_client::Error> {
    let components_patterns = Arc::new(components_patterns);

    // Helper to create and connect a new client
    let create_client = || async {
        let mut client = Client::new(url.as_str()).await?;
        client.connect().await?;
        Ok::<Client, vector_api_client::Error>(client)
    };

    #[cfg_attr(not(feature = "allocation-tracing"), allow(unused_mut))]
    let mut handles = vec![
        tokio::spawn(poll_components(
            create_client().await?,
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(received_bytes_totals(
            create_client().await?,
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(received_bytes_throughputs(
            create_client().await?,
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(received_events_totals(
            create_client().await?,
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(received_events_throughputs(
            create_client().await?,
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(sent_bytes_totals(
            create_client().await?,
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(sent_bytes_throughputs(
            create_client().await?,
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(sent_events_totals(
            create_client().await?,
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(sent_events_throughputs(
            create_client().await?,
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(errors_totals(
            create_client().await?,
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(uptime_changed(create_client().await?, tx.clone(), interval)),
    ];

    #[cfg(feature = "allocation-tracing")]
    handles.push(tokio::spawn(allocated_bytes(
        create_client().await?,
        tx,
        interval,
        Arc::clone(&components_patterns),
    )));

    Ok(handles)
}

/// Retrieve the initial components/metrics for first paint. Further updating the metrics
/// will be handled by subscriptions.
pub async fn init_components(
    url: &str,
    components_patterns: &[Pattern],
) -> Result<state::State, vector_api_client::Error> {
    let mut client = Client::new(url).await?;
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

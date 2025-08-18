use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use glob::Pattern;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;
use vector_lib::api_client::{
    gql::{ComponentsQueryExt, ComponentsSubscriptionExt, MetricsSubscriptionExt},
    Client, SubscriptionClient,
};

use super::state::{self, OutputMetrics};
use crate::{config::ComponentKey, top::state::SentEventsMetric};

fn component_matches_patterns(component_id: &str, components_patterns: &[Pattern]) -> bool {
    if components_patterns.is_empty() {
        return true;
    }

    components_patterns
        .iter()
        .any(|pattern| pattern.matches(component_id))
}

/// Components that have been added
async fn component_added(
    client: Arc<SubscriptionClient>,
    tx: state::EventTx,
    components_patterns: Arc<Vec<Pattern>>,
) {
    tokio::pin! {
        let stream = client.component_added();
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_added;
            let component_id = c.component_id;
            if !component_matches_patterns(&component_id, &components_patterns) {
                continue;
            }
            let key = ComponentKey::from(component_id);
            _ = tx
                .send(state::EventType::ComponentAdded(state::ComponentRow {
                    key,
                    kind: c.on.to_string(),
                    component_type: c.component_type,
                    outputs: HashMap::new(),
                    received_bytes_total: 0,
                    received_bytes_throughput_sec: 0,
                    received_events_total: 0,
                    received_events_throughput_sec: 0,
                    sent_bytes_total: 0,
                    sent_bytes_throughput_sec: 0,
                    sent_events_total: 0,
                    sent_events_throughput_sec: 0,
                    #[cfg(feature = "allocation-tracing")]
                    allocated_bytes: 0,
                    errors: 0,
                }))
                .await;
        }
    }
}

/// Allocated bytes per component
#[cfg(feature = "allocation-tracing")]
async fn allocated_bytes(
    client: Arc<SubscriptionClient>,
    tx: state::EventTx,
    interval: i64,
    components_patterns: Arc<Vec<Pattern>>,
) {
    tokio::pin! {
        let stream = client.component_allocated_bytes_subscription(interval);
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_allocated_bytes;
            _ = tx
                .send(state::EventType::AllocatedBytes(
                    c.into_iter()
                        .filter(|c| {
                            component_matches_patterns(&c.component_id, &components_patterns)
                        })
                        .map(|c| {
                            (
                                ComponentKey::from(c.component_id),
                                c.metric.allocated_bytes as i64,
                            )
                        })
                        .collect(),
                ))
                .await;
        }
    }
}
/// Components that have been removed
async fn component_removed(
    client: Arc<SubscriptionClient>,
    tx: state::EventTx,
    components_patterns: Arc<Vec<Pattern>>,
) {
    tokio::pin! {
        let stream = client.component_removed();
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_removed;
            let component_id = c.component_id;
            if !component_matches_patterns(&component_id, &components_patterns) {
                continue;
            }
            let id = ComponentKey::from(component_id);
            _ = tx.send(state::EventType::ComponentRemoved(id)).await;
        }
    }
}

async fn received_bytes_totals(
    client: Arc<SubscriptionClient>,
    tx: state::EventTx,
    interval: i64,
    components_patterns: Arc<Vec<Pattern>>,
) {
    tokio::pin! {
        let stream = client.component_received_bytes_totals_subscription(interval);
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_received_bytes_totals;

            _ = tx
                .send(state::EventType::ReceivedBytesTotals(
                    c.into_iter()
                        .filter(|c| {
                            component_matches_patterns(&c.component_id, &components_patterns)
                        })
                        .map(|c| {
                            (
                                ComponentKey::from(c.component_id),
                                c.metric.received_bytes_total as i64,
                            )
                        })
                        .collect(),
                ))
                .await;
        }
    }
}

async fn received_bytes_throughputs(
    client: Arc<SubscriptionClient>,
    tx: state::EventTx,
    interval: i64,
    components_patterns: Arc<Vec<Pattern>>,
) {
    tokio::pin! {
        let stream = client.component_received_bytes_throughputs_subscription(interval);
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_received_bytes_throughputs;
            _ = tx
                .send(state::EventType::ReceivedBytesThroughputs(
                    interval,
                    c.into_iter()
                        .filter(|c| {
                            component_matches_patterns(&c.component_id, &components_patterns)
                        })
                        .map(|c| (ComponentKey::from(c.component_id), c.throughput))
                        .collect(),
                ))
                .await;
        }
    }
}

async fn received_events_totals(
    client: Arc<SubscriptionClient>,
    tx: state::EventTx,
    interval: i64,
    components_patterns: Arc<Vec<Pattern>>,
) {
    tokio::pin! {
        let stream = client.component_received_events_totals_subscription(interval);
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_received_events_totals;
            _ = tx
                .send(state::EventType::ReceivedEventsTotals(
                    c.into_iter()
                        .filter(|c| {
                            component_matches_patterns(&c.component_id, &components_patterns)
                        })
                        .map(|c| {
                            (
                                ComponentKey::from(c.component_id),
                                c.metric.received_events_total as i64,
                            )
                        })
                        .collect(),
                ))
                .await;
        }
    }
}

async fn received_events_throughputs(
    client: Arc<SubscriptionClient>,
    tx: state::EventTx,
    interval: i64,
    components_patterns: Arc<Vec<Pattern>>,
) {
    tokio::pin! {
        let stream = client.component_received_events_throughputs_subscription(interval);
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_received_events_throughputs;
            _ = tx
                .send(state::EventType::ReceivedEventsThroughputs(
                    interval,
                    c.into_iter()
                        .filter(|c| {
                            component_matches_patterns(&c.component_id, &components_patterns)
                        })
                        .map(|c| (ComponentKey::from(c.component_id), c.throughput))
                        .collect(),
                ))
                .await;
        }
    }
}

async fn sent_bytes_totals(
    client: Arc<SubscriptionClient>,
    tx: state::EventTx,
    interval: i64,
    components_patterns: Arc<Vec<Pattern>>,
) {
    tokio::pin! {
        let stream = client.component_sent_bytes_totals_subscription(interval);
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_sent_bytes_totals;
            _ = tx
                .send(state::EventType::SentBytesTotals(
                    c.into_iter()
                        .filter(|c| {
                            component_matches_patterns(&c.component_id, &components_patterns)
                        })
                        .map(|c| {
                            (
                                ComponentKey::from(c.component_id),
                                c.metric.sent_bytes_total as i64,
                            )
                        })
                        .collect(),
                ))
                .await;
        }
    }
}

async fn sent_bytes_throughputs(
    client: Arc<SubscriptionClient>,
    tx: state::EventTx,
    interval: i64,
    components_patterns: Arc<Vec<Pattern>>,
) {
    tokio::pin! {
        let stream = client.component_sent_bytes_throughputs_subscription(interval);
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_sent_bytes_throughputs;
            _ = tx
                .send(state::EventType::SentBytesThroughputs(
                    interval,
                    c.into_iter()
                        .filter(|c| {
                            component_matches_patterns(&c.component_id, &components_patterns)
                        })
                        .map(|c| (ComponentKey::from(c.component_id), c.throughput))
                        .collect(),
                ))
                .await;
        }
    }
}

async fn sent_events_totals(
    client: Arc<SubscriptionClient>,
    tx: state::EventTx,
    interval: i64,
    components_patterns: Arc<Vec<Pattern>>,
) {
    tokio::pin! {
        let stream = client.component_sent_events_totals_subscription(interval);
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_sent_events_totals;
            _ = tx
                .send(state::EventType::SentEventsTotals(
                    c.into_iter()
                        .filter(|c| {
                            component_matches_patterns(&c.component_id, &components_patterns)
                        })
                        .map(|c| SentEventsMetric {
                            key: ComponentKey::from(c.component_id.as_str()),
                            total: c.metric.sent_events_total as i64,
                            outputs: c.outputs().into_iter().collect(),
                        })
                        .collect(),
                ))
                .await;
        }
    }
}

async fn sent_events_throughputs(
    client: Arc<SubscriptionClient>,
    tx: state::EventTx,
    interval: i64,
    components_patterns: Arc<Vec<Pattern>>,
) {
    tokio::pin! {
        let stream = client.component_sent_events_throughputs_subscription(interval);
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_sent_events_throughputs;
            _ = tx
                .send(state::EventType::SentEventsThroughputs(
                    interval,
                    c.into_iter()
                        .filter(|c| {
                            component_matches_patterns(&c.component_id, &components_patterns)
                        })
                        .map(|c| SentEventsMetric {
                            key: ComponentKey::from(c.component_id.as_str()),
                            total: c.throughput,
                            outputs: c.outputs().into_iter().collect(),
                        })
                        .collect(),
                ))
                .await;
        }
    }
}

async fn errors_totals(
    client: Arc<SubscriptionClient>,
    tx: state::EventTx,
    interval: i64,
    components_patterns: Arc<Vec<Pattern>>,
) {
    tokio::pin! {
        let stream = client.component_errors_totals_subscription(interval);
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_errors_totals;
            _ = tx
                .send(state::EventType::ErrorsTotals(
                    c.into_iter()
                        .filter(|c| {
                            component_matches_patterns(&c.component_id, &components_patterns)
                        })
                        .map(|c| {
                            (
                                ComponentKey::from(c.component_id.as_str()),
                                c.metric.errors_total as i64,
                            )
                        })
                        .collect(),
                ))
                .await;
        }
    }
}

async fn uptime_changed(client: Arc<SubscriptionClient>, tx: state::EventTx) {
    tokio::pin! {
        let stream = client.uptime_subscription();
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            _ = tx
                .send(state::EventType::UptimeChanged(d.uptime.seconds))
                .await;
        }
    }
}

/// Subscribe to each metrics channel through a separate client. This is a temporary workaround
/// until client multiplexing is fixed. In future, we should be able to use a single client
pub fn subscribe(
    client: SubscriptionClient,
    tx: state::EventTx,
    interval: i64,
    components_patterns: Vec<Pattern>,
) -> Vec<JoinHandle<()>> {
    let client = Arc::new(client);
    let components_patterns = Arc::new(components_patterns);
    vec![
        tokio::spawn(component_added(
            Arc::clone(&client),
            tx.clone(),
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(component_removed(
            Arc::clone(&client),
            tx.clone(),
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(received_bytes_totals(
            Arc::clone(&client),
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(received_bytes_throughputs(
            Arc::clone(&client),
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(received_events_totals(
            Arc::clone(&client),
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(received_events_throughputs(
            Arc::clone(&client),
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(sent_bytes_totals(
            Arc::clone(&client),
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(sent_bytes_throughputs(
            Arc::clone(&client),
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(sent_events_totals(
            Arc::clone(&client),
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(sent_events_throughputs(
            Arc::clone(&client),
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        #[cfg(feature = "allocation-tracing")]
        tokio::spawn(allocated_bytes(
            Arc::clone(&client),
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(errors_totals(
            Arc::clone(&client),
            tx.clone(),
            interval,
            Arc::clone(&components_patterns),
        )),
        tokio::spawn(uptime_changed(Arc::clone(&client), tx)),
    ]
}

/// Retrieve the initial components/metrics for first paint. Further updating the metrics
/// will be handled by subscriptions.
pub async fn init_components(
    client: &Client,
    components_patterns: &[Pattern],
) -> Result<state::State, ()> {
    // Execute a query to get the latest components, and aggregate metrics for each resource.
    // Since we don't know currently have a mechanism for scrolling/paging through results,
    // we're using an artificially high page size to capture all likely component configurations.
    let rows = client
        .components_query(i16::MAX as i64)
        .await
        .map_err(|_| ())?
        .data
        .ok_or(())?
        .components
        .edges
        .into_iter()
        .filter(|edge| component_matches_patterns(&edge.node.component_id, components_patterns))
        .map(|edge| {
            let d = edge.node;
            let key = ComponentKey::from(d.component_id);
            (
                key.clone(),
                state::ComponentRow {
                    key,
                    kind: d.on.to_string(),
                    component_type: d.component_type,
                    outputs: d
                        .on
                        .outputs()
                        .into_iter()
                        .map(|(id, sent_events_total)| (id, OutputMetrics::from(sent_events_total)))
                        .collect(),
                    received_bytes_total: d.on.received_bytes_total(),
                    received_bytes_throughput_sec: 0,
                    received_events_total: d.on.received_events_total(),
                    received_events_throughput_sec: 0,
                    sent_bytes_total: d.on.sent_bytes_total(),
                    sent_bytes_throughput_sec: 0,
                    sent_events_total: d.on.sent_events_total(),
                    sent_events_throughput_sec: 0,
                    #[cfg(feature = "allocation-tracing")]
                    allocated_bytes: 0,
                    errors: 0,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();

    Ok(state::State::new(rows))
}

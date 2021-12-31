use std::sync::Arc;

use tokio_stream::StreamExt;
use vector_api_client::{
    gql::{ComponentsQueryExt, ComponentsSubscriptionExt, MetricsSubscriptionExt},
    Client, SubscriptionClient,
};

use super::state;
use crate::config::ComponentKey;

/// Components that have been added
async fn component_added(client: Arc<SubscriptionClient>, tx: state::EventTx) {
    let res = client.component_added();

    tokio::pin! {
        let stream = res.stream();
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_added;
            let key = ComponentKey::from(c.component_id);
            let _ = tx
                .send(state::EventType::ComponentAdded(state::ComponentRow {
                    key,
                    kind: c.on.to_string(),
                    component_type: c.component_type,
                    received_events_total: 0,
                    received_events_throughput_sec: 0,
                    sent_events_total: 0,
                    sent_events_throughput_sec: 0,
                    processed_bytes_total: 0,
                    processed_bytes_throughput_sec: 0,
                    errors: 0,
                }))
                .await;
        }
    }
}

/// Components that have been removed
async fn component_removed(client: Arc<SubscriptionClient>, tx: state::EventTx) {
    let res = client.component_removed();

    tokio::pin! {
        let stream = res.stream();
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_removed;
            let id = ComponentKey::from(c.component_id.as_str());
            let _ = tx.send(state::EventType::ComponentRemoved(id)).await;
        }
    }
}

async fn received_events_totals(
    client: Arc<SubscriptionClient>,
    tx: state::EventTx,
    interval: i64,
) {
    let res = client.component_received_events_totals_subscription(interval);

    tokio::pin! {
        let stream = res.stream();
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_received_events_totals;
            let _ = tx
                .send(state::EventType::ReceivedEventsTotals(
                    c.into_iter()
                        .map(|c| {
                            (
                                ComponentKey::from(c.component_id.as_str()),
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
) {
    let res = client.component_received_events_throughputs_subscription(interval);

    tokio::pin! {
        let stream = res.stream();
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_received_events_throughputs;
            let _ = tx
                .send(state::EventType::ReceivedEventsThroughputs(
                    interval,
                    c.into_iter()
                        .map(|c| (ComponentKey::from(c.component_id.as_str()), c.throughput))
                        .collect(),
                ))
                .await;
        }
    }
}

async fn sent_events_totals(client: Arc<SubscriptionClient>, tx: state::EventTx, interval: i64) {
    let res = client.component_sent_events_totals_subscription(interval);

    tokio::pin! {
        let stream = res.stream();
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_sent_events_totals;
            let _ = tx
                .send(state::EventType::SentEventsTotals(
                    c.into_iter()
                        .map(|c| {
                            (
                                ComponentKey::from(c.component_id.as_str()),
                                c.metric.sent_events_total as i64,
                            )
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
) {
    let res = client.component_sent_events_throughputs_subscription(interval);

    tokio::pin! {
        let stream = res.stream();
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_sent_events_throughputs;
            let _ = tx
                .send(state::EventType::SentEventsThroughputs(
                    interval,
                    c.into_iter()
                        .map(|c| (ComponentKey::from(c.component_id.as_str()), c.throughput))
                        .collect(),
                ))
                .await;
        }
    }
}

async fn processed_bytes_totals(
    client: Arc<SubscriptionClient>,
    tx: state::EventTx,
    interval: i64,
) {
    let res = client.component_processed_bytes_totals_subscription(interval);

    tokio::pin! {
        let stream = res.stream();
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_processed_bytes_totals;
            let _ = tx
                .send(state::EventType::ProcessedBytesTotals(
                    c.into_iter()
                        .map(|c| {
                            (
                                ComponentKey::from(c.component_id.as_str()),
                                c.metric.processed_bytes_total as i64,
                            )
                        })
                        .collect(),
                ))
                .await;
        }
    }
}

async fn processed_bytes_throughputs(
    client: Arc<SubscriptionClient>,
    tx: state::EventTx,
    interval: i64,
) {
    let res = client.component_processed_bytes_throughputs_subscription(interval);

    tokio::pin! {
        let stream = res.stream();
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_processed_bytes_throughputs;
            let _ = tx
                .send(state::EventType::ProcessedBytesThroughputs(
                    interval,
                    c.into_iter()
                        .map(|c| (ComponentKey::from(c.component_id.as_str()), c.throughput))
                        .collect(),
                ))
                .await;
        }
    }
}

/// Subscribe to each metrics channel through a separate client. This is a temporary workaround
/// until client multiplexing is fixed. In future, we should be able to use a single client
pub fn subscribe(client: SubscriptionClient, tx: state::EventTx, interval: i64) {
    let client = Arc::new(client);

    tokio::spawn(component_added(Arc::clone(&client), tx.clone()));
    tokio::spawn(component_removed(Arc::clone(&client), tx.clone()));
    tokio::spawn(received_events_totals(
        Arc::clone(&client),
        tx.clone(),
        interval,
    ));
    tokio::spawn(received_events_throughputs(
        Arc::clone(&client),
        tx.clone(),
        interval,
    ));
    tokio::spawn(sent_events_totals(
        Arc::clone(&client),
        tx.clone(),
        interval,
    ));
    tokio::spawn(sent_events_throughputs(
        Arc::clone(&client),
        tx.clone(),
        interval,
    ));
    tokio::spawn(processed_bytes_totals(
        Arc::clone(&client),
        tx.clone(),
        interval,
    ));
    tokio::spawn(processed_bytes_throughputs(
        Arc::clone(&client),
        tx,
        interval,
    ));
}

/// Retrieve the initial components/metrics for first paint. Further updating the metrics
/// will be handled by subscriptions.
pub async fn init_components(client: &Client) -> Result<state::State, ()> {
    // Execute a query to get the latest components, and aggregate metrics for each resource.
    // Since we don't know currently have a mechanism for scrolling/paging through results,
    // we're using an artificially high page size to capture all likely component configurations.
    let rows = client
        .components_query(i16::max_value() as i64)
        .await
        .map_err(|_| ())?
        .data
        .ok_or(())?
        .components
        .edges
        .into_iter()
        .flat_map(|d| {
            d.into_iter().filter_map(|edge| {
                let d = edge?.node;
                let key = ComponentKey::from(d.component_id);
                Some((
                    key.clone(),
                    state::ComponentRow {
                        key,
                        kind: d.on.to_string(),
                        component_type: d.component_type,
                        received_events_total: d.on.received_events_total(),
                        received_events_throughput_sec: 0,
                        sent_events_total: d.on.sent_events_total(),
                        sent_events_throughput_sec: 0,
                        processed_bytes_total: d.on.processed_bytes_total(),
                        processed_bytes_throughput_sec: 0,

                        errors: 0,
                    },
                ))
            })
        })
        .collect::<state::State>();

    Ok(rows)
}

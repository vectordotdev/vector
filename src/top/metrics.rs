use super::state;
use std::sync::Arc;
use tokio::stream::StreamExt;
use vector_api_client::{
    gql::{ComponentsQueryExt, MetricsSubscriptionExt},
    Client, SubscriptionClient,
};

/// Events processed metrics
async fn events_processed(client: Arc<SubscriptionClient>, mut tx: state::EventTx, interval: i64) {
    if let Ok(res) = client
        .component_events_processed_total_subscription(interval)
        .await
    {
        tokio::pin! {
            let stream = res.stream();
        };

        while let Some(Some(res)) = stream.next().await {
            if let Some(d) = res.data {
                let c = d.component_events_processed_total;
                let _ = tx
                    .send((
                        c.name,
                        state::EventType::EventsProcessedTotal(
                            c.metric.events_processed_total as i64,
                        ),
                    ))
                    .await;
            }
        }
    }
}

/// Bytes processed metrics
async fn bytes_processed(client: Arc<SubscriptionClient>, mut tx: state::EventTx, interval: i64) {
    if let Ok(res) = client
        .component_bytes_processed_total_subscription(interval)
        .await
    {
        tokio::pin! {
            let stream = res.stream();
        };

        while let Some(Some(res)) = stream.next().await {
            if let Some(d) = res.data {
                let c = d.component_bytes_processed_total;
                let _ = tx
                    .send((
                        c.name,
                        state::EventType::BytesProcessedTotal(
                            c.metric.bytes_processed_total as i64,
                        ),
                    ))
                    .await;
            }
        }
    }
}

pub fn subscribe(client: SubscriptionClient, tx: state::EventTx, interval: i64) {
    let client = Arc::new(client);

    tokio::spawn(events_processed(Arc::clone(&client), tx.clone(), interval));
    tokio::spawn(bytes_processed(Arc::clone(&client), tx.clone(), interval));
}

/// Retrieve the initial components/metrics for first paint. Further updating the metrics
/// will be handled by subscriptions.
pub async fn init_components(client: &Client) -> Result<state::State, ()> {
    // Execute a query to get the latest components, and aggregate metrics for each resource
    let rows = client
        .components_query()
        .await
        .map_err(|_| ())?
        .data
        .ok_or_else(|| ())?
        .components
        .into_iter()
        .map(|d| {
            (
                d.name.clone(),
                state::ComponentRow {
                    name: d.name,
                    component_type: d.on.to_string(),
                    events_processed_total: d
                        .events_processed_total
                        .as_ref()
                        .map(|ep| ep.events_processed_total as i64)
                        .unwrap_or(0),
                    bytes_processed_total: d
                        .bytes_processed_total
                        .as_ref()
                        .map(|ep| ep.bytes_processed_total as i64)
                        .unwrap_or(0),
                    errors: 0,
                },
            )
        })
        .collect::<state::State>();

    Ok(rows)
}

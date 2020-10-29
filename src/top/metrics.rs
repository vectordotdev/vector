use super::state::{ComponentRow, ComponentsState, WidgetsState};
use std::sync::Arc;
use tokio::stream::StreamExt;
use vector_api_client::{
    gql::{ComponentsQueryExt, MetricsSubscriptionExt},
    Client, SubscriptionClient,
};

async fn events_processed(
    client: Arc<SubscriptionClient>,
    state: Arc<WidgetsState>,
    interval: i64,
) -> Result<(), ()> {
    let events = client
        .component_events_processed_total_subscription(interval)
        .await
        .map_err(|_| ())?;

    tokio::pin! {
        let stream = events.stream();
    };

    loop {
        if let Some(Some(res)) = stream.next().await {
            if let Some(d) = res.data {
                let c = d.component_events_processed_total;
                state.update_component_events_processed_total(
                    &c.name,
                    c.metric.events_processed_total as i64,
                );
            }
        }
    }

    Ok(())
}

pub fn subscribe(client: SubscriptionClient, state: Arc<WidgetsState>, interval: i64) {
    let client = Arc::new(client);

    for metric_fn in [events_processed].iter() {
        let client = Arc::clone(&client);
        let state = Arc::clone(&state);
        let interval = interval;

        tokio::spawn(async move {
            let _ = metric_fn(client, state, interval).await;
        });
    }
}

/// Retrieve the initial components/metrics for first paint. Further updating the metrics
/// will be handled by subscriptions.
pub async fn init_components(client: &Client) -> Result<ComponentsState, ()> {
    // Execute a query to get the latest components, and aggregate metrics for each resource
    let rows = client
        .components_query()
        .await
        .map_err(|_| ())?
        .data
        .ok_or_else(|| ())?
        .components
        .into_iter()
        .map(|d| ComponentRow {
            name: d.name,
            component_type: d.on.to_string(),
            events_processed_total: d
                .events_processed_total
                .as_ref()
                .map(|ep| ep.events_processed_total as i64)
                .unwrap_or(0),
            errors: 0,
            throughput: 0.00,
        })
        .collect::<Vec<_>>();

    Ok(ComponentsState::from_rows(rows))
}

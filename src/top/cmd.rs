use super::{
    dashboard::{init_dashboard, is_tty, Widgets},
    state::{ComponentRow, ComponentsState, WidgetsState},
};
use crate::config;
use std::sync::Arc;
use tokio::stream::StreamExt;
use url::Url;
use vector_api_client::{
    connect_subscription_client,
    gql::{ComponentsQueryExt, HealthQueryExt, MetricsSubscriptionExt},
    Client, SubscriptionClient,
};

/// Retrieve the initial components/metrics for first paint. Further updating the metrics
/// will be handled by subscriptions.
async fn init_components(client: &Client) -> Result<ComponentsState, ()> {
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

/// Subscribe to metrics updates, for patching widget state
async fn subscribe_metrics(
    client: Arc<SubscriptionClient>,
    state: Arc<WidgetsState>,
    interval: i64,
) -> Result<(), ()> {
    // Events processed
    let mut events = client
        .component_events_processed_total_subscription(interval)
        .await
        .map_err(|_| ())?
        .stream();

    tokio::spawn(async move { while let Some(res) = events.next().await {} });

    Ok(())
}

/// CLI command func for displaying Vector components, and communicating with a local/remote
/// Vector API server via HTTP/WebSockets
pub async fn cmd(opts: &super::Opts) -> exitcode::ExitCode {
    // Exit early if the terminal is not a teletype
    if !is_tty() {
        eprintln!("Terminal must be a teletype (TTY) to display a Vector dashboard.");
        return exitcode::IOERR;
    }

    // Use the provided URL as the Vector GraphQL API server, or default to the local port
    // provided by the API config. This will work despite `api` and `api-client` being distinct
    // features; the config is available even if `api` is disabled
    let url = opts.url.clone().unwrap_or_else(|| {
        let addr = config::api::default_bind().unwrap();
        Url::parse(&*format!("http://{}/graphql", addr))
            .expect("Couldn't parse default API URL. Please report this.")
    });

    // Create a new API client for connecting to the local/remote Vector instance
    let client = Client::new(url.clone());

    // Check that the GraphQL server is reachable
    match client.health_query().await {
        Ok(_) => (),
        _ => {
            eprintln!("Vector API server not reachable");
            return exitcode::UNAVAILABLE;
        }
    }

    // Get the initial component state
    let component_state = match init_components(&client).await {
        Ok(component_state) => component_state,
        _ => {
            eprintln!("Couldn't query Vector components");
            return exitcode::UNAVAILABLE;
        }
    };

    // Create a new subscription client
    let subscription_client = match connect_subscription_client(&url).await {
        Ok(c) => Arc::new(c),
        _ => {
            eprintln!("Couldn't connect to Vector API via WebSockets");
            return exitcode::UNAVAILABLE;
        }
    };

    // Create initial topology, to be shared by the API client and dashboard renderer
    let state = Arc::new(WidgetsState::new(url, component_state));

    // Subscribe to metrics updates
    if let Err(_) = subscribe_metrics(
        Arc::clone(&subscription_client),
        Arc::clone(&state),
        opts.refresh_interval as i64,
    )
    .await
    {
        eprintln!("Couldn't subscribe to Vector metrics");
        return exitcode::UNAVAILABLE;
    }

    // Render a dashboard with the configured widgets
    let widgets = Widgets::new(Arc::clone(&state));

    match init_dashboard(&widgets).await {
        Ok(_) => exitcode::OK,
        _ => {
            eprintln!("Your terminal doesn't support building a dashboard. Exiting.");
            exitcode::IOERR
        }
    }
}

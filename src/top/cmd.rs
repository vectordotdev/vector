use super::{
    dashboard::{init_dashboard, Widgets},
    state::{TopologyRow, TopologyState, WidgetsState},
};
use crate::config;
use std::sync::Arc;
use url::Url;
use vector_api_client::{
    gql::{HealthQueryExt, TopologyQueryExt},
    Client,
};

/// Executes a toplogy query to the GraphQL server, and creates an initial TopologyState
/// table based on the returned topology/metrics. This will contain all of the rows initially
/// to render the topology table widget
async fn update_topology(
    interval: u64,
    client: Client,
    state: Arc<WidgetsState>,
) -> Result<(), ()> {
    // Loop every `interval` ms to update topology
    let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(interval));

    loop {
        interval.tick().await;

        // Execute a query to get the latest topology, and aggregate metrics for each resource
        let rows = client
            .topology_query()
            .await
            .map_err(|_| ())?
            .data
            .ok_or_else(|| ())?
            .topology
            .into_iter()
            .map(|d| TopologyRow {
                name: d.name,
                topology_type: d.on.to_string(),
                events_processed_total: d
                    .events_processed_total
                    .as_ref()
                    .map(|ep| ep.events_processed_total as i64)
                    .unwrap_or(0),
                errors: 0,
                throughput: 0.00,
            })
            .collect::<Vec<_>>();

        state.update_topology_rows(rows);
    }
}

/// CLI command func for displaying Vector topology, and communicating with a local/remote
/// Vector API server via HTTP/WebSockets
pub async fn cmd(opts: &super::Opts) -> exitcode::ExitCode {
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

    // Create initial topology, to be shared by the API client and dashboard renderer
    let state = Arc::new(WidgetsState::new(url, TopologyState::new()));

    // Update dashboard based on the provided refresh interval
    tokio::spawn(update_topology(
        opts.refresh_interval,
        client,
        Arc::clone(&state),
    ));

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

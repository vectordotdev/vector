use super::{
    dashboard::{init_dashboard, is_tty, Widgets},
    state::{ComponentRow, ComponentsState, WidgetsState},
};
use crate::config;
use std::sync::Arc;
use url::Url;
use vector_api_client::{
    gql::{ComponentsQueryExt, HealthQueryExt},
    Client,
};

/// Executes a toplogy query to the GraphQL server, and creates an initial ComponentsState
/// table based on the returned components/metrics. This will contain all of the rows initially
/// to render the components table widget
async fn update_components(
    interval: u64,
    client: Client,
    state: Arc<WidgetsState>,
) -> Result<(), ()> {
    // Loop every `interval` ms to update components
    let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(interval));

    loop {
        interval.tick().await;

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

        state.update_component_rows(rows);
    }
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

    // Create initial topology, to be shared by the API client and dashboard renderer
    let state = Arc::new(WidgetsState::new(url, ComponentsState::new()));

    // Update dashboard based on the provided refresh interval
    tokio::spawn(update_components(
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

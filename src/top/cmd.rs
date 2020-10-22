use super::{
    dashboard::{Dashboard, Widgets},
    state::{TopologyRow, TopologyState, WidgetsState},
};
use crate::config;
use arc_swap::ArcSwap;
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
    topology_state: ArcSwap<TopologyState>,
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
            .collect();

        // Swap the
        topology_state.swap(topology_state.load().with_swapped_rows(rows));
    }

    unreachable!("not possible")
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

    // Create initial topology; spawn updater
    let topology_state = TopologyState::arc_new();
    tokio::spawn(update_topology(
        opts.refresh_interval,
        client,
        ArcSwap::clone(&topology_state),
    ));

    // Spawn a new dashboard with the configured widgets
    let state = WidgetsState::new(url, ArcSwap::clone(&topology_state));
    let widgets = Widgets::new(state);

    Dashboard::new().run(&widgets);

    exitcode::OK
}

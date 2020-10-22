use super::{
    dashboard::{Config, Dashboard, Widgets},
    events::{Event, Events},
    state::{TopologyRow, TopologyState},
};
use crate::config;
use arc_swap::ArcSwap;
use std::{error::Error, io, sync::Arc};
use url::Url;
use vector_api_client::{
    gql::{HealthQueryExt, TopologyQueryExt},
    Client, SubscriptionClient,
};

/// Executes a toplogy query to the GraphQL server, and creates an initial TopologyState
/// table based on the returned topology/metrics. This will contain all of the rows initially
/// to render the topology table widget
async fn spawn_update_topology(
    client: &Client,
    topology_state: Arc<TopologyState>,
) -> Result<(), ()> {
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

    topology_state.update_rows(rows);

    Ok(())
}

/// Spawns the host
async fn spawn_host_metrics(client: &SubscriptionClient) {}

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
    let topology_state = Arc::new(TopologyState::new());
    spawn_update_topology(&client, Arc::clone(&topology_state));

    // tokio::spawn(async move {
    //     use rand::Rng;
    //
    //     let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(50));
    //     loop {
    //         interval.tick().await;
    //
    //         let mut rng = rand::thread_rng();
    //
    //         cloned.load().rows().for_each(|r| {
    //             let mut r = r.lock().unwrap();
    //             let events_processed_total = r.events_processed_total;
    //             r.update_events_processed_total(
    //                 events_processed_total + rng.gen_range::<i64>(0, 50),
    //             );
    //         });
    //     }
    // });

    // Configure widgets, based on the user CLI options
    let config = Config {
        url,
        topology_state: Arc::clone(&topology_state),
    };

    // Spawn a new dashboard with the configured widgets
    let widgets = Widgets::new(&config);
    Dashboard::new().run(&widgets);

    exitcode::OK
}

use super::{
    dashboard::{init_dashboard, is_tty, Widgets},
    metrics,
    state::WidgetsState,
};
use crate::config;
use std::sync::Arc;
use url::Url;
use vector_api_client::{connect_subscription_client, gql::HealthQueryExt, Client};

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
    let component_state = match metrics::init_components(&client).await {
        Ok(component_state) => component_state,
        _ => {
            eprintln!("Couldn't query Vector components");
            return exitcode::UNAVAILABLE;
        }
    };

    // Change the HTTP schema to WebSockets
    let mut ws_url = url.clone();
    ws_url
        .set_scheme(match url.scheme() {
            "https" => "wss",
            _ => "ws",
        })
        .expect("Couldn't build WebSocket URL. Please report.");

    let subscription_client = match connect_subscription_client(&ws_url).await {
        Ok(c) => c,
        _ => {
            eprintln!("Couldn't connect to Vector API via WebSockets");
            return exitcode::UNAVAILABLE;
        }
    };

    // Create initial topology, to be shared by the API client and dashboard renderer
    let state = Arc::new(WidgetsState::new(url, component_state));

    // Subscribe to updated metrics
    metrics::subscribe(
        subscription_client,
        Arc::clone(&state),
        opts.refresh_interval as i64,
    );

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

use url::Url;
use vector_api_client::{connect_subscription_client, Client};

use super::{
    dashboard::{init_dashboard, is_tty},
    metrics, state,
};
use crate::config;

/// CLI command func for displaying Vector components, and communicating with a local/remote
/// Vector API server via HTTP/WebSockets
pub async fn cmd(opts: &super::Opts) -> exitcode::ExitCode {
    // Exit early if the terminal is not a teletype
    if !is_tty() {
        #[allow(clippy::print_stderr)]
        {
            eprintln!("Terminal must be a teletype (TTY) to display a Vector dashboard.");
        }
        return exitcode::IOERR;
    }

    // Use the provided URL as the Vector GraphQL API server, or default to the local port
    // provided by the API config. This will work despite `api` and `api-client` being distinct
    // features; the config is available even if `api` is disabled
    let url = opts.url.clone().unwrap_or_else(|| {
        let addr = config::api::default_address().unwrap();
        Url::parse(&*format!("http://{}/graphql", addr))
            .expect("Couldn't parse default API URL. Please report this.")
    });

    // Create a new API client for connecting to the local/remote Vector instance.
    let client = match Client::new_with_healthcheck(url.clone()).await {
        Some(client) => client,
        None => return exitcode::UNAVAILABLE,
    };

    // Create a metrics state updater
    let (tx, rx) = tokio::sync::mpsc::channel(20);

    // Get the initial component state
    let sender = match metrics::init_components(&client).await {
        Ok(state) => state::updater(state, rx).await,
        _ => {
            #[allow(clippy::print_stderr)]
            {
                eprintln!("Couldn't query Vector components.");
            }
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

    let subscription_client = match connect_subscription_client(ws_url).await {
        Ok(c) => c,
        Err(e) => {
            #[allow(clippy::print_stderr)]
            {
                eprintln!("Couldn't connect to Vector API via WebSockets: {:?}", e);
            }
            return exitcode::UNAVAILABLE;
        }
    };

    // Subscribe to updated metrics
    metrics::subscribe(subscription_client, tx.clone(), opts.interval as i64);

    // Initialize the dashboard
    match init_dashboard(url.as_str(), opts, sender).await {
        Ok(_) => exitcode::OK,
        _ => {
            #[allow(clippy::print_stderr)]
            {
                eprintln!("Your terminal doesn't support building a dashboard. Exiting.");
            }
            exitcode::IOERR
        }
    }
}

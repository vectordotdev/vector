use std::collections::BTreeMap;
use std::time::Duration;

use chrono::Local;
use futures_util::future::join_all;
use http::Uri;
use regex::Regex;
use tokio::sync::{mpsc, oneshot};
use vector_lib::api_client::{Client, RECONNECT_DELAY_MS};

use vector_lib::top::{
    dashboard::{init_dashboard, is_tty},
    metrics,
    state::{self, ConnectionStatus, EventType, State},
};

/// CLI command func for displaying Vector components, and communicating with a local/remote
/// Vector API server via gRPC
#[allow(clippy::print_stderr)]
pub async fn cmd(opts: &super::Opts) -> exitcode::ExitCode {
    // Exit early if the terminal is not a teletype
    if !is_tty() {
        eprintln!("Terminal must be a teletype (TTY) to display a Vector dashboard.");
        return exitcode::IOERR;
    }

    let url = opts.url();

    // Create a new API client for connecting to the local/remote Vector instance.
    let Ok(uri) = url.as_str().parse::<Uri>() else {
        eprintln!("Invalid API URL: {url}");
        return exitcode::USAGE;
    };
    let mut client = Client::new(uri.clone());

    if client.connect().await.is_err() || client.health().await.is_err() {
        eprintln!(
            indoc::indoc! {"
            Vector API server isn't reachable ({}).

            Have you enabled the API?

            To enable the API, add the following to your Vector config file:

            [api]
                enabled = true"},
            url
        );
        return exitcode::UNAVAILABLE;
    }

    top(opts, uri, "Vector").await
}

/// General monitoring
pub async fn top(opts: &super::Opts, uri: Uri, dashboard_title: &str) -> exitcode::ExitCode {
    // Channel for updating state via event messages
    let (tx, rx) = tokio::sync::mpsc::channel(20);
    let mut starting_state = State::new(BTreeMap::new());
    starting_state.sort_state.column = opts.sort_field;
    starting_state.sort_state.reverse = opts.sort_desc;
    starting_state.filter_state.column = opts.filter_field;
    starting_state.filter_state.pattern = opts
        .filter_value
        .as_deref()
        .map(Regex::new)
        .and_then(Result::ok);
    let state_rx = state::updater(rx, starting_state).await;
    // Channel for shutdown signal
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let connection = tokio::spawn(subscription(opts.clone(), uri, tx.clone(), shutdown_tx));

    // Initialize the dashboard
    match init_dashboard(
        dashboard_title,
        opts.url().as_str(),
        opts.interval,
        opts.human_metrics,
        tx,
        state_rx,
        shutdown_rx,
    )
    .await
    {
        Ok(_) => {
            connection.abort();
            exitcode::OK
        }
        Err(err) => {
            #[allow(clippy::print_stderr)]
            {
                eprintln!("[top] Encountered shutdown error: {err}");
            }
            connection.abort();
            exitcode::IOERR
        }
    }
}

// This task handles reconnecting the gRPC client and all
// subscriptions in the case of a connection failure
async fn subscription(
    opts: super::Opts,
    uri: Uri,
    tx: mpsc::Sender<EventType>,
    shutdown_tx: oneshot::Sender<()>,
) {
    loop {
        // Initialize state. On future reconnects, we re-initialize state in
        // order to accurately capture added, removed, and edited
        // components.
        let state = match metrics::init_components(uri.clone(), &opts.components).await {
            Ok(state) => state,
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(RECONNECT_DELAY_MS)).await;
                continue;
            }
        };
        let initial_components = state
            .components
            .keys()
            .map(|k| k.id().to_string())
            .collect();
        _ = tx.send(EventType::InitializeState(state)).await;

        // Subscribe to updated metrics via gRPC streaming
        let handles = match metrics::subscribe(
            uri.clone(),
            tx.clone(),
            opts.interval as i64,
            opts.components.clone(),
            initial_components,
        )
        .await
        {
            Ok(handles) => handles,
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(RECONNECT_DELAY_MS)).await;
                continue;
            }
        };

        _ = tx
            .send(EventType::ConnectionUpdated(ConnectionStatus::Connected(
                Local::now(),
            )))
            .await;

        // Wait for metric stream tasks to finish. poll_components is intentionally
        // excluded: it runs indefinitely while get_components succeeds, so joining
        // it here would prevent reconnection when metric streams fail first.
        _ = join_all(handles.metric_handles).await;
        handles.poll_handle.abort();

        _ = tx
            .send(EventType::ConnectionUpdated(
                ConnectionStatus::Disconnected(RECONNECT_DELAY_MS),
            ))
            .await;

        if opts.no_reconnect {
            _ = shutdown_tx.send(());
            break;
        }
    }
}

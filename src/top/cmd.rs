use std::time::Duration;

use chrono::Local;
use futures_util::future::join_all;
use tokio::sync::{mpsc, oneshot};
use vector_lib::api_client::{connect_subscription_client, Client};

use super::{
    dashboard::{init_dashboard, is_tty},
    metrics,
    state::{self, ConnectionStatus, EventType},
};

/// Delay (in milliseconds) before attempting to reconnect to the Vector API
const RECONNECT_DELAY: u64 = 5000;

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

    let url = opts.url();
    // Create a new API client for connecting to the local/remote Vector instance.
    let client = Client::new(url.clone());
    #[allow(clippy::print_stderr)]
    if client.healthcheck().await.is_err() {
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

    top(opts, client, "Vector").await
}

/// General monitoring
pub async fn top(opts: &super::Opts, client: Client, dashboard_title: &str) -> exitcode::ExitCode {
    // Channel for updating state via event messages
    let (tx, rx) = tokio::sync::mpsc::channel(20);
    let state_rx = state::updater(rx).await;
    // Channel for shutdown signal
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let connection = tokio::spawn(subscription(opts.clone(), client, tx, shutdown_tx));

    // Initialize the dashboard
    match init_dashboard(
        dashboard_title,
        opts.url().as_str(),
        opts,
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
                eprintln!("[top] Encountered shutdown error: {}", err);
            }
            connection.abort();
            exitcode::IOERR
        }
    }
}

// This task handles reconnecting the subscription client and all
// subscriptions in the case of a web socket disconnect
async fn subscription(
    opts: super::Opts,
    client: Client,
    tx: mpsc::Sender<EventType>,
    shutdown_tx: oneshot::Sender<()>,
) {
    let ws_url = opts.web_socket_url();

    loop {
        // Initialize state. On future reconnects, we re-initialize state in
        // order to accurately capture added, removed, and edited
        // components.
        let state = match metrics::init_components(&client).await {
            Ok(state) => state,
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(RECONNECT_DELAY)).await;
                continue;
            }
        };
        _ = tx.send(EventType::InitializeState(state)).await;

        let subscription_client = match connect_subscription_client(ws_url.clone()).await {
            Ok(c) => c,
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(RECONNECT_DELAY)).await;
                continue;
            }
        };

        // Subscribe to updated metrics
        let finished = metrics::subscribe(subscription_client, tx.clone(), opts.interval as i64);

        _ = tx
            .send(EventType::ConnectionUpdated(ConnectionStatus::Connected(
                Local::now(),
            )))
            .await;
        // Tasks spawned in metrics::subscribe finish when the subscription
        // streams have completed. Currently, subscription streams only
        // complete when the underlying web socket connection to the GraphQL
        // server drops.
        _ = join_all(finished).await;
        _ = tx
            .send(EventType::ConnectionUpdated(
                ConnectionStatus::Disconnected(RECONNECT_DELAY),
            ))
            .await;
        if opts.no_reconnect {
            _ = shutdown_tx.send(());
            break;
        }
    }
}

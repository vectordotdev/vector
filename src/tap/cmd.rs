use std::{time::Duration};
use vector_lib::api_client::Client;
use vector_lib::tap::{exec_tap, EventFormatter, TapExecutorError};

use crate::signal::{SignalRx, SignalTo};

/// Delay (in milliseconds) before attempting to reconnect to the Vector API
const RECONNECT_DELAY: u64 = 5000;

/// CLI command func for issuing 'tap' queries, and communicating with a local/remote
/// Vector API server via HTTP/WebSockets.
pub(crate) async fn cmd(opts: &super::Opts, signal_rx: SignalRx) -> exitcode::ExitCode {
    let url = opts.url();
    // Return early with instructions for enabling the API if the endpoint isn't reachable
    // via a healthcheck.
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

    tap(opts, signal_rx).await
}

/// Observe event flow from specified components
pub async fn tap(opts: &super::Opts, mut signal_rx: SignalRx) -> exitcode::ExitCode {
    let subscription_url = opts.web_socket_url();
    let formatter = EventFormatter::new(opts.meta, opts.format);
    let outputs_patterns = opts.outputs_patterns();

    loop {
        tokio::select! {
            biased;
            Ok(SignalTo::Shutdown(_) | SignalTo::Quit) = signal_rx.recv() => break,
            exec_result = exec_tap(
                &subscription_url,
                opts.interval as i64,
                opts.limit as i64,
                opts.duration_ms,
                opts.inputs_of.clone(),
                outputs_patterns.clone(),
                opts.format,
                &formatter,
                opts.quiet,
            ) => {
                match exec_result {
                    Ok(_) => {
                        break;
                    }
                    Err(TapExecutorError::ConnectionFailure) | Err(TapExecutorError::GraphQLError) => {
                        if !opts.no_reconnect {
                            #[allow(clippy::print_stderr)]
                            {
                                eprintln!("[tap] Connection failed. Reconnecting in {:?} seconds.", RECONNECT_DELAY / 1000);
                            }
                            tokio::time::sleep(Duration::from_millis(RECONNECT_DELAY)).await;
                        }
                        else {
                            break;
                        }
                    }
                }
            }
        }
    }

    exitcode::OK
}

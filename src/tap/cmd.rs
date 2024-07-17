use std::time::Duration;

use vector_lib::api_client::Client;
use vector_lib::tap::{EventFormatter, OutputChannel, TapRunner};

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
    let output_channel = OutputChannel::Stdout(EventFormatter::new(opts.meta, opts.format));
    let tap_runner = TapRunner::new(
        &subscription_url,
        opts.inputs_of.clone(),
        opts.outputs_patterns().clone(),
        &output_channel,
        opts.format,
    );

    loop {
        tokio::select! {
            biased;
            Ok(SignalTo::Shutdown(_) | SignalTo::Quit) = signal_rx.recv() => break,
            exec_result = tap_runner.run_tap(
                opts.interval as i64,
                opts.limit as i64,
                opts.duration_ms,
                opts.quiet,
            ) => {
                match exec_result {
                    Ok(_) => {
                        break;
                    }
                    Err(tap_executor_error) => {
                        if !opts.no_reconnect {
                            #[allow(clippy::print_stderr)]
                            {
                                eprintln!(
                                    "[tap] Connection failed with error {:?}. Reconnecting in {:?} seconds.",
                                    tap_executor_error,
                                    RECONNECT_DELAY / 1000);
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

use std::time::Duration;

use vector_lib::{
    api_client::{Client, RECONNECT_DELAY_MS},
    tap::{EventFormatter, OutputChannel, TapRunner},
};

use crate::signal::{SignalRx, SignalTo};

/// CLI command func for issuing 'tap' queries, and communicating with a local/remote
/// Vector API server via HTTP/WebSockets.
#[allow(clippy::print_stderr)]
pub(crate) async fn cmd(opts: &super::Opts, signal_rx: SignalRx) -> exitcode::ExitCode {
    let url = opts.url();
    let Ok(uri) = url.as_str().parse() else {
        eprintln!("Invalid API URL: {url}");
        return exitcode::USAGE;
    };
    let mut client = Client::new(uri);

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

    tap_internal(opts, signal_rx, Some(client)).await
}

/// Observe event flow from specified components
pub async fn tap(opts: &super::Opts, signal_rx: SignalRx) -> exitcode::ExitCode {
    tap_internal(opts, signal_rx, None).await
}

async fn tap_internal(
    opts: &super::Opts,
    mut signal_rx: SignalRx,
    mut client_opt: Option<Client>,
) -> exitcode::ExitCode {
    let url = opts.url();
    let output_channel = OutputChannel::Stdout(EventFormatter::new(opts.meta, opts.format));
    let tap_runner = TapRunner::new(
        &url,
        opts.inputs_of.clone(),
        opts.outputs_patterns().clone(),
        &output_channel,
    );

    loop {
        tokio::select! {
            biased;
            Ok(SignalTo::Shutdown(_) | SignalTo::Quit) = signal_rx.recv() => break,
            exec_result = async {
                if let Some(client) = client_opt.take() {
                    tap_runner.run_tap_with_client(
                        client,
                        opts.interval as i64,
                        opts.limit as i64,
                        opts.duration_ms,
                        opts.quiet,
                    ).await
                } else {
                    tap_runner.run_tap(
                        opts.interval as i64,
                        opts.limit as i64,
                        opts.duration_ms,
                        opts.quiet,
                    ).await
                }
            } => {
                match exec_result {
                    Ok(_) => {
                        break;
                    }
                    Err(tap_executor_error) => {
                        #[allow(clippy::print_stderr)]
                        if tap_executor_error.is_fatal() {
                            eprintln!("[tap] Error: {tap_executor_error:?}");
                            break;
                        } else if !opts.no_reconnect {
                            eprintln!(
                                "[tap] Connection failed with error {:?}. Reconnecting in {:?} seconds.",
                                tap_executor_error,
                                RECONNECT_DELAY_MS / 1000);
                            tokio::time::sleep(Duration::from_millis(RECONNECT_DELAY_MS)).await;
                        } else {
                            break;
                        }
                    }
                }
            }
        }
    }

    exitcode::OK
}

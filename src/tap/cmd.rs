use tokio_stream::StreamExt;
use url::Url;
use vector_api_client::{connect_subscription_client, gql::TapSubscriptionExt, Client};

use crate::{
    config,
    signal::{SignalRx, SignalTo},
};

/// CLI command func for issuing 'tap' queries, and communicating with a local/remote
/// Vector API server via HTTP/WebSockets.
pub async fn cmd(opts: &super::Opts, mut signal_rx: SignalRx) -> exitcode::ExitCode {
    // Use the provided URL as the Vector GraphQL API server, or default to the local port
    // provided by the API config. This will work despite `api` and `api-client` being distinct
    // features; the config is available even if `api` is disabled.
    let mut url = opts.url.clone().unwrap_or_else(|| {
        let addr = config::api::default_address().unwrap();
        Url::parse(&*format!("http://{}/graphql", addr))
            .expect("Couldn't parse default API URL. Please report this.")
    });

    // Return early with instructions for enabling the API if the endpoint isn't reachable
    // via a healthcheck.
    if Client::new_with_healthcheck(url.clone()).await.is_none() {
        return exitcode::UNAVAILABLE;
    }

    // Change the HTTP schema to WebSockets.
    url.set_scheme(match url.scheme() {
        "https" => "wss",
        _ => "ws",
    })
    .expect("Couldn't build WebSocket URL. Please report.");

    let subscription_client = match connect_subscription_client(url).await {
        Ok(c) => c,
        Err(e) => {
            #[allow(clippy::print_stderr)]
            {
                eprintln!("Couldn't connect to Vector API via WebSockets: {:?}", e);
            }
            return exitcode::UNAVAILABLE;
        }
    };

    // Issue the 'tap' request, printing to stdout.
    let res = subscription_client.output_events_by_component_id_patterns_subscription(
        opts.component_id_patterns.clone(),
        opts.format,
        opts.limit as i64,
        opts.interval as i64,
    );

    tokio::pin! {
        let stream = res.stream();
    };

    // Loop over the returned results, printing out log events.
    // NOTE: This will currently ignore notifications. A later `--verbose` option is planned
    // to include these.
    // TODO: https://github.com/timberio/vector/issues/6870
    loop {
        tokio::select! {
            biased;
            Some(SignalTo::Shutdown | SignalTo::Quit) = signal_rx.recv() => break,
            Some(Some(res)) = stream.next() => {
                if let Some(d) = res.data {
                    for log_event in d.output_events_by_component_id_patterns.iter().filter_map(|ev| ev.as_log()) {
                        #[allow(clippy::print_stdout)]
                        {
                            println!("{}", log_event.string);
                        }
                    }
                }
            }
        }
    }

    exitcode::OK
}

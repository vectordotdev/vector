use crate::config;
use url::Url;
use vector_api_client::{connect_subscription_client, gql::TapSubscriptionExt, Client};

/// CLI command func for issuing 'tap' queries, and communicating with a local/remote
/// Vector API server via HTTP/WebSockets.
pub async fn cmd(opts: &super::Opts) -> exitcode::ExitCode {
    // Use the provided URL as the Vector GraphQL API server, or default to the local port
    // provided by the API config. This will work despite `api` and `api-client` being distinct
    // features; the config is available even if `api` is disabled.
    let url = opts.url.clone().unwrap_or_else(|| {
        let addr = config::api::default_address().unwrap();
        Url::parse(&*format!("http://{}/graphql", addr))
            .expect("Couldn't parse default API URL. Please report this.")
    });

    // Create a new API client for connecting to the local/remote Vector instance.
    let client = match Client::new_with_healthcheck(url).await {
        Some(client) => client,
        None => return exitcode::UNAVAILABLE,
    };

    // Issue the tap subscription, printing log lines to stdout.

    exitcode::OK
}

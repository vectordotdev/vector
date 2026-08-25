use std::net::{SocketAddr, ToSocketAddrs as _};

use tokio::time::Duration;

use super::*;
use crate::test_util::components::{HTTP_PUSH_SOURCE_TAGS, run_and_assert_source_compliance};

fn source_receive_address() -> SocketAddr {
    let address = std::env::var("REMOTE_WRITE_SOURCE_RECEIVE_ADDRESS")
        .unwrap_or_else(|_| "127.0.0.1:9102".into());
    // TODO: This logic should maybe be moved up into the source, and possibly into other
    // sources, wrapped in a new socket address type that does the lookup during config parsing.
    address
        .to_socket_addrs()
        .unwrap()
        .next()
        .unwrap_or_else(|| panic!("Socket address {address:?} did not resolve"))
}

#[tokio::test]
async fn receive_something() {
    // TODO: This test depends on the single instance of Prometheus that we spin up for
    // integration tests both scraping an endpoint and then also remote writing that stuff to
    // this remote write source.  This makes sense from a "test the actual behavior" standpoint
    // but it feels a little fragile.
    //
    // It could be nice to split up the Prometheus integration tests in the future, or
    // maybe there's a way to do a one-shot remote write from Prometheus? Not sure.
    let config = PrometheusRemoteWriteConfig {
        address: source_receive_address(),
        path: default_path(),
        auth: None,
        tls: None,
        metadata_conflict_strategy: Default::default(),
        acknowledgements: SourceAcknowledgementsConfig::default(),
        keepalive: KeepaliveConfig::default(),
        skip_nan_values: false,
    };

    let events =
        run_and_assert_source_compliance(config, Duration::from_secs(5), &HTTP_PUSH_SOURCE_TAGS)
            .await;
    assert!(!events.is_empty());
}

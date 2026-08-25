use std::time::Duration;

use futures::Stream;
use tokio::time::timeout;
use vrl::event_path;

use super::*;
use crate::{
    SourceSender,
    config::SourceContext,
    event::EventStatus,
    test_util::{
        collect_n,
        components::{SOCKET_PUSH_SOURCE_TAGS, assert_source_compliance},
        wait_for_tcp,
    },
    tls::{TlsConfig, TlsEnableableConfig},
};

fn heartbeat_address() -> String {
    std::env::var("HEARTBEAT_ADDRESS")
        .expect("Address of Beats Heartbeat service must be specified.")
}

#[tokio::test]
async fn beats_heartbeat() {
    let events = assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
        let out = source(heartbeat_address(), None).await;

        timeout(Duration::from_secs(60), collect_n(out, 1))
            .await
            .unwrap()
    })
    .await;

    assert!(!events.is_empty());

    let log = events[0].as_log();
    assert_eq!(
        log.get(event_path!("@metadata", "beat")),
        Some(String::from("heartbeat").into()).as_ref()
    );
    assert_eq!(
        log.get(event_path!("summary", "up")),
        Some(1.into()).as_ref()
    );
    assert!(log.get(event_path!("timestamp")).is_some());
    assert!(log.get(event_path!("host")).is_some());
}

fn logstash_address() -> String {
    std::env::var("LOGSTASH_ADDRESS")
        .expect("Listen address for `logstash` source must be specified.")
}

#[tokio::test]
async fn logstash() {
    let events = assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async {
        let out = source(
            logstash_address(),
            Some(TlsEnableableConfig {
                enabled: Some(true),
                options: TlsConfig {
                    crt_file: Some("tests/integration/shared/data/host.docker.internal.crt".into()),
                    key_file: Some("tests/integration/shared/data/host.docker.internal.key".into()),
                    ..Default::default()
                },
            }),
        )
        .await;

        timeout(Duration::from_secs(60), collect_n(out, 1))
            .await
            .unwrap()
    })
    .await;

    assert!(!events.is_empty());

    let log = events[0].as_log();
    assert!(
        log.get(event_path!("line"))
            .unwrap()
            .to_string_lossy()
            .contains("Hello World")
    );
    assert!(log.get(event_path!("host")).is_some());
}

async fn source(
    address: String,
    tls: Option<TlsEnableableConfig>,
) -> impl Stream<Item = Event> + Unpin {
    let (sender, recv) = SourceSender::new_test_finalize(EventStatus::Delivered);
    let address: SocketAddr = address.parse().unwrap();
    let tls_config = TlsSourceConfig {
        client_metadata_key: None,
        tls_config: tls.unwrap_or_default(),
    };
    tokio::spawn(async move {
        LogstashConfig {
            address: address.into(),
            tls: Some(tls_config),
            keepalive: None,
            permit_origin: None,
            receive_buffer_bytes: None,
            acknowledgements: false.into(),
            connection_limit: None,
            tls_handshake_timeout_secs: None,
            log_namespace: None,
        }
        .build(SourceContext::new_test(sender, None))
        .await
        .unwrap()
        .await
        .unwrap()
    });
    wait_for_tcp(address).await;
    recv
}

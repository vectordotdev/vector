use std::io::Write;

use flate2::{Compression, write::GzEncoder};
use futures::StreamExt;
use tokio::time::{Duration, timeout};

use crate::{
    config::{Config, log_schema},
    event::into_event_stream,
    sources::http_server::SimpleHttpConfig,
    test_util::{
        addr::next_addr, metrics::counter_value_with_tags, mock::basic_sink, start_topology,
        trace_init, wait_for_tcp,
    },
};

#[cfg(feature = "sources-socket")]
#[tokio::test]
async fn udp_network_bytes_include_rejected_payloads_with_source_component_labels() {
    trace_init();
    let (_guard, address) = next_addr();
    let component = "network_udp_source";
    let (events, sink) = basic_sink(10);
    let mut events = Box::pin(events.flat_map(into_event_stream));
    let mut config = Config::builder();
    config.add_source(
        component,
        serde_yaml::from_str::<crate::sources::socket::SocketConfig>(&format!(
            "mode: udp\naddress: \"{address}\"\ndecoding:\n  codec: json\n"
        ))
        .unwrap(),
    );
    config.add_sink("network_udp_output", &[component], sink);
    let (topology, _) = start_topology(config.build().unwrap(), false).await;
    // UDP has no connection handshake. Wait for the source to own its port before sending.
    timeout(Duration::from_secs(10), async {
        loop {
            match std::net::UdpSocket::bind(address) {
                Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => break,
                Err(error) => panic!("failed probing UDP source readiness: {error}"),
                Ok(socket) => drop(socket),
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("UDP source did not bind");

    let sender = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let tags = &[
        ("component_id", component),
        ("component_kind", "source"),
        ("component_type", "socket"),
        ("protocol", "udp"),
    ];
    let mut total = 0_u32;
    for payload in [b"not json".as_slice(), br#"{"message":"accepted"}"#] {
        sender.send_to(payload, address).await.unwrap();
        total += u32::try_from(payload.len()).unwrap();
        // Check each datagram separately, including the one rejected by the JSON decoder.
        timeout(Duration::from_secs(10), async {
            while counter_value_with_tags("component_received_network_bytes_total", tags).to_bits()
                != f64::from(total).to_bits()
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("UDP network counter did not include the received datagram");
    }
    let event = timeout(Duration::from_secs(10), events.next())
        .await
        .expect("UDP source did not emit the valid event")
        .expect("UDP source output closed");
    assert_eq!(
        event
            .as_log()
            .get(log_schema().message_key_target_path().unwrap())
            .unwrap()
            .to_string_lossy(),
        "accepted",
    );
    assert_eq!(
        counter_value_with_tags("component_received_bytes_total", tags).to_bits(),
        f64::from(total).to_bits(),
        "serialized-byte accounting must retain its existing behavior",
    );
    timeout(Duration::from_secs(10), topology.stop())
        .await
        .expect("UDP source topology did not shut down");
}

// The test metrics recorder is thread-local; spawned server tasks must use the same runtime thread.
#[tokio::test]
async fn http_network_bytes_retain_source_component_labels_across_server_tasks() {
    trace_init();
    let (_first_guard, first_addr) = next_addr();
    let (_second_guard, second_addr) = next_addr();
    let sources = [
        ("network_http_source_first", first_addr, "first".repeat(256)),
        (
            "network_http_source_second",
            second_addr,
            "second".repeat(256),
        ),
    ];
    let (events, sink) = basic_sink(10);
    let mut events = Box::pin(events.flat_map(into_event_stream));
    let mut config = Config::builder();
    for (id, address, _) in &sources {
        config.add_source(
            *id,
            serde_yaml::from_str::<SimpleHttpConfig>(&format!("address: \"{address}\"")).unwrap(),
        );
    }
    config.add_sink(
        "network_http_source_output",
        &["network_http_source_first", "network_http_source_second"],
        sink,
    );
    // Let the real topology create the component spans. Neither this test nor its HTTP
    // client is instrumented with the source's labels.
    let (topology, _) = start_topology(config.build().unwrap(), false).await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    for (id, address, plaintext) in sources {
        wait_for_tcp(address).await;
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(plaintext.as_bytes()).unwrap();
        let compressed = encoder.finish().unwrap();
        let network_size = f64::from(u32::try_from(compressed.len()).unwrap());
        let serialized_size = f64::from(u32::try_from(plaintext.len()).unwrap());
        assert!(network_size < serialized_size);
        let response = client
            .post(format!("http://{address}/"))
            .header("content-encoding", "gzip")
            .body(compressed)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let event = timeout(Duration::from_secs(10), events.next())
            .await
            .expect("timed out waiting for the HTTP source event")
            .expect("HTTP source output closed");
        assert_eq!(
            event
                .as_log()
                .get(log_schema().message_key_target_path().unwrap())
                .unwrap()
                .to_string_lossy(),
            plaintext,
        );
        let tags = &[
            ("component_id", id),
            ("component_kind", "source"),
            ("component_type", "http_server"),
            ("protocol", "http"),
        ];
        for (name, expected) in [
            ("component_received_network_bytes_total", network_size),
            ("component_received_bytes_total", serialized_size),
        ] {
            assert_eq!(
                counter_value_with_tags(name, tags).to_bits(),
                expected.to_bits(),
                "{name} must retain the receiving source's labels",
            );
        }
    }
    timeout(Duration::from_secs(10), topology.stop())
        .await
        .expect("HTTP source topology did not shut down");
}

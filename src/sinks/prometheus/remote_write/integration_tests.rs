use vector_lib::event::EventStatus;

use super::tests::*;
use crate::{
    SourceSender,
    config::{SinkConfig, SinkContext, SourceConfig, SourceContext},
    event::Event,
    sinks::prometheus::remote_write::config::RemoteWriteConfig,
    sources::prometheus::PrometheusRemoteWriteConfig,
    test_util::{self, addr::next_addr, wait_for_tcp},
    tls::{self, MaybeTlsSettings, TlsEnableableConfig},
};

#[tokio::test]
async fn insert_metrics_over_http() {
    insert_metrics(None).await;
}

#[tokio::test]
async fn insert_metrics_over_https() {
    insert_metrics(Some(TlsEnableableConfig::test_config())).await;
}

async fn insert_metrics(tls: Option<TlsEnableableConfig>) {
    let (_guard, address) = next_addr();
    let (tx, rx) = SourceSender::new_test_finalize(EventStatus::Delivered);

    let proto = MaybeTlsSettings::from_config(tls.as_ref(), true)
        .unwrap()
        .http_protocol_name();

    // Start a `prometheus_remote_write` source to act as the remote write receiver.
    let tls_yaml = tls.as_ref().map(|_| {
        format!(
            "tls:\n  enabled: true\n  ca_file: \"{}\"\n  crt_file: \"{}\"\n  key_file: \"{}\"",
            tls::TEST_PEM_CA_PATH,
            tls::TEST_PEM_CRT_PATH,
            tls::TEST_PEM_KEY_PATH
        )
    });
    let source: PrometheusRemoteWriteConfig = serde_yaml::from_str(&format!(
        "address: \"{address}\"\n{}",
        tls_yaml.unwrap_or_default()
    ))
    .unwrap();
    let source = source
        .build(SourceContext::new_test(tx, None))
        .await
        .expect("source should not fail to build");
    tokio::spawn(source);
    wait_for_tcp(address).await;

    let config = RemoteWriteConfig {
        endpoint: format!("{proto}://localhost:{}/", address.port()),
        tls: tls.map(|tls| tls.options),
        ..Default::default()
    };
    let events = create_events(0..5, |n| n * 11.0);
    let events_copy = events.clone();

    let cx = SinkContext::default();
    let (sink, _) = config.build(cx).await.expect("error building config");

    let mut output = test_util::spawn_collect_ready(
        async move {
            sink.run_events(events_copy).await.unwrap();
        },
        rx,
        1,
    )
    .await;

    // The MetricBuffer used by the sink may reorder the metrics, so
    // put them back into order before comparing.
    output.sort_unstable_by_key(|event| event.as_metric().name().to_owned());

    assert_eq!(output.len(), events.len());

    for (sent, received) in events.iter().zip(output.iter()) {
        let sent = sent.as_metric();
        let received = received.as_metric();
        assert_eq!(sent.name(), received.name());
        assert_eq!(sent.value(), received.value());
        assert_eq!(sent.tags(), received.tags());
        // Remote write stores timestamps with millisecond precision.
        assert_eq!(
            sent.timestamp().unwrap().timestamp_millis(),
            received.timestamp().unwrap().timestamp_millis()
        );
    }
}

fn create_events(name_range: std::ops::Range<i32>, value: impl Fn(f64) -> f64) -> Vec<Event> {
    name_range
        .map(move |num| create_event(format!("metric_{num}"), value(num as f64)))
        .collect()
}

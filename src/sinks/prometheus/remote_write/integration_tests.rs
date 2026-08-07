use std::net::SocketAddr;

use bytes::Bytes;
use futures::{FutureExt, SinkExt, TryFutureExt, channel::mpsc};
use futures_util::StreamExt;
use http::request::Parts;
use hyper::{
    Body, Request, Response, Server,
    service::{make_service_fn, service_fn},
};
use stream_cancel::{Trigger, Tripwire};
use vector_lib::event::EventStatus;

use super::tests::*;
use crate::{
    SourceSender,
    config::{SinkConfig, SinkContext, SourceConfig, SourceContext},
    event::Event,
    sinks::prometheus::remote_write::config::RemoteWriteConfig,
    sources::prometheus::PrometheusRemoteWriteConfig,
    test_util::{
        self,
        addr::next_addr,
        components::{HTTP_SINK_TAGS, assert_sink_compliance},
        wait_for_tcp,
    },
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
    // Verify the sink emits the expected compliance events while sending to a
    // remote write endpoint. A mock server is used as the receiver here (rather
    // than Vector's own source) so that only the sink's internal events are
    // recorded.
    assert_sink_compliance(&HTTP_SINK_TAGS, async {
        let (_guard, addr) = next_addr();
        let (rx, trigger, server) = build_mock_server(addr, tls.clone()).await;
        tokio::spawn(server);

        let proto = MaybeTlsSettings::from_config(tls.as_ref(), true)
            .unwrap()
            .http_protocol_name();
        let config = RemoteWriteConfig {
            endpoint: format!("{proto}://localhost:{}/write", addr.port()),
            tls: tls.clone().map(|tls| tls.options),
            ..Default::default()
        };
        let events = create_events(0..5, |n| n * 11.0);
        let cx = SinkContext::default();
        let (sink, _) = config.build(cx).await.expect("error building config");
        sink.run_events(events.clone()).await.unwrap();

        drop(trigger);
        let requests = rx.collect::<Vec<_>>().await;
        assert_eq!(requests.len(), 1);
    })
    .await;

    // Verify the sink's output is accepted by Vector's own
    // `prometheus_remote_write` source and that the metrics round-trip
    // correctly.
    let (_guard, address) = next_addr();
    let (tx, rx) = SourceSender::new_test_finalize(EventStatus::Delivered);

    let proto = MaybeTlsSettings::from_config(tls.as_ref(), true)
        .unwrap()
        .http_protocol_name();

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

    output.sort_unstable_by_key(|event| event.as_metric().name().to_owned());

    assert_eq!(output.len(), events.len());

    for (sent, received) in events.iter().zip(output.iter()) {
        let sent = sent.as_metric();
        let received = received.as_metric();
        assert_eq!(sent.name(), received.name());
        assert_eq!(sent.value(), received.value());
        assert_eq!(sent.tags(), received.tags());
        assert_eq!(
            sent.timestamp().unwrap().timestamp_millis(),
            received.timestamp().unwrap().timestamp_millis()
        );
    }
}

/// Builds a mock remote write HTTP server, optionally with TLS enabled.
async fn build_mock_server(
    addr: SocketAddr,
    tls: Option<TlsEnableableConfig>,
) -> (
    mpsc::Receiver<(Parts, Bytes)>,
    Trigger,
    impl std::future::Future<Output = Result<(), ()>>,
) {
    let (tx, rx) = mpsc::channel(100);
    let service = make_service_fn(move |_| {
        let tx = tx.clone();
        async move {
            Ok::<_, hyper::Error>(service_fn(move |req: Request<Body>| {
                let mut tx = tx.clone();
                async move {
                    let (parts, body) = req.into_parts();
                    tokio::spawn(async move {
                        let bytes = http_body::Body::collect(body).await.unwrap().to_bytes();
                        tx.send((parts, bytes)).await.unwrap();
                    });
                    Ok::<_, hyper::Error>(Response::new(Body::empty()))
                }
            }))
        }
    });

    let settings = MaybeTlsSettings::from_config(tls.as_ref(), true).unwrap();
    let listener = settings.bind(&addr).await.unwrap();
    let (trigger, tripwire) = Tripwire::new();
    let server = Server::builder(hyper::server::accept::from_stream(listener.accept_stream()))
        .serve(service)
        .with_graceful_shutdown(tripwire.then(crate::shutdown::tripwire_handler))
        .map_err(|error| panic!("Server error: {error}"));

    (rx, trigger, server)
}

fn create_events(name_range: std::ops::Range<i32>, value: impl Fn(f64) -> f64) -> Vec<Event> {
    name_range
        .map(move |num| create_event(format!("metric_{num}"), value(num as f64)))
        .collect()
}

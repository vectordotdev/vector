use bytes::Bytes;
use futures::{StreamExt, TryStreamExt};
use futures_util::stream;
use tokio::{net::UdpSocket, sync::mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::{codec::BytesCodec, udp::UdpFramed};
use vector_lib::{
    event::{metric::TagValue, Event, Metric, MetricKind, MetricTags, MetricValue, StatisticKind},
    metric_tags,
};

use crate::{
    config::{SinkConfig, SinkContext},
    sinks::{statsd::config::Mode, util::service::net::UdpConnectorConfig},
    test_util::{
        collect_n,
        components::{assert_sink_compliance, SINK_TAGS},
        next_addr, trace_init,
    },
};

use super::StatsdSinkConfig;

fn tags() -> MetricTags {
    metric_tags!(
        "normal_tag" => "value",
        "multi_value" => "true",
        "multi_value" => "false",
        "multi_value" => TagValue::Bare,
        "bare_tag" => TagValue::Bare,
    )
}

#[tokio::test]
async fn test_send_to_statsd() {
    trace_init();

    let addr = next_addr();

    let config = StatsdSinkConfig {
        default_namespace: Some("ns".into()),
        mode: Mode::Udp(UdpConnectorConfig::from_address(
            addr.ip().to_string(),
            addr.port(),
        )),
        batch: Default::default(),
        acknowledgements: Default::default(),
    };

    let events = vec![
        Event::Metric(
            Metric::new(
                "counter",
                MetricKind::Incremental,
                MetricValue::Counter { value: 1.5 },
            )
            .with_namespace(Some("vector"))
            .with_tags(Some(tags())),
        ),
        Event::Metric(
            Metric::new(
                "histogram",
                MetricKind::Incremental,
                MetricValue::Distribution {
                    samples: vector_lib::samples![2.0 => 100],
                    statistic: StatisticKind::Histogram,
                },
            )
            .with_namespace(Some("vector")),
        ),
    ];
    let (tx, rx) = mpsc::channel(1);

    let context = SinkContext::default();
    assert_sink_compliance(&SINK_TAGS, async move {
        let (sink, _healthcheck) = config.build(context).await.unwrap();

        let socket = UdpSocket::bind(addr).await.unwrap();
        tokio::spawn(async move {
            let mut stream = UdpFramed::new(socket, BytesCodec::new())
                .map_err(|error| error!(message = "Error reading line.", %error))
                .map_ok(|(bytes, _addr)| bytes.freeze());

            while let Some(Ok(item)) = stream.next().await {
                tx.send(item).await.unwrap();
            }
        });

        sink.run(stream::iter(events).map(Into::into))
            .await
            .expect("Running sink failed")
    })
    .await;

    let messages = collect_n(ReceiverStream::new(rx), 1).await;
    assert_eq!(
        messages[0],
        Bytes::from("vector.counter:1.5|c|#bare_tag,multi_value:true,multi_value:false,multi_value,normal_tag:value\nvector.histogram:2|h|@0.01\n"),
    );
}

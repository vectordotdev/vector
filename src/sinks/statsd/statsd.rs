use std::task::{Context, Poll};

use bytes::BytesMut;
use futures::{future, TryFutureExt};
use futures_util::FutureExt;
use tower::Service;

use crate::sinks::util::udp::UdpService;

pub struct StatsdSvc {
    inner: UdpService,
}

impl Service<BytesMut> for StatsdSvc {
    type Response = ();
    type Error = crate::Error;
    type Future = future::BoxFuture<'static, Result<(), Self::Error>>;

    // Emission of Error internal event is handled upstream by the caller
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    // Emission of Error internal event is handled upstream by the caller
    fn call(&mut self, frame: BytesMut) -> Self::Future {
        self.inner.call(frame).err_into().boxed()
    }
}

#[cfg(test)]
mod test {
    use bytes::Bytes;
    use futures::{channel::mpsc, StreamExt, TryStreamExt};
    use tokio::net::UdpSocket;
    use tokio_util::{codec::BytesCodec, udp::UdpFramed};
    use vector_core::{event::metric::TagValue, metric_tags};
    #[cfg(feature = "sources-statsd")]
    use {crate::sources::statsd::parser::parse, std::str::from_utf8};

    use super::*;
    use crate::{
        event::Metric,
        test_util::{
            components::{assert_sink_compliance, SINK_TAGS},
            *,
        },
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<StatsdSinkConfig>();
    }

    fn tags() -> MetricTags {
        metric_tags!(
            "normal_tag" => "value",
            "multi_value" => "true",
            "multi_value" => "false",
            "multi_value" => TagValue::Bare,
            "bare_tag" => TagValue::Bare,
        )
    }

    #[test]
    fn test_encode_tags() {
        let actual = encode_tags(&tags());
        let mut actual = actual.split(',').collect::<Vec<_>>();
        actual.sort();

        let mut expected =
            "bare_tag,normal_tag:value,multi_value:true,multi_value:false,multi_value"
                .split(',')
                .collect::<Vec<_>>();
        expected.sort();

        assert_eq!(actual, expected);
    }

    #[test]
    fn tags_order() {
        assert_eq!(
            &encode_tags(
                &vec![
                    ("a", "value"),
                    ("b", "value"),
                    ("c", "value"),
                    ("d", "value"),
                    ("e", "value"),
                ]
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect()
            ),
            "a:value,b:value,c:value,d:value,e:value"
        );
    }

    #[cfg(feature = "sources-statsd")]
    #[test]
    fn test_encode_counter() {
        let metric1 = Metric::new(
            "counter",
            MetricKind::Incremental,
            MetricValue::Counter { value: 1.5 },
        )
        .with_tags(Some(tags()));
        let event = Event::Metric(metric1.clone());
        let mut encoder = StatsdEncoder {
            default_namespace: None,
        };
        let mut frame = BytesMut::new();
        encoder.encode(event, &mut frame).unwrap();
        let metric2 = parse(from_utf8(&frame).unwrap().trim()).unwrap();
        vector_common::assert_event_data_eq!(metric1, metric2);
    }

    #[cfg(feature = "sources-statsd")]
    #[test]
    fn test_encode_absolute_counter() {
        let metric1 = Metric::new(
            "counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.5 },
        );
        let event = Event::Metric(metric1);
        let mut encoder = StatsdEncoder {
            default_namespace: None,
        };
        let mut frame = BytesMut::new();
        encoder.encode(event, &mut frame).unwrap();
        // The statsd parser will parse the counter as Incremental,
        // so we can't compare it with the parsed value.
        assert_eq!("counter:1.5|c\n", from_utf8(&frame).unwrap());
    }

    #[cfg(feature = "sources-statsd")]
    #[test]
    fn test_encode_gauge() {
        let metric1 = Metric::new(
            "gauge",
            MetricKind::Incremental,
            MetricValue::Gauge { value: -1.5 },
        )
        .with_tags(Some(tags()));
        let event = Event::Metric(metric1.clone());
        let mut encoder = StatsdEncoder {
            default_namespace: None,
        };
        let mut frame = BytesMut::new();
        encoder.encode(event, &mut frame).unwrap();
        let metric2 = parse(from_utf8(&frame).unwrap().trim()).unwrap();
        vector_common::assert_event_data_eq!(metric1, metric2);
    }

    #[cfg(feature = "sources-statsd")]
    #[test]
    fn test_encode_absolute_gauge() {
        let metric1 = Metric::new(
            "gauge",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 1.5 },
        )
        .with_tags(Some(tags()));
        let event = Event::Metric(metric1.clone());
        let mut encoder = StatsdEncoder {
            default_namespace: None,
        };
        let mut frame = BytesMut::new();
        encoder.encode(event, &mut frame).unwrap();
        let metric2 = parse(from_utf8(&frame).unwrap().trim()).unwrap();
        vector_common::assert_event_data_eq!(metric1, metric2);
    }

    #[cfg(feature = "sources-statsd")]
    #[test]
    fn test_encode_distribution() {
        let metric1 = Metric::new(
            "distribution",
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: vector_core::samples![1.5 => 1, 1.5 => 1],
                statistic: StatisticKind::Histogram,
            },
        )
        .with_tags(Some(tags()));

        let metric1_compressed = Metric::new(
            "distribution",
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: vector_core::samples![1.5 => 2],
                statistic: StatisticKind::Histogram,
            },
        )
        .with_tags(Some(tags()));

        let event = Event::Metric(metric1);
        let mut encoder = StatsdEncoder {
            default_namespace: None,
        };
        let mut frame = BytesMut::new();
        encoder.encode(event, &mut frame).unwrap();
        let metric2 = parse(from_utf8(&frame).unwrap().trim()).unwrap();
        vector_common::assert_event_data_eq!(metric1_compressed, metric2);
    }

    #[cfg(feature = "sources-statsd")]
    #[test]
    fn test_encode_set() {
        let metric1 = Metric::new(
            "set",
            MetricKind::Incremental,
            MetricValue::Set {
                values: vec!["abc".to_owned()].into_iter().collect(),
            },
        )
        .with_tags(Some(tags()));
        let event = Event::Metric(metric1.clone());
        let mut encoder = StatsdEncoder {
            default_namespace: None,
        };
        let mut frame = BytesMut::new();
        encoder.encode(event, &mut frame).unwrap();
        let metric2 = parse(from_utf8(&frame).unwrap().trim()).unwrap();
        vector_common::assert_event_data_eq!(metric1, metric2);
    }

    #[tokio::test]
    async fn test_send_to_statsd() {
        trace_init();

        let addr = next_addr();
        let mut batch = BatchConfig::default();
        batch.max_bytes = Some(512);

        let config = StatsdSinkConfig {
            default_namespace: Some("ns".into()),
            mode: Mode::Udp(StatsdUdpConfig {
                batch,
                udp: UdpSinkConfig::from_address(addr.to_string()),
            }),
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
                        samples: vector_core::samples![2.0 => 100],
                        statistic: StatisticKind::Histogram,
                    },
                )
                .with_namespace(Some("vector")),
            ),
        ];
        let (mut tx, rx) = mpsc::channel(0);

        let context = SinkContext::new_test();
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

        let messages = collect_n(rx, 1).await;
        assert_eq!(
            messages[0],
            Bytes::from("vector.counter:1.5|c|#bare_tag,multi_value:true,multi_value:false,multi_value,normal_tag:value\nvector.histogram:2|h|@0.01\n"),
        );
    }
}

use std::{
    fmt::Display,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    task::{Context, Poll},
};

use bytes::{BufMut, BytesMut};
use futures::{future, stream, SinkExt, TryFutureExt};
use futures_util::FutureExt;
use tokio_util::codec::Encoder;
use tower::{Service, ServiceBuilder};
use vector_config::configurable_component;
use vector_core::ByteSizeOf;

use super::util::SinkBatchSettings;
#[cfg(unix)]
use crate::sinks::util::unix::UnixSinkConfig;
use crate::{
    config::{AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext},
    event::{
        metric::{Metric, MetricKind, MetricTags, MetricValue, StatisticKind},
        Event,
    },
    internal_events::StatsdInvalidMetricError,
    sinks::util::{
        buffer::metrics::compress_distribution,
        encode_namespace,
        tcp::TcpSinkConfig,
        udp::{UdpService, UdpSinkConfig},
        BatchConfig, BatchSink, Buffer, Compression, EncodedEvent,
    },
};

pub struct StatsdSvc {
    inner: UdpService,
}

/// Configuration for the `statsd` sink.
#[configurable_component(sink("statsd"))]
#[derive(Clone, Debug)]
pub struct StatsdSinkConfig {
    /// Sets the default namespace for any metrics sent.
    ///
    /// This namespace is only used if a metric has no existing namespace. When a namespace is
    /// present, it is used as a prefix to the metric name, and separated with a period (`.`).
    #[serde(alias = "namespace")]
    pub default_namespace: Option<String>,

    #[serde(flatten)]
    pub mode: Mode,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

/// Socket mode.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(tag = "mode", rename_all = "snake_case")]
#[configurable(metadata(docs::enum_tag_description = "The type of socket to use."))]
pub enum Mode {
    /// Send over TCP.
    Tcp(TcpSinkConfig),

    /// Send over UDP.
    Udp(StatsdUdpConfig),

    /// Send over a Unix domain socket (UDS).
    #[cfg(unix)]
    Unix(UnixSinkConfig),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StatsdDefaultBatchSettings;

impl SinkBatchSettings for StatsdDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(1000);
    const MAX_BYTES: Option<usize> = Some(1300);
    const TIMEOUT_SECS: f64 = 1.0;
}

/// UDP configuration.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct StatsdUdpConfig {
    #[serde(flatten)]
    pub udp: UdpSinkConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<StatsdDefaultBatchSettings>,
}

fn default_address() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8125)
}

impl GenerateConfig for StatsdSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(&Self {
            default_namespace: None,
            mode: Mode::Udp(StatsdUdpConfig {
                batch: Default::default(),
                udp: UdpSinkConfig::from_address(default_address().to_string()),
            }),
            acknowledgements: Default::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
impl SinkConfig for StatsdSinkConfig {
    async fn build(
        &self,
        _cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let default_namespace = self.default_namespace.clone();
        let mut encoder = StatsdEncoder { default_namespace };
        match &self.mode {
            Mode::Tcp(config) => config.build(Default::default(), encoder),
            Mode::Udp(config) => {
                // 1432 bytes is a recommended packet size to fit into MTU
                // https://github.com/statsd/statsd/blob/master/docs/metric_types.md#multi-metric-packets
                // However we need to leave some space for +1 extra trailing event in the buffer.
                // Also one might keep an eye on server side limitations, like
                // mentioned here https://github.com/DataDog/dd-agent/issues/2638
                let batch = config.batch.into_batch_settings()?;
                let (service, healthcheck) = config.udp.build_service()?;
                let service = StatsdSvc { inner: service };
                let sink = BatchSink::new(
                    ServiceBuilder::new().service(service),
                    Buffer::new(batch.size, Compression::None),
                    batch.timeout,
                )
                .sink_map_err(|error| error!(message = "Fatal statsd sink error.", %error))
                .with_flat_map(move |event: Event| {
                    stream::iter({
                        let byte_size = event.size_of();
                        let mut bytes = BytesMut::new();

                        // Errors are handled by `Encoder`.
                        encoder
                            .encode(event, &mut bytes)
                            .map(|_| Ok(EncodedEvent::new(bytes, byte_size)))
                    })
                });

                Ok((super::VectorSink::from_event_sink(sink), healthcheck))
            }
            #[cfg(unix)]
            Mode::Unix(config) => config.build(Default::default(), encoder),
        }
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

// Note that if multi-valued tags are present, this encoding may change the order from the input
// event, since the tags with multiple values may not have been grouped together.
// This is not an issue, but noting as it may be an observed behavior.
fn encode_tags(tags: &MetricTags) -> String {
    let parts: Vec<_> = tags
        .iter_all()
        .map(|(name, tag_value)| match tag_value {
            Some(value) => format!("{}:{}", name, value),
            None => name.to_owned(),
        })
        .collect();

    // `parts` is already sorted by key because of BTreeMap
    parts.join(",")
}

fn push_event<V: Display>(
    buf: &mut Vec<String>,
    metric: &Metric,
    val: V,
    metric_type: &str,
    sample_rate: Option<u32>,
) {
    buf.push(format!("{}:{}|{}", metric.name(), val, metric_type));

    if let Some(sample_rate) = sample_rate {
        if sample_rate != 1 {
            buf.push(format!("@{}", 1.0 / f64::from(sample_rate)))
        }
    };

    if let Some(t) = metric.tags() {
        buf.push(format!("#{}", encode_tags(t)));
    };
}

#[derive(Debug, Clone)]
struct StatsdEncoder {
    default_namespace: Option<String>,
}

impl Encoder<Event> for StatsdEncoder {
    type Error = codecs::encoding::Error;

    fn encode(&mut self, event: Event, bytes: &mut BytesMut) -> Result<(), Self::Error> {
        let mut buf = Vec::new();

        let metric = event.as_metric();
        match metric.value() {
            MetricValue::Counter { value } => {
                push_event(&mut buf, metric, value, "c", None);
            }
            MetricValue::Gauge { value } => {
                match metric.kind() {
                    MetricKind::Incremental => {
                        push_event(&mut buf, metric, format!("{:+}", value), "g", None)
                    }
                    MetricKind::Absolute => push_event(&mut buf, metric, value, "g", None),
                };
            }
            MetricValue::Distribution { samples, statistic } => {
                let metric_type = match statistic {
                    StatisticKind::Histogram => "h",
                    StatisticKind::Summary => "d",
                };

                // TODO: This would actually be good to potentially add a helper combinator for, in the same vein as
                // `SinkBuilderExt::normalized`, that provides a metric "optimizer" for doing these sorts of things. We
                // don't actually compress distributions as-is in other metrics sinks unless they use the old-style
                // approach coupled with `MetricBuffer`. While not every sink would benefit from this -- the
                // `datadog_metrics` sink always converts distributions to sketches anyways, for example -- a lot of
                // them could.
                //
                // This would also imply rewriting this sink in the new style to take advantage of it.
                let mut samples = samples.clone();
                let compressed_samples = compress_distribution(&mut samples);
                for sample in compressed_samples {
                    push_event(
                        &mut buf,
                        metric,
                        sample.value,
                        metric_type,
                        Some(sample.rate),
                    );
                }
            }
            MetricValue::Set { values } => {
                for val in values {
                    push_event(&mut buf, metric, val, "s", None);
                }
            }
            _ => {
                emit!(StatsdInvalidMetricError {
                    value: metric.value(),
                    kind: &metric.kind(),
                });

                return Ok(());
            }
        };

        let message = encode_namespace(
            metric.namespace().or(self.default_namespace.as_deref()),
            '.',
            buf.join("|"),
        );

        bytes.put_slice(&message.into_bytes());
        bytes.put_u8(b'\n');

        Ok(())
    }
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

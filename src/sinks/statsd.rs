use std::{
    fmt::Display,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    task::{Context, Poll},
};

use bytes::{BufMut, BytesMut};
use futures::{future, stream, FutureExt, SinkExt, TryFutureExt};
use serde::{Deserialize, Serialize};
use tokio_util::codec::Encoder;
use tower::{Service, ServiceBuilder};
use vector_core::ByteSizeOf;

use super::util::SinkBatchSettings;
#[cfg(unix)]
use crate::sinks::util::unix::UnixSinkConfig;
use crate::{
    config::{
        AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext, SinkDescription,
    },
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

#[derive(Deserialize, Serialize, Debug, Clone)]
// TODO: add back when serde-rs/serde#1358 is addressed
// #[serde(deny_unknown_fields)]
pub struct StatsdSinkConfig {
    #[serde(alias = "namespace")]
    pub default_namespace: Option<String>,
    #[serde(flatten)]
    pub mode: Mode,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Mode {
    Tcp(TcpSinkConfig),
    Udp(StatsdUdpConfig),
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

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct StatsdUdpConfig {
    #[serde(flatten)]
    pub udp: UdpSinkConfig,

    #[serde(default)]
    pub batch: BatchConfig<StatsdDefaultBatchSettings>,
}

inventory::submit! {
    SinkDescription::new::<StatsdSinkConfig>("statsd")
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
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "statsd")]
impl SinkConfig for StatsdSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let default_namespace = self.default_namespace.clone();
        let mut encoder = StatsdEncoder { default_namespace };
        match &self.mode {
            Mode::Tcp(config) => config.build(cx, Default::default(), encoder),
            Mode::Udp(config) => {
                // 1432 bytes is a recommended packet size to fit into MTU
                // https://github.com/statsd/statsd/blob/master/docs/metric_types.md#multi-metric-packets
                // However we need to leave some space for +1 extra trailing event in the buffer.
                // Also one might keep an eye on server side limitations, like
                // mentioned here https://github.com/DataDog/dd-agent/issues/2638
                let batch = config.batch.into_batch_settings()?;
                let (service, healthcheck) = config.udp.build_service(cx.clone())?;
                let service = StatsdSvc { inner: service };
                let sink = BatchSink::new(
                    ServiceBuilder::new().service(service),
                    Buffer::new(batch.size, Compression::None),
                    batch.timeout,
                    cx.acker(),
                )
                .sink_map_err(|error| error!(message = "Fatal statsd sink error.", %error))
                .with_flat_map(move |event: Event| {
                    stream::iter({
                        let byte_size = event.size_of();
                        let mut bytes = BytesMut::new();
                        encoder
                            .encode(event, &mut bytes)
                            .map(|_| Ok(EncodedEvent::new(bytes, byte_size)))
                    })
                });

                Ok((super::VectorSink::from_event_sink(sink), healthcheck))
            }
            #[cfg(unix)]
            Mode::Unix(config) => config.build(cx, Default::default(), encoder),
        }
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn sink_type(&self) -> &'static str {
        "statsd"
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        None
    }
}

fn encode_tags(tags: &MetricTags) -> String {
    let parts: Vec<_> = tags
        .iter()
        .map(|(name, value)| {
            if value == "true" {
                name.to_string()
            } else {
                format!("{}:{}", name, value)
            }
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

                // TODO: This is a good example of where it'd be nice to bake this into a dedicated combinator or
                // normalizer feature, although this sink also needs to be rewritten in the new-style to begin with.
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
            metric
                .namespace()
                .or_else(|| self.default_namespace.as_deref()),
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

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

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
    #[cfg(feature = "sources-statsd")]
    use {crate::sources::statsd::parser::parse, std::str::from_utf8};

    use super::*;
    use crate::{event::Metric, test_util::*};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<StatsdSinkConfig>();
    }

    fn tags() -> MetricTags {
        vec![
            ("normal_tag".to_owned(), "value".to_owned()),
            ("true_tag".to_owned(), "true".to_owned()),
            ("empty_tag".to_owned(), "".to_owned()),
        ]
        .into_iter()
        .collect()
    }

    #[test]
    fn test_encode_tags() {
        assert_eq!(
            &encode_tags(&tags()),
            "empty_tag:,normal_tag:value,true_tag"
        );
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
        };

        let context = SinkContext::new_test();
        let (sink, _healthcheck) = config.build(context).await.unwrap();

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

        let socket = UdpSocket::bind(addr).await.unwrap();
        tokio::spawn(async move {
            let mut stream = UdpFramed::new(socket, BytesCodec::new())
                .map_err(|error| error!(message = "Error reading line.", %error))
                .map_ok(|(bytes, _addr)| bytes.freeze());

            while let Some(Ok(item)) = stream.next().await {
                tx.send(item).await.unwrap();
            }
        });

        sink.run_events(events).await.unwrap();

        let messages = collect_n(rx, 1).await;
        assert_eq!(
            messages[0],
            Bytes::from("vector.counter:1.5|c|#empty_tag:,normal_tag:value,true_tag\nvector.histogram:2|h|@0.01\n"),
        );
    }
}

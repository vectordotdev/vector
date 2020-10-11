use crate::{
    buffers::Acker,
    config::{DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    event::metric::{Metric, MetricKind, MetricValue, StatisticKind},
    event::Event,
    internal_events::StatsdInvalidMetricReceived,
    sinks::util::{encode_namespace, BatchConfig, BatchSettings, BatchSink, Buffer, Compression},
};
use futures::{future, FutureExt};
use futures01::{stream, Sink};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::collections::BTreeMap;
use std::fmt::Display;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::task::{Context, Poll};
use tower::{Service, ServiceBuilder};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("failed to bind to UDP listener socket, error = {:?}", source))]
    SocketBindError { source: std::io::Error },
}

pub struct StatsdSvc {
    client: Client,
}

pub struct Client {
    socket: UdpSocket,
    address: SocketAddr,
}

impl Client {
    pub fn new(address: SocketAddr) -> crate::Result<Self> {
        let from = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0);
        let socket = UdpSocket::bind(&from).context(SocketBindError)?;
        Ok(Client { socket, address })
    }

    pub fn send(&self, buf: &[u8]) -> usize {
        self.socket
            .send_to(buf, &self.address)
            .map_err(|e| error!("Error sending datagram: {:?}", e))
            .unwrap_or_default()
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct StatsdSinkConfig {
    pub namespace: Option<String>,
    #[serde(default = "default_address")]
    pub address: SocketAddr,
    #[serde(default)]
    pub batch: BatchConfig,
}

pub fn default_address() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8125)
}

inventory::submit! {
    SinkDescription::new::<StatsdSinkConfig>("statsd")
}

impl GenerateConfig for StatsdSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(&Self {
            namespace: None,
            address: default_address(),
            batch: Default::default(),
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
        let sink = StatsdSvc::new(self.clone(), cx.acker())?;
        Ok((sink, future::ok(()).boxed()))
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }

    fn sink_type(&self) -> &'static str {
        "statsd"
    }
}

impl StatsdSvc {
    pub fn new(config: StatsdSinkConfig, acker: Acker) -> crate::Result<super::VectorSink> {
        // 1432 bytes is a recommended packet size to fit into MTU
        // https://github.com/statsd/statsd/blob/master/docs/metric_types.md#multi-metric-packets
        // However we need to leave some space for +1 extra trailing event in the buffer.
        // Also one might keep an eye on server side limitations, like
        // mentioned here https://github.com/DataDog/dd-agent/issues/2638
        let batch = BatchSettings::default()
            .bytes(1300)
            .events(1000)
            .timeout(1)
            .parse_config(config.batch)?;
        let namespace = config.namespace.clone();

        let client = Client::new(config.address)?;
        let service = StatsdSvc { client };

        let svc = ServiceBuilder::new().service(service);

        let sink = BatchSink::new(
            svc,
            Buffer::new(batch.size, Compression::None),
            batch.timeout,
            acker,
        )
        .sink_map_err(|e| error!("Fatal statsd sink error: {}", e))
        .with_flat_map(move |event| stream::iter_ok(encode_event(event, namespace.as_deref())));

        Ok(super::VectorSink::Futures01Sink(Box::new(sink)))
    }
}

fn encode_tags(tags: &BTreeMap<String, String>) -> String {
    let mut parts: Vec<_> = tags
        .iter()
        .map(|(name, value)| {
            if value == "true" {
                name.to_string()
            } else {
                format!("{}:{}", name, value)
            }
        })
        .collect();
    parts.sort();
    parts.join(",")
}

fn push_event<V: Display>(
    buf: &mut Vec<String>,
    metric: &Metric,
    val: V,
    metric_type: &str,
    sample_rate: Option<u32>,
) {
    buf.push(format!("{}:{}|{}", metric.name, val, metric_type));

    if let Some(sample_rate) = sample_rate {
        if sample_rate != 1 {
            buf.push(format!("@{}", 1.0 / f64::from(sample_rate)))
        }
    };

    if let Some(t) = &metric.tags {
        buf.push(format!("#{}", encode_tags(t)));
    };
}

fn encode_event(event: Event, namespace: Option<&str>) -> Option<Vec<u8>> {
    let mut buf = Vec::new();

    let metric = event.as_metric();
    match &metric.value {
        MetricValue::Counter { value } => {
            push_event(&mut buf, &metric, value, "c", None);
        }
        MetricValue::Gauge { value } => {
            match metric.kind {
                MetricKind::Incremental => {
                    push_event(&mut buf, &metric, format!("{:+}", value), "g", None)
                }
                MetricKind::Absolute => push_event(&mut buf, &metric, value, "g", None),
            };
        }
        MetricValue::Distribution {
            values,
            sample_rates,
            statistic,
        } => {
            let metric_type = match statistic {
                StatisticKind::Histogram => "h",
                StatisticKind::Summary => "d",
            };
            for (val, sample_rate) in values.iter().zip(sample_rates.iter()) {
                push_event(&mut buf, &metric, val, metric_type, Some(*sample_rate));
            }
        }
        MetricValue::Set { values } => {
            for val in values {
                push_event(&mut buf, &metric, val, "s", None);
            }
        }
        _ => {
            emit!(StatsdInvalidMetricReceived {
                value: &metric.value,
                kind: &metric.kind,
            });

            return None;
        }
    };

    let message = encode_namespace(namespace, '.', buf.join("|"));

    let mut body: Vec<u8> = message.into_bytes();
    body.push(b'\n');

    Some(body)
}

impl Service<Vec<u8>> for StatsdSvc {
    type Response = ();
    type Error = tokio::io::Error;
    type Future = future::Ready<Result<(), Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut frame: Vec<u8>) -> Self::Future {
        // remove trailing delimiter
        if let Some(b'\n') = frame.last() {
            frame.pop();
        };
        self.client.send(frame.as_ref());
        future::ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        buffers::Acker,
        event::{metric::MetricKind, metric::MetricValue, metric::StatisticKind, Metric},
        test_util::{collect_n, trace_init},
        Event,
    };
    use bytes::Bytes;
    use futures::{compat::Sink01CompatExt, stream, SinkExt, StreamExt, TryStreamExt};
    use futures01::sync::mpsc;
    use tokio::net::UdpSocket;
    use tokio_util::{codec::BytesCodec, udp::UdpFramed};
    #[cfg(feature = "sources-statsd")]
    use {crate::sources::statsd::parser::parse, std::str::from_utf8};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<StatsdSinkConfig>();
    }

    fn tags() -> BTreeMap<String, String> {
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

    #[cfg(feature = "sources-statsd")]
    #[test]
    fn test_encode_counter() {
        let metric1 = Metric {
            name: "counter".to_owned(),
            timestamp: None,
            tags: Some(tags()),
            kind: MetricKind::Incremental,
            value: MetricValue::Counter { value: 1.5 },
        };
        let event = Event::Metric(metric1.clone());
        let frame = &encode_event(event, None).unwrap();
        let metric2 = parse(from_utf8(&frame).unwrap().trim()).unwrap();
        assert_eq!(metric1, metric2);
    }

    #[cfg(feature = "sources-statsd")]
    #[test]
    fn test_encode_absolute_counter() {
        let metric1 = Metric {
            name: "counter".to_owned(),
            timestamp: None,
            tags: None,
            kind: MetricKind::Absolute,
            value: MetricValue::Counter { value: 1.5 },
        };
        let event = Event::Metric(metric1);
        let frame = &encode_event(event, None).unwrap();
        // The statsd parser will parse the counter as Incremental,
        // so we can't compare it with the parsed value.
        assert_eq!("counter:1.5|c\n", from_utf8(&frame).unwrap());
    }

    #[cfg(feature = "sources-statsd")]
    #[test]
    fn test_encode_gauge() {
        let metric1 = Metric {
            name: "gauge".to_owned(),
            timestamp: None,
            tags: Some(tags()),
            kind: MetricKind::Incremental,
            value: MetricValue::Gauge { value: -1.5 },
        };
        let event = Event::Metric(metric1.clone());
        let frame = &encode_event(event, None).unwrap();
        let metric2 = parse(from_utf8(&frame).unwrap().trim()).unwrap();
        assert_eq!(metric1, metric2);
    }

    #[cfg(feature = "sources-statsd")]
    #[test]
    fn test_encode_absolute_gauge() {
        let metric1 = Metric {
            name: "gauge".to_owned(),
            timestamp: None,
            tags: Some(tags()),
            kind: MetricKind::Absolute,
            value: MetricValue::Gauge { value: 1.5 },
        };
        let event = Event::Metric(metric1.clone());
        let frame = &encode_event(event, None).unwrap();
        let metric2 = parse(from_utf8(&frame).unwrap().trim()).unwrap();
        assert_eq!(metric1, metric2);
    }

    #[cfg(feature = "sources-statsd")]
    #[test]
    fn test_encode_distribution() {
        let metric1 = Metric {
            name: "distribution".to_owned(),
            timestamp: None,
            tags: Some(tags()),
            kind: MetricKind::Incremental,
            value: MetricValue::Distribution {
                values: vec![1.5],
                sample_rates: vec![1],
                statistic: StatisticKind::Histogram,
            },
        };
        let event = Event::Metric(metric1.clone());
        let frame = &encode_event(event, None).unwrap();
        let metric2 = parse(from_utf8(&frame).unwrap().trim()).unwrap();
        assert_eq!(metric1, metric2);
    }

    #[cfg(feature = "sources-statsd")]
    #[test]
    fn test_encode_set() {
        let metric1 = Metric {
            name: "set".to_owned(),
            timestamp: None,
            tags: Some(tags()),
            kind: MetricKind::Incremental,
            value: MetricValue::Set {
                values: vec!["abc".to_owned()].into_iter().collect(),
            },
        };
        let event = Event::Metric(metric1.clone());
        let frame = &encode_event(event, None).unwrap();
        let metric2 = parse(from_utf8(&frame).unwrap().trim()).unwrap();
        assert_eq!(metric1, metric2);
    }

    #[tokio::test]
    async fn test_send_to_statsd() {
        trace_init();

        let config = StatsdSinkConfig {
            namespace: Some("vector".into()),
            address: default_address(),
            batch: BatchConfig {
                max_bytes: Some(512),
                timeout_secs: Some(1),
                ..Default::default()
            },
        };
        let sink = StatsdSvc::new(config, Acker::Null).unwrap();

        let events = vec![
            Event::Metric(Metric {
                name: "counter".to_owned(),
                timestamp: None,
                tags: Some(tags()),
                kind: MetricKind::Incremental,
                value: MetricValue::Counter { value: 1.5 },
            }),
            Event::Metric(Metric {
                name: "histogram".to_owned(),
                timestamp: None,
                tags: None,
                kind: MetricKind::Incremental,
                value: MetricValue::Distribution {
                    values: vec![2.0],
                    sample_rates: vec![100],
                    statistic: StatisticKind::Histogram,
                },
            }),
        ];
        let (tx, rx) = mpsc::channel(1);

        let socket = UdpSocket::bind(default_address()).await.unwrap();
        tokio::spawn(async move {
            UdpFramed::new(socket, BytesCodec::new())
                .map_err(|e| error!("Error reading line: {:?}", e))
                .map_ok(|(bytes, _addr)| bytes.freeze())
                .forward(
                    tx.sink_compat()
                        .sink_map_err(|e| error!("Error sending event: {:?}", e)),
                )
                .await
                .unwrap()
        });

        sink.run(stream::iter(events)).await.unwrap();

        let messages = collect_n(rx, 1).await.unwrap();
        assert_eq!(
            messages[0],
            Bytes::from("vector.counter:1.5|c|#empty_tag:,normal_tag:value,true_tag\nvector.histogram:2|h|@0.01"),
        );
    }
}

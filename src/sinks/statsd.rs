#[cfg(unix)]
use crate::sinks::util::unix::{IntoUnixSink, UnixSinkConfig};
use crate::{
    config::{DataType, SinkConfig, SinkContext, SinkDescription},
    event::metric::{Metric, MetricKind, MetricValue, StatisticKind},
    event::Event,
    sinks::util::{
        tcp::{IntoTcpSink, TcpSinkConfig},
        udp::{IntoUdpSink, UdpBuildError, UdpSinkConfig},
    },
    sinks::util::{BatchConfig, BatchSettings, BatchSink, Buffer, Compression},
};
use futures::{future, FutureExt};
use futures01::{stream, Sink};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::collections::BTreeMap;
use std::fmt::Display;
use std::task::{Context, Poll};
use tower::{Service, ServiceBuilder};

#[derive(Debug, Snafu)]
pub enum StatsdError {
    UdpError {
        #[snafu(source)]
        source: UdpBuildError,
    },
    SendError,
}

pub struct StatsdSvc {
    client: Client,
}

#[derive(Clone)]
enum Client {
    Tcp(IntoTcpSink),
    Udp(IntoUdpSink),
    #[cfg(unix)]
    Unix(IntoUnixSink),
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct StatsdSinkConfig {
    pub namespace: String,
    pub mode: Mode,
    #[serde(default)]
    pub batch: BatchConfig,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Mode {
    Tcp(TcpSinkConfig),
    Udp(UdpSinkConfig),
    #[cfg(unix)]
    Unix(UnixSinkConfig),
}

inventory::submit! {
    SinkDescription::new_without_default::<StatsdSinkConfig>("statsd")
}

#[typetag::serde(name = "statsd")]
impl SinkConfig for StatsdSinkConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        // 1432 bytes is a recommended packet size to fit into MTU
        // https://github.com/statsd/statsd/blob/master/docs/metric_types.md#multi-metric-packets
        // However we need to leave some space for +1 extra trailing event in the buffer.
        // Also one might keep an eye on server side limitations, like
        // mentioned here https://github.com/DataDog/dd-agent/issues/2638
        let batch = BatchSettings::default()
            .bytes(1300)
            .events(1000)
            .timeout(1)
            .parse_config(self.batch.clone())?;
        let namespace = self.namespace.clone();

        let (client, healthcheck) = match &self.mode {
            Mode::Tcp(ref config) => {
                let (inner, healthcheck) = config.prepare(cx.clone())?;
                (Client::Tcp(inner), healthcheck)
            }
            Mode::Udp(ref config) => {
                let (inner, healthcheck) = config.prepare(cx.clone())?;
                (Client::Udp(inner), healthcheck)
            }
            #[cfg(unix)]
            Mode::Unix(ref config) => {
                let (inner, healthcheck) = config.prepare()?;
                (Client::Unix(inner), healthcheck)
            }
        };
        let service = StatsdSvc { client };

        let sink = BatchSink::new(
            ServiceBuilder::new().service(service),
            Buffer::new(batch.size, Compression::None),
            batch.timeout,
            cx.acker(),
        )
        .sink_map_err(|e| error!("Fatal statsd sink error: {}", e))
        .with_flat_map(move |event| stream::iter_ok(encode_event(event, &namespace)));

        Ok((Box::new(sink), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }

    fn sink_type(&self) -> &'static str {
        "statsd"
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

fn encode_event(event: Event, namespace: &str) -> Option<Vec<u8>> {
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
            warn!(
                "invalid metric sent to statsd sink ({:?}) ({:?})",
                metric.kind, metric.value
            );
        }
    };

    let mut message: String = buf.join("|");
    if !namespace.is_empty() {
        message = format!("{}.{}", namespace, message);
    };

    let mut body: Vec<u8> = message.into_bytes();
    body.push(b'\n');

    Some(body)
}

impl Service<Vec<u8>> for StatsdSvc {
    type Response = ();
    type Error = StatsdError;
    type Future = future::BoxFuture<'static, Result<(), Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, frame: Vec<u8>) -> Self::Future {
        use bytes::Bytes;
        use futures::compat::Sink01CompatExt;
        use futures::Sink;
        use futures::SinkExt;
        use std::pin::Pin;

        let client = self.client.clone();
        async move {
            let mut sink: Pin<Box<dyn Sink<Bytes, Error = ()> + 'static + Send>> = match client {
                Client::Udp(inner) => {
                    Box::pin(inner.clone().into_sink().context(UdpError)?.sink_compat())
                }
                Client::Tcp(inner) => Box::pin(inner.clone().into_sink().sink_compat()),
                #[cfg(unix)]
                Client::Unix(inner) => Box::pin(inner.clone().into_sink().sink_compat()),
            };
            sink.send(frame.into())
                .await
                .map_err(|_| StatsdError::SendError)?;
            Ok(())
        }
        .boxed()
    }
}

/*
#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        buffers::Acker,
        event::{metric::MetricKind, metric::MetricValue, metric::StatisticKind, Metric},
        test_util::{collect_n, runtime},
        Event,
    };
    use bytes::Bytes;
    use futures::compat::{Future01CompatExt, Sink01CompatExt};
    use futures::{SinkExt, StreamExt, TryStreamExt};
    use futures01::{sync::mpsc, Sink};
    use tokio::net::UdpSocket;
    use tokio_util::{codec::BytesCodec, udp::UdpFramed};
    #[cfg(feature = "sources-statsd")]
    use {crate::sources::statsd::parser::parse, std::str::from_utf8};

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
        let frame = &encode_event(event, "").unwrap();
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
        let frame = &encode_event(event, "").unwrap();
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
        let frame = &encode_event(event, "").unwrap();
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
        let frame = &encode_event(event, "").unwrap();
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
        let frame = &encode_event(event, "").unwrap();
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
        let frame = &encode_event(event, "").unwrap();
        let metric2 = parse(from_utf8(&frame).unwrap().trim()).unwrap();
        assert_eq!(metric1, metric2);
    }

    /*
    #[test]
    fn test_send_to_statsd() {
        crate::test_util::trace_init();

        let mut rt = runtime();
        rt.block_on_std(async move {
            let config = StatsdSinkConfig {
                namespace: "vector".into(),
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
                        statistic: StatisticKind::Histogram
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

            let stream = stream::iter_ok(events);
            let _ = sink.send_all(stream).compat().await.unwrap();

            let messages = collect_n(rx, 1).compat().await.ok().unwrap();
            assert_eq!(
                messages[0],
                Bytes::from("vector.counter:1.5|c|#empty_tag:,normal_tag:value,true_tag\nvector.histogram:2|h|@0.01"),
            );
        });
    }
    */
}
*/

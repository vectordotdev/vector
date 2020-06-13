use crate::{
    buffers::Acker,
    event::metric::{MetricKind, MetricValue},
    event::Event,
    sinks::util::{service2::TowerCompat, BatchBytesConfig, BatchSink, Buffer, Compression},
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use futures::{future, FutureExt, TryFutureExt};
use futures01::{stream::iter_ok, Sink};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::task::{Context, Poll};
use tower03::{Service, ServiceBuilder};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("failed to bind to udp listener socket, error = {:?}", source))]
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
            .map_err(|e| error!("error sending datagram: {:?}", e))
            .unwrap_or_default()
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct StatsdSinkConfig {
    pub namespace: String,
    #[serde(default = "default_address")]
    pub address: SocketAddr,
    #[serde(default)]
    pub batch: BatchBytesConfig,
}

pub fn default_address() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8125)
}

inventory::submit! {
    SinkDescription::new_without_default::<StatsdSinkConfig>("statsd")
}

#[typetag::serde(name = "statsd")]
impl SinkConfig for StatsdSinkConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let sink = StatsdSvc::new(self.clone(), cx.acker())?;
        let healthcheck = StatsdSvc::healthcheck(self.clone()).boxed().compat();
        Ok((sink, Box::new(healthcheck)))
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }

    fn sink_type(&self) -> &'static str {
        "statsd"
    }
}

impl StatsdSvc {
    pub fn new(config: StatsdSinkConfig, acker: Acker) -> crate::Result<super::RouterSink> {
        // 1432 bytes is a recommended packet size to fit into MTU
        // https://github.com/statsd/statsd/blob/master/docs/metric_types.md#multi-metric-packets
        // However we need to leave some space for +1 extra trailing event in the buffer.
        // Also one might keep an eye on server side limitations, like
        // mentioned here https://github.com/DataDog/dd-agent/issues/2638
        let batch = config.batch.unwrap_or(1300, 1);
        let namespace = config.namespace.clone();

        let client = Client::new(config.address)?;
        let service = StatsdSvc { client };

        let svc = ServiceBuilder::new().service(service);

        let sink = BatchSink::new(
            TowerCompat::new(svc),
            Buffer::new(Compression::None),
            batch,
            acker,
        )
        .sink_map_err(|e| error!("Fatal statsd sink error: {}", e))
        .with_flat_map(move |event| iter_ok(encode_event(event, &namespace)));

        Ok(Box::new(sink))
    }

    async fn healthcheck(_config: StatsdSinkConfig) -> crate::Result<()> {
        Ok(())
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

fn encode_event(event: Event, namespace: &str) -> Option<Vec<u8>> {
    let mut buf = Vec::new();

    let metric = event.as_metric();
    match metric.kind {
        MetricKind::Incremental => match &metric.value {
            MetricValue::Counter { value } => {
                buf.push(format!("{}:{}", metric.name, value));
                buf.push("c".to_string());
                if let Some(t) = &metric.tags {
                    buf.push(format!("#{}", encode_tags(t)));
                };
            }
            MetricValue::Gauge { value } => {
                buf.push(format!("{}:{:+}", metric.name, value));
                buf.push("g".to_string());
                if let Some(t) = &metric.tags {
                    buf.push(format!("#{}", encode_tags(t)));
                };
            }
            MetricValue::Distribution {
                values,
                sample_rates,
            } => {
                for (val, sample_rate) in values.iter().zip(sample_rates.iter()) {
                    buf.push(format!("{}:{}", metric.name, val));
                    buf.push("h".to_string());
                    if *sample_rate != 1 {
                        buf.push(format!("@{}", 1.0 / f64::from(*sample_rate)));
                    };
                    if let Some(t) = &metric.tags {
                        buf.push(format!("#{}", encode_tags(t)));
                    };
                }
            }
            MetricValue::Set { values } => {
                for val in values {
                    buf.push(format!("{}:{}", metric.name, val));
                    buf.push("s".to_string());
                    if let Some(t) = &metric.tags {
                        buf.push(format!("#{}", encode_tags(t)));
                    };
                }
            }
            _ => {}
        },
        MetricKind::Absolute => {
            match &metric.value {
                MetricValue::Gauge { value } => {
                    buf.push(format!("{}:{}", metric.name, value));
                    buf.push("g".to_string());
                    if let Some(t) = &metric.tags {
                        buf.push(format!("#{}", encode_tags(t)));
                    };
                }
                _ => {}
            };
        }
    }

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
        event::{metric::MetricKind, metric::MetricValue, Metric},
        test_util::{collect_n, runtime},
        Event,
    };
    use bytes::Bytes;
    use futures01::{future, stream::Stream, sync::mpsc, Future, Sink};
    use std::time::{Duration, Instant};
    use tokio01::{
        self,
        codec::BytesCodec,
        net::{UdpFramed, UdpSocket},
    };
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
    fn test_encode_distribution() {
        let metric1 = Metric {
            name: "distribution".to_owned(),
            timestamp: None,
            tags: Some(tags()),
            kind: MetricKind::Incremental,
            value: MetricValue::Distribution {
                values: vec![1.5],
                sample_rates: vec![1],
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

    #[test]
    fn test_send_to_statsd() {
        crate::test_util::trace_init();

        let config = StatsdSinkConfig {
            namespace: "vector".into(),
            address: default_address(),
            batch: BatchBytesConfig {
                max_size: Some(512),
                timeout_secs: Some(1),
            },
        };

        let mut rt = runtime();
        let sink = StatsdSvc::new(config, Acker::Null).unwrap();

        let mut events = Vec::new();
        let event = Event::Metric(Metric {
            name: "counter".to_owned(),
            timestamp: None,
            tags: Some(tags()),
            kind: MetricKind::Incremental,
            value: MetricValue::Counter { value: 1.5 },
        });
        events.push(event);

        let event = Event::Metric(Metric {
            name: "histogram".to_owned(),
            timestamp: None,
            tags: None,
            kind: MetricKind::Incremental,
            value: MetricValue::Distribution {
                values: vec![2.0],
                sample_rates: vec![100],
            },
        });
        events.push(event);

        let stream = iter_ok(events.clone().into_iter());
        let sender = sink.send_all(stream);
        let deadline = Instant::now() + Duration::from_millis(100);

        // Add a delay to the write side to let the read side
        // poll for read interest. Otherwise, this could cause
        // a race condition in noisy environments.
        let sender = tokio01::timer::Delay::new(deadline)
            .map_err(drop)
            .and_then(|_| sender);

        let (tx, rx) = mpsc::channel(1);

        let receiver = Box::new(
            future::lazy(|| {
                let socket = UdpSocket::bind(&default_address()).unwrap();
                future::ok(socket)
            })
            .and_then(|socket| {
                UdpFramed::new(socket, BytesCodec::new())
                    .map_err(|e| error!("error reading line: {:?}", e))
                    .map(|(bytes, _addr)| bytes)
                    .forward(tx.sink_map_err(|e| error!("error sending event: {:?}", e)))
                    .map(|_| ())
            }),
        );

        rt.spawn(receiver);
        let _ = rt.block_on(sender).unwrap();

        let messages = rt.block_on(collect_n(rx, 1)).ok().unwrap();
        assert_eq!(
            messages[0],
            Bytes::from("vector.counter:1.5|c|#empty_tag:,normal_tag:value,true_tag\nvector.histogram:2|h|@0.01")
        );
    }
}

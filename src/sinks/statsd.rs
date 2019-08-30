use crate::{
    buffers::Acker,
    event::{metric::Direction, Event, Metric},
    sinks::util::{BatchServiceSink, SinkExt},
    topology::config::{DataType, SinkConfig},
};
use futures::{future, Future, Poll};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::Duration;
use tower::{Service, ServiceBuilder};

pub struct StatsdSvc {
    client: Client,
    config: StatsdSinkConfig,
}

pub struct Client {
    socket: UdpSocket,
    address: SocketAddr,
}

impl Client {
    pub fn new(address: SocketAddr) -> Self {
        let from = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);
        let socket = UdpSocket::bind(&from).expect("failed to bind to udp listener socket");
        Client { socket, address }
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
    pub batch_size: Option<usize>,
    pub batch_timeout: Option<u64>,
}

pub fn default_address() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8125)
}

#[typetag::serde(name = "statsd")]
impl SinkConfig for StatsdSinkConfig {
    fn build(&self, acker: Acker) -> Result<(super::RouterSink, super::Healthcheck), String> {
        let sink = StatsdSvc::new(self.clone(), acker)?;
        let healthcheck = StatsdSvc::healthcheck(self.clone())?;
        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }
}

impl StatsdSvc {
    pub fn new(config: StatsdSinkConfig, acker: Acker) -> Result<super::RouterSink, String> {
        let batch_size = config.batch_size.unwrap_or(20);
        let batch_timeout = config.batch_timeout.unwrap_or(1);

        let client = Client::new(config.address);
        let service = StatsdSvc { client, config };

        let svc = ServiceBuilder::new().service(service);

        let sink = BatchServiceSink::new(svc, acker).batched_with_min(
            Vec::new(),
            batch_size,
            Duration::from_secs(batch_timeout),
        );

        Ok(Box::new(sink))
    }

    fn healthcheck(_config: StatsdSinkConfig) -> Result<super::Healthcheck, String> {
        Ok(Box::new(future::ok(())))
    }
}

fn encode_tags(tags: &HashMap<String, String>) -> String {
    let mut parts: Vec<_> = tags
        .iter()
        .map(|(name, value)| {
            if value == "true" {
                format!("{}", name)
            } else {
                format!("{}:{}", name, value)
            }
        })
        .collect();
    parts.sort();
    parts.join(",")
}

fn encode_event(event: &Event) -> String {
    let mut buf = Vec::new();

    match event.as_metric() {
        Metric::Counter {
            name, val, tags, ..
        } => {
            buf.push(format!("{}:{}", name, val));
            buf.push("c".to_string());
            if let Some(t) = tags {
                buf.push(format!("#{}", encode_tags(t)));
            };
        }
        Metric::Gauge {
            name,
            val,
            direction,
            tags,
            ..
        } => {
            let val_with_direction = match direction {
                None => format!("{}", val),
                Some(Direction::Plus) => format!("+{}", val),
                Some(Direction::Minus) => format!("-{}", val),
            };
            buf.push(format!("{}:{}", name, val_with_direction));
            buf.push("g".to_string());
            if let Some(t) = tags {
                buf.push(format!("#{}", encode_tags(t)));
            };
        }
        Metric::Histogram {
            name,
            val,
            sample_rate,
            tags,
            ..
        } => {
            buf.push(format!("{}:{}", name, val));
            buf.push("h".to_string());
            if *sample_rate != 1 {
                buf.push(format!("@{}", 1.0 / *sample_rate as f64));
            };
            if let Some(t) = tags {
                buf.push(format!("#{}", encode_tags(t)));
            };
        }
        Metric::Set {
            name, val, tags, ..
        } => {
            buf.push(format!("{}:{}", name, val));
            buf.push("s".to_string());
            if let Some(t) = tags {
                buf.push(format!("#{}", encode_tags(t)));
            };
        }
    };

    buf.join("|")
}

impl Service<Vec<Event>> for StatsdSvc {
    type Response = ();
    type Error = tokio::io::Error;
    type Future = Box<dyn Future<Item = Self::Response, Error = Self::Error> + Send + 'static>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, items: Vec<Event>) -> Self::Future {
        let messages: Vec<_> = items
            .iter()
            .map(encode_event)
            .map(|message| {
                if self.config.namespace.is_empty() {
                    message
                } else {
                    format!("{}.{}", self.config.namespace, message)
                }
            })
            .collect();
        let frame = messages.join("\n");

        self.client.send(frame.as_ref());

        Box::new(future::ok(()))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        buffers::Acker,
        sources::statsd::parser::parse,
        test_util::{collect_n, runtime},
        Event,
    };
    use bytes::Bytes;
    use futures::{stream, stream::Stream, sync::mpsc, Sink};
    use tokio::{
        self,
        codec::BytesCodec,
        net::{UdpFramed, UdpSocket},
    };

    fn tags() -> HashMap<String, String> {
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
    fn test_encode_counter() {
        let metric1 = Metric::Counter {
            name: "counter".to_owned(),
            val: 1.5,
            timestamp: None,
            tags: Some(tags()),
        };
        let event1 = Event::Metric(metric1.clone());
        let metric2 = parse(&encode_event(&event1)).unwrap();
        assert_eq!(metric1, metric2);
    }

    #[test]
    fn test_encode_gauge() {
        let metric1 = Metric::Gauge {
            name: "gauge".to_owned(),
            val: 1.5,
            direction: Some(Direction::Minus),
            timestamp: None,
            tags: Some(tags()),
        };
        let event1 = Event::Metric(metric1.clone());
        let metric2 = parse(&encode_event(&event1)).unwrap();
        assert_eq!(metric1, metric2);
    }

    #[test]
    fn test_encode_histogram() {
        let metric1 = Metric::Histogram {
            name: "histogram".to_owned(),
            val: 1.5,
            sample_rate: 1,
            timestamp: None,
            tags: Some(tags()),
        };
        let event1 = Event::Metric(metric1.clone());
        let metric2 = parse(&encode_event(&event1)).unwrap();
        assert_eq!(metric1, metric2);
    }

    #[test]
    fn test_encode_set() {
        let metric1 = Metric::Set {
            name: "set".to_owned(),
            val: "abc".to_owned(),
            timestamp: None,
            tags: Some(tags()),
        };
        let event1 = Event::Metric(metric1.clone());
        let metric2 = parse(&encode_event(&event1)).unwrap();
        assert_eq!(metric1, metric2);
    }

    #[test]
    fn test_send_to_statsd() {
        let config = StatsdSinkConfig {
            namespace: "vector".into(),
            address: default_address(),
            batch_size: Some(2),
            batch_timeout: Some(1),
        };

        let mut rt = runtime();
        let sink = StatsdSvc::new(config, Acker::Null).unwrap();

        let mut events = Vec::new();
        let event = Event::Metric(Metric::Counter {
            name: "counter".to_owned(),
            val: 1.5,
            timestamp: None,
            tags: Some(tags()),
        });
        events.push(event);

        let event = Event::Metric(Metric::Histogram {
            name: "histogram".to_owned(),
            val: 2.0,
            sample_rate: 100,
            timestamp: None,
            tags: None,
        });
        events.push(event);

        let stream = stream::iter_ok(events.clone().into_iter());
        let sender = sink.send_all(stream);

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

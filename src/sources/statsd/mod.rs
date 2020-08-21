use crate::{
    config::{self, GlobalOptions},
    internal_events::{StatsdEventReceived, StatsdInvalidRecord, StatsdSocketError},
    shutdown::ShutdownSignal,
    Event, Pipeline,
};
use futures::{
    compat::{Future01CompatExt, Sink01CompatExt},
    stream, FutureExt, StreamExt, TryFutureExt,
};
use futures01::Sink;
use parser::parse;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio_util::{codec::BytesCodec, udp::UdpFramed};
use tracing::field;

pub mod parser;

#[derive(Deserialize, Serialize, Debug)]
struct StatsdConfig {
    address: SocketAddr,
}

#[typetag::serde(name = "statsd")]
impl crate::config::SourceConfig for StatsdConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> crate::Result<super::Source> {
        Ok(statsd(self.address, shutdown, out))
    }

    fn output_type(&self) -> crate::config::DataType {
        config::DataType::Metric
    }

    fn source_type(&self) -> &'static str {
        "statsd"
    }
}

fn statsd(addr: SocketAddr, shutdown: ShutdownSignal, out: Pipeline) -> super::Source {
    let out = out.sink_map_err(|e| error!("Error sending metric: {:?}", e));

    Box::new(
        async move {
            let socket = UdpSocket::bind(&addr)
                .map_err(|error| emit!(StatsdSocketError::bind(error)))
                .await?;

            info!(
                message = "Listening.",
                addr = &field::display(addr),
                r#type = "udp"
            );

            let _ = UdpFramed::new(socket, BytesCodec::new())
                .take_until(shutdown.compat())
                .filter_map(|frame| async move {
                    match frame {
                        Ok((bytes, _sock)) => {
                            let packet = String::from_utf8_lossy(bytes.as_ref());
                            let metrics = packet
                                .lines()
                                .filter_map(|line| match parse(line) {
                                    Ok(metric) => {
                                        emit!(StatsdEventReceived {
                                            byte_size: line.len()
                                        });
                                        Some(Ok(Event::Metric(metric)))
                                    }
                                    Err(error) => {
                                        emit!(StatsdInvalidRecord { error, text: line });
                                        None
                                    }
                                })
                                .collect::<Vec<_>>();
                            Some(stream::iter(metrics))
                        }
                        Err(error) => {
                            emit!(StatsdSocketError::read(error));
                            None
                        }
                    }
                })
                .flatten()
                .forward(out.sink_compat())
                .await;

            info!("Finished sending");
            Ok(())
        }
        .boxed()
        .compat(),
    )
}

#[cfg(feature = "sinks-prometheus")]
#[cfg(test)]
mod test {
    use super::StatsdConfig;
    use crate::{
        config,
        sinks::prometheus::PrometheusSinkConfig,
        test_util::{next_addr, start_topology},
    };
    use futures::{compat::Future01CompatExt, TryStreamExt};
    use futures01::Stream;
    use tokio::time::{delay_for, Duration};

    fn parse_count(lines: &[&str], prefix: &str) -> usize {
        lines
            .iter()
            .find(|s| s.starts_with(prefix))
            .map(|s| s.split_whitespace().nth(1).unwrap())
            .unwrap()
            .parse::<usize>()
            .unwrap()
    }

    #[tokio::test]
    async fn test_statsd() {
        let in_addr = next_addr();
        let out_addr = next_addr();

        let mut config = config::Config::empty();
        config.add_source("in", StatsdConfig { address: in_addr });
        config.add_sink(
            "out",
            &["in"],
            PrometheusSinkConfig {
                address: out_addr,
                namespace: "vector".into(),
                buckets: vec![1.0, 2.0, 4.0],
                flush_period_secs: 1,
            },
        );

        let (topology, _crash) = start_topology(config, false).await;

        let bind_addr = next_addr();
        let socket = std::net::UdpSocket::bind(&bind_addr).unwrap();

        for _ in 0..100 {
            socket
                .send_to(
                    b"foo:1|c|#a,b:b\nbar:42|g\nfoo:1|c|#a,b:c\nglork:3|h|@0.1\nmilliglork:3000|ms|@0.1\nset:0|s\nset:1|s\n",
                    &in_addr,
                )
                .unwrap();
            // Space things out slightly to try to avoid dropped packets
            delay_for(Duration::from_millis(10)).await;
        }

        // Give packets some time to flow through
        delay_for(Duration::from_millis(100)).await;

        let client = hyper::Client::new();
        let response = client
            .get(format!("http://{}/metrics", out_addr).parse().unwrap())
            .await
            .unwrap();
        assert!(response.status().is_success());

        let body = response
            .into_body()
            .compat()
            .map(|bytes| bytes.to_vec())
            .concat2()
            .compat()
            .await
            .unwrap();
        let lines = std::str::from_utf8(&body)
            .unwrap()
            .lines()
            .collect::<Vec<_>>();

        // note that prometheus client reorders the labels
        let vector_foo1 = parse_count(&lines, "vector_foo{a=\"true\",b=\"b\"");
        let vector_foo2 = parse_count(&lines, "vector_foo{a=\"true\",b=\"c\"");
        // packets get lost :(
        assert!(vector_foo1 > 90);
        assert!(vector_foo2 > 90);

        let vector_bar = parse_count(&lines, "vector_bar");
        assert_eq!(42, vector_bar);

        assert_eq!(parse_count(&lines, "vector_glork_bucket{le=\"1\"}"), 0);
        assert_eq!(parse_count(&lines, "vector_glork_bucket{le=\"2\"}"), 0);
        assert!(parse_count(&lines, "vector_glork_bucket{le=\"4\"}") > 0);
        assert!(parse_count(&lines, "vector_glork_bucket{le=\"+Inf\"}") > 0);
        let glork_sum = parse_count(&lines, "vector_glork_sum");
        let glork_count = parse_count(&lines, "vector_glork_count");
        assert_eq!(glork_count * 3, glork_sum);

        assert_eq!(parse_count(&lines, "vector_milliglork_bucket{le=\"1\"}"), 0);
        assert_eq!(parse_count(&lines, "vector_milliglork_bucket{le=\"2\"}"), 0);
        assert!(parse_count(&lines, "vector_milliglork_bucket{le=\"4\"}") > 0);
        assert!(parse_count(&lines, "vector_milliglork_bucket{le=\"+Inf\"}") > 0);
        let milliglork_sum = parse_count(&lines, "vector_milliglork_sum");
        let milliglork_count = parse_count(&lines, "vector_milliglork_count");
        assert_eq!(milliglork_count * 3, milliglork_sum);

        // Set test
        // Flush could have occured
        assert!(parse_count(&lines, "vector_set") <= 2);

        // Flush test
        {
            // Wait for flush to happen
            delay_for(Duration::from_millis(2000)).await;

            let response = client
                .get(format!("http://{}/metrics", out_addr).parse().unwrap())
                .await
                .unwrap();
            assert!(response.status().is_success());

            let body = response
                .into_body()
                .compat()
                .map(|bytes| bytes.to_vec())
                .concat2()
                .compat()
                .await
                .unwrap();
            let lines = std::str::from_utf8(&body)
                .unwrap()
                .lines()
                .collect::<Vec<_>>();

            // Check rested
            assert_eq!(parse_count(&lines, "vector_set"), 0);

            // Recheck that set is also reseted------------

            socket.send_to(b"set:0|s\nset:1|s\n", &in_addr).unwrap();
            // Space things out slightly to try to avoid dropped packets
            delay_for(Duration::from_millis(10)).await;
            // Give packets some time to flow through
            delay_for(Duration::from_millis(100)).await;

            let response = client
                .get(format!("http://{}/metrics", out_addr).parse().unwrap())
                .await
                .unwrap();
            assert!(response.status().is_success());

            let body = response
                .into_body()
                .compat()
                .map(|bytes| bytes.to_vec())
                .concat2()
                .compat()
                .await
                .unwrap();
            let lines = std::str::from_utf8(&body)
                .unwrap()
                .lines()
                .collect::<Vec<_>>();

            // Set test
            assert_eq!(parse_count(&lines, "vector_set"), 2);
        }

        // Shut down server
        topology.stop().compat().await.unwrap();
    }
}

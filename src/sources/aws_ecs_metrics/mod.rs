use crate::{
    config::{self, GenerateConfig, GlobalOptions, SourceConfig, SourceDescription},
    internal_events::{
        AwsEcsMetricsErrorResponse, AwsEcsMetricsHttpError, AwsEcsMetricsParseError,
        AwsEcsMetricsReceived, AwsEcsMetricsRequestCompleted,
    },
    shutdown::ShutdownSignal,
    Event, Pipeline,
};
use futures::{compat::Sink01CompatExt, future, stream, FutureExt, StreamExt, TryFutureExt};
use futures01::Sink;
use hyper::{Body, Client, Request};
use serde::{Deserialize, Serialize};
use std::{
    env,
    time::{Duration, Instant},
};

pub mod parser;

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "lowercase")]
enum Version {
    V2,
    V3,
    V4,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
struct AwsEcsMetricsSourceConfig {
    version: Version,
    #[serde(default = "default_scrape_interval_secs")]
    scrape_interval_secs: u64,
}

pub fn default_scrape_interval_secs() -> u64 {
    15
}

inventory::submit! {
    SourceDescription::new::<AwsEcsMetricsSourceConfig>("aws_ecs_metrics")
}

impl GenerateConfig for AwsEcsMetricsSourceConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            version: Version::V4,
            scrape_interval_secs: default_scrape_interval_secs(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_ecs_metrics")]
impl SourceConfig for AwsEcsMetricsSourceConfig {
    async fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> crate::Result<super::Source> {
        let url = match self.version {
            Version::V2 => "169.254.170.2/v2/stats".to_string(),
            Version::V3 => format!("{}/task/stats", env::var("ECS_CONTAINER_METADATA_URI")?),
            Version::V4 => format!("{}/task/stats", env::var("ECS_CONTAINER_METADATA_URI_V4")?),
        };

        Ok(aws_ecs_metrics(
            url,
            self.scrape_interval_secs,
            shutdown,
            out,
        ))
    }

    fn output_type(&self) -> crate::config::DataType {
        config::DataType::Metric
    }

    fn source_type(&self) -> &'static str {
        "aws_ecs_metrics"
    }
}

fn aws_ecs_metrics(
    url: String,
    interval: u64,
    shutdown: ShutdownSignal,
    out: Pipeline,
) -> super::Source {
    let out = out
        .sink_map_err(|e| error!("error sending metric: {:?}", e))
        .sink_compat();
    let task = tokio::time::interval(Duration::from_secs(interval))
        .take_until(shutdown)
        .map(move |_| {
            let client = Client::new();

            let request = Request::get(&url)
                .body(Body::empty())
                .expect("error creating request");

            let start = Instant::now();
            let url2 = url.clone();
            client
                .request(request)
                .and_then(|response| async move {
                    let (header, body) = response.into_parts();
                    let body = hyper::body::to_bytes(body).await?;
                    Ok((header, body))
                })
                .into_stream()
                .filter_map(move |response| {
                    future::ready(match response {
                        Ok((header, body)) if header.status == hyper::StatusCode::OK => {
                            emit!(AwsEcsMetricsRequestCompleted {
                                start,
                                end: Instant::now()
                            });

                            let byte_size = body.len();
                            let body = String::from_utf8_lossy(&body);

                            match parser::parse(&body) {
                                Ok(metrics) => {
                                    emit!(AwsEcsMetricsReceived {
                                        byte_size,
                                        count: metrics.len(),
                                    });
                                    Some(stream::iter(metrics).map(Event::Metric).map(Ok))
                                }
                                Err(error) => {
                                    emit!(AwsEcsMetricsParseError {
                                        error,
                                        url: url2.clone(),
                                        body,
                                    });
                                    None
                                }
                            }
                        }
                        Ok((header, _)) => {
                            emit!(AwsEcsMetricsErrorResponse {
                                code: header.status,
                                url: url2.clone(),
                            });
                            None
                        }
                        Err(error) => {
                            emit!(AwsEcsMetricsHttpError {
                                error,
                                url: url2.clone(),
                            });
                            None
                        }
                    })
                })
                .flatten()
        })
        .flatten()
        .forward(out)
        .inspect(|_| info!("finished sending"));

    Box::new(task.boxed().compat())
}

#[cfg(feature = "sinks-prometheus")]
#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        config,
        sinks::prometheus::PrometheusSinkConfig,
        test_util::{next_addr, start_topology},
        Error,
    };
    use futures::compat::Future01CompatExt;
    use hyper::{
        service::{make_service_fn, service_fn},
        {Body, Client, Response, Server},
    };
    use tokio::time::{delay_for, Duration};

    fn metric_eq(lines: &[&str], name: &str, tag: &str, value: u64) -> bool {
        lines
            .iter()
            .find(|s| s.starts_with(name) && s.contains(tag) && s.ends_with(&value.to_string()))
            .is_some()
    }

    #[tokio::test]
    async fn test_aws_ecs_metrics_source() {
        let in_addr = next_addr();
        let out_addr = next_addr();

        let make_svc = make_service_fn(|_| async {
            Ok::<_, Error>(service_fn(|_| async {
                Ok::<_, Error>(Response::new(Body::from(
                    r##"
                    {
                        "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-3822082590": {
                            "read": "2020-09-23T20:32:26.292561674Z",
                            "preread": "2020-09-23T20:32:21.290708273Z",
                            "pids_stats": {},
                            "blkio_stats": {
                                "io_service_bytes_recursive": [],
                                "io_serviced_recursive": [],
                                "io_queue_recursive": [],
                                "io_service_time_recursive": [],
                                "io_wait_time_recursive": [],
                                "io_merged_recursive": [],
                                "io_time_recursive": [],
                                "sectors_recursive": []
                            },
                            "num_procs": 0,
                            "storage_stats": {},
                            "cpu_stats": {
                                "cpu_usage": {
                                    "total_usage": 863993897,
                                    "percpu_usage": [
                                        607511353,
                                        256482544,
                                        0,
                                        0,
                                        0,
                                        0,
                                        0,
                                        0,
                                        0,
                                        0,
                                        0,
                                        0,
                                        0,
                                        0,
                                        0
                                    ],
                                    "usage_in_kernelmode": 80000000,
                                    "usage_in_usermode": 610000000
                                },
                                "system_cpu_usage": 2007100000000,
                                "online_cpus": 2,
                                "throttling_data": {
                                    "periods": 0,
                                    "throttled_periods": 0,
                                    "throttled_time": 0
                                }
                            },
                            "precpu_stats": {
                                "cpu_usage": {
                                    "total_usage": 0,
                                    "usage_in_kernelmode": 0,
                                    "usage_in_usermode": 0
                                },
                                "throttling_data": {
                                    "periods": 0,
                                    "throttled_periods": 0,
                                    "throttled_time": 0
                                }
                            },
                            "memory_stats": {
                                "usage": 39931904,
                                "max_usage": 40054784,
                                "stats": {
                                    "active_anon": 37457920,
                                    "active_file": 4096,
                                    "cache": 4096,
                                    "dirty": 0,
                                    "hierarchical_memory_limit": 536870912,
                                    "hierarchical_memsw_limit": 9223372036854771712,
                                    "inactive_anon": 0,
                                    "inactive_file": 0,
                                    "mapped_file": 0,
                                    "pgfault": 15745,
                                    "pgmajfault": 0,
                                    "pgpgin": 12086,
                                    "pgpgout": 2940,
                                    "rss": 37457920,
                                    "rss_huge": 0,
                                    "total_active_anon": 37457920,
                                    "total_active_file": 4096,
                                    "total_cache": 4096,
                                    "total_dirty": 0,
                                    "total_inactive_anon": 0,
                                    "total_inactive_file": 0,
                                    "total_mapped_file": 0,
                                    "total_pgfault": 15745,
                                    "total_pgmajfault": 0,
                                    "total_pgpgin": 12086,
                                    "total_pgpgout": 2940,
                                    "total_rss": 37457920,
                                    "total_rss_huge": 0,
                                    "total_unevictable": 0,
                                    "total_writeback": 0,
                                    "unevictable": 0,
                                    "writeback": 0
                                },
                                "limit": 9223372036854771712
                            },
                            "name": "vector1",
                            "id": "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-3822082590",
                            "networks": {
                                "eth1": {
                                    "rx_bytes": 329932716,
                                    "rx_packets": 224158,
                                    "rx_errors": 0,
                                    "rx_dropped": 0,
                                    "tx_bytes": 2001229,
                                    "tx_packets": 29201,
                                    "tx_errors": 0,
                                    "tx_dropped": 0
                                }
                            }
                        },
                        "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352": {
                            "read": "2020-09-23T20:32:26.314100759Z",
                            "preread": "2020-09-23T20:32:21.315056862Z",
                            "pids_stats": {},
                            "blkio_stats": {
                                "io_service_bytes_recursive": [
                                    {
                                        "major": 202,
                                        "minor": 26368,
                                        "op": "Read",
                                        "value": 0
                                    },
                                    {
                                        "major": 202,
                                        "minor": 26368,
                                        "op": "Write",
                                        "value": 520192
                                    },
                                    {
                                        "major": 202,
                                        "minor": 26368,
                                        "op": "Sync",
                                        "value": 516096
                                    },
                                    {
                                        "major": 202,
                                        "minor": 26368,
                                        "op": "Async",
                                        "value": 4096
                                    },
                                    {
                                        "major": 202,
                                        "minor": 26368,
                                        "op": "Total",
                                        "value": 520192
                                    }
                                ],
                                "io_serviced_recursive": [
                                    {
                                        "major": 202,
                                        "minor": 26368,
                                        "op": "Read",
                                        "value": 0
                                    },
                                    {
                                        "major": 202,
                                        "minor": 26368,
                                        "op": "Write",
                                        "value": 10
                                    },
                                    {
                                        "major": 202,
                                        "minor": 26368,
                                        "op": "Sync",
                                        "value": 9
                                    },
                                    {
                                        "major": 202,
                                        "minor": 26368,
                                        "op": "Async",
                                        "value": 1
                                    },
                                    {
                                        "major": 202,
                                        "minor": 26368,
                                        "op": "Total",
                                        "value": 10
                                    }
                                ],
                                "io_queue_recursive": [],
                                "io_service_time_recursive": [],
                                "io_wait_time_recursive": [],
                                "io_merged_recursive": [],
                                "io_time_recursive": [],
                                "sectors_recursive": []
                            },
                            "num_procs": 0,
                            "storage_stats": {},
                            "cpu_stats": {
                                "cpu_usage": {
                                    "total_usage": 2324920942,
                                    "percpu_usage": [
                                        1095931487,
                                        1228989455,
                                        0,
                                        0,
                                        0,
                                        0,
                                        0,
                                        0,
                                        0,
                                        0,
                                        0,
                                        0,
                                        0,
                                        0,
                                        0
                                    ],
                                    "usage_in_kernelmode": 190000000,
                                    "usage_in_usermode": 510000000
                                },
                                "system_cpu_usage": 2007130000000,
                                "online_cpus": 2,
                                "throttling_data": {
                                    "periods": 0,
                                    "throttled_periods": 0,
                                    "throttled_time": 0
                                }
                            },
                            "precpu_stats": {
                                "cpu_usage": {
                                    "total_usage": 0,
                                    "usage_in_kernelmode": 0,
                                    "usage_in_usermode": 0
                                },
                                "throttling_data": {
                                    "periods": 0,
                                    "throttled_periods": 0,
                                    "throttled_time": 0
                                }
                            },
                            "memory_stats": {
                                "usage": 40120320,
                                "max_usage": 47177728,
                                "stats": {
                                    "active_anon": 34885632,
                                    "active_file": 65536,
                                    "cache": 413696,
                                    "dirty": 0,
                                    "hierarchical_memory_limit": 536870912,
                                    "hierarchical_memsw_limit": 9223372036854771712,
                                    "inactive_anon": 4096,
                                    "inactive_file": 344064,
                                    "mapped_file": 4096,
                                    "pgfault": 31131,
                                    "pgmajfault": 0,
                                    "pgpgin": 22360,
                                    "pgpgout": 13742,
                                    "rss": 34885632,
                                    "rss_huge": 0,
                                    "total_active_anon": 34885632,
                                    "total_active_file": 65536,
                                    "total_cache": 413696,
                                    "total_dirty": 0,
                                    "total_inactive_anon": 4096,
                                    "total_inactive_file": 344064,
                                    "total_mapped_file": 4096,
                                    "total_pgfault": 31131,
                                    "total_pgmajfault": 0,
                                    "total_pgpgin": 22360,
                                    "total_pgpgout": 13742,
                                    "total_rss": 34885632,
                                    "total_rss_huge": 0,
                                    "total_unevictable": 0,
                                    "total_writeback": 0,
                                    "unevictable": 0,
                                    "writeback": 0
                                },
                                "limit": 9223372036854771712
                            },
                            "name": "vector2",
                            "id": "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
                            "networks": {
                                "eth1": {
                                    "rx_bytes": 329932716,
                                    "rx_packets": 224158,
                                    "rx_errors": 0,
                                    "rx_dropped": 0,
                                    "tx_bytes": 2001229,
                                    "tx_packets": 29201,
                                    "tx_errors": 0,
                                    "tx_dropped": 0
                                }
                            }
                        }
                    }
                    "##,
                )))
            }))
        });

        tokio::spawn(async move {
            if let Err(e) = Server::bind(&in_addr).serve(make_svc).await {
                error!("server error: {:?}", e);
            }
        });

        env::set_var(
            "ECS_CONTAINER_METADATA_URI_V4",
            format!("http://{}", in_addr),
        );

        let mut config = config::Config::builder();
        config.add_source(
            "in",
            AwsEcsMetricsSourceConfig {
                version: Version::V4,
                scrape_interval_secs: 1,
            },
        );
        config.add_sink(
            "out",
            &["in"],
            PrometheusSinkConfig {
                address: out_addr,
                namespace: None,
                buckets: vec![1.0, 2.0, 4.0],
                quantiles: vec![],
                flush_period_secs: 1,
            },
        );

        let (topology, _crash) = start_topology(config.build().unwrap(), false).await;
        delay_for(Duration::from_secs(1)).await;

        let response = Client::new()
            .get(format!("http://{}/metrics", out_addr).parse().unwrap())
            .await
            .unwrap();
        assert!(response.status().is_success());

        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
        let lines = std::str::from_utf8(&body)
            .unwrap()
            .lines()
            .collect::<Vec<_>>();

        assert!(metric_eq(
            &lines,
            "aws_ecs_blkio_io_service_bytes_recursive",
            "op=\"read\"",
            0
        ));
        assert!(metric_eq(
            &lines,
            "aws_ecs_blkio_io_service_bytes_recursive",
            "op=\"write\"",
            520192
        ));

        assert!(metric_eq(
            &lines,
            "aws_ecs_cpu_total_usage",
            "vector1",
            863993897
        ));
        assert!(metric_eq(
            &lines,
            "aws_ecs_cpu_total_usage",
            "vector2",
            2324920942
        ));

        assert!(metric_eq(
            &lines,
            "aws_ecs_memory_total_pgfault",
            "vector1",
            15745
        ));

        assert!(metric_eq(
            &lines,
            "aws_ecs_network_rx_bytes",
            "eth1",
            329932716
        ));

        topology.stop().compat().await.unwrap();
    }
}

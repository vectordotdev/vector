use std::{env, time::Duration};

use futures::StreamExt;
use hyper::{Body, Request};
use serde_with::serde_as;
use tokio::time;
use tokio_stream::wrappers::IntervalStream;
use vector_lib::configurable::configurable_component;
use vector_lib::internal_event::{ByteSize, BytesReceived, InternalEventHandle as _, Protocol};
use vector_lib::{config::LogNamespace, EstimatedJsonEncodedSizeOf};

use crate::{
    config::{GenerateConfig, SourceConfig, SourceContext, SourceOutput},
    http::HttpClient,
    internal_events::{
        AwsEcsMetricsEventsReceived, AwsEcsMetricsParseError, HttpClientHttpError,
        HttpClientHttpResponseError, StreamClosedError,
    },
    shutdown::ShutdownSignal,
    SourceSender,
};

mod parser;

/// Version of the AWS ECS task metadata endpoint to use.
///
/// More information about the different versions can be found
/// [here][meta_endpoint].
///
/// [meta_endpoint]: https://docs.aws.amazon.com/AmazonECS/latest/developerguide/task-metadata-endpoint.html
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Version {
    /// Version 2.
    ///
    /// More information about version 2 of the task metadata endpoint can be found [here][endpoint_v2].
    ///
    /// [endpoint_v2]: https://docs.aws.amazon.com/AmazonECS/latest/developerguide/task-metadata-endpoint-v2.html
    V2,
    /// Version 3.
    ///
    /// More information about version 3 of the task metadata endpoint can be found [here][endpoint_v3].
    ///
    /// [endpoint_v3]: https://docs.aws.amazon.com/AmazonECS/latest/developerguide/task-metadata-endpoint-v3.html
    V3,
    /// Version 4.
    ///
    /// More information about version 4 of the task metadata endpoint can be found [here][endpoint_v4].
    ///
    /// [endpoint_v4]: https://docs.aws.amazon.com/AmazonECS/latest/developerguide/task-metadata-endpoint-v4.html
    V4,
}

/// Configuration for the `aws_ecs_metrics` source.
#[serde_as]
#[configurable_component(source(
    "aws_ecs_metrics",
    "Collect Docker container stats for tasks running in AWS ECS and AWS Fargate."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct AwsEcsMetricsSourceConfig {
    /// Base URI of the task metadata endpoint.
    ///
    /// If empty, the URI is automatically discovered based on the latest version detected.
    ///
    /// By default:
    /// - The version 4 endpoint base URI is stored in the environment variable `ECS_CONTAINER_METADATA_URI_V4`.
    /// - The version 3 endpoint base URI is stored in the environment variable `ECS_CONTAINER_METADATA_URI`.
    /// - The version 2 endpoint base URI is `169.254.170.2/v2/`.
    #[serde(default = "default_endpoint")]
    endpoint: String,

    /// The version of the task metadata endpoint to use.
    ///
    /// If empty, the version is automatically discovered based on environment variables.
    ///
    /// By default:
    /// - Version 4 is used if the environment variable `ECS_CONTAINER_METADATA_URI_V4` is defined.
    /// - Version 3 is used if the environment variable `ECS_CONTAINER_METADATA_URI_V4` is not defined, but the
    ///   environment variable `ECS_CONTAINER_METADATA_URI` _is_ defined.
    /// - Version 2 is used if neither of the environment variables `ECS_CONTAINER_METADATA_URI_V4` or
    ///   `ECS_CONTAINER_METADATA_URI` are defined.
    #[serde(default = "default_version")]
    version: Version,

    /// The interval between scrapes, in seconds.
    #[serde(default = "default_scrape_interval_secs")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[configurable(metadata(docs::human_name = "Scrape Interval"))]
    scrape_interval_secs: Duration,

    /// The namespace of the metric.
    ///
    /// Disabled if empty.
    #[serde(default = "default_namespace")]
    namespace: String,
}

const METADATA_URI_V4: &str = "ECS_CONTAINER_METADATA_URI";
const METADATA_URI_V3: &str = "ECS_CONTAINER_METADATA_URI_V4";

pub fn default_endpoint() -> String {
    env::var(METADATA_URI_V4)
        .or_else(|_| env::var(METADATA_URI_V3))
        .unwrap_or_else(|_| "http://169.254.170.2/v2".into())
}

pub fn default_version() -> Version {
    if env::var(METADATA_URI_V4).is_ok() {
        Version::V4
    } else if env::var(METADATA_URI_V3).is_ok() {
        Version::V3
    } else {
        Version::V2
    }
}

pub const fn default_scrape_interval_secs() -> Duration {
    Duration::from_secs(15)
}

pub fn default_namespace() -> String {
    "awsecs".to_string()
}

impl AwsEcsMetricsSourceConfig {
    fn stats_endpoint(&self) -> String {
        match self.version {
            Version::V2 => format!("{}/stats", self.endpoint),
            _ => format!("{}/task/stats", self.endpoint),
        }
    }
}

impl GenerateConfig for AwsEcsMetricsSourceConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            endpoint: default_endpoint(),
            version: default_version(),
            scrape_interval_secs: default_scrape_interval_secs(),
            namespace: default_namespace(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_ecs_metrics")]
impl SourceConfig for AwsEcsMetricsSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let namespace = Some(self.namespace.clone()).filter(|namespace| !namespace.is_empty());
        let http_client = HttpClient::new(None, &cx.proxy)?;

        Ok(Box::pin(aws_ecs_metrics(
            http_client,
            self.stats_endpoint(),
            self.scrape_interval_secs,
            namespace,
            cx.out,
            cx.shutdown,
        )))
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        vec![SourceOutput::new_metrics()]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

async fn aws_ecs_metrics(
    http_client: HttpClient,
    url: String,
    interval: Duration,
    namespace: Option<String>,
    mut out: SourceSender,
    shutdown: ShutdownSignal,
) -> Result<(), ()> {
    let mut interval = IntervalStream::new(time::interval(interval)).take_until(shutdown);
    let bytes_received = register!(BytesReceived::from(Protocol::HTTP));
    while interval.next().await.is_some() {
        let request = Request::get(&url)
            .body(Body::empty())
            .expect("error creating request");
        let uri = request.uri().clone();

        match http_client.send(request).await {
            Ok(response) if response.status() == hyper::StatusCode::OK => {
                match hyper::body::to_bytes(response).await {
                    Ok(body) => {
                        bytes_received.emit(ByteSize(body.len()));

                        match parser::parse(body.as_ref(), namespace.clone()) {
                            Ok(metrics) => {
                                let count = metrics.len();
                                emit!(AwsEcsMetricsEventsReceived {
                                    byte_size: metrics.estimated_json_encoded_size_of(),
                                    count,
                                    endpoint: uri.path(),
                                });

                                if (out.send_batch(metrics).await).is_err() {
                                    emit!(StreamClosedError { count });
                                    return Err(());
                                }
                            }
                            Err(error) => {
                                emit!(AwsEcsMetricsParseError {
                                    error,
                                    endpoint: &url,
                                    body: String::from_utf8_lossy(&body),
                                });
                            }
                        }
                    }
                    Err(error) => {
                        emit!(HttpClientHttpError {
                            error: crate::Error::from(error),
                            url: url.to_owned(),
                        });
                    }
                }
            }
            Ok(response) => {
                emit!(HttpClientHttpResponseError {
                    code: response.status(),
                    url: url.to_owned(),
                });
            }
            Err(error) => {
                emit!(HttpClientHttpError {
                    error: crate::Error::from(error),
                    url: url.to_owned(),
                });
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use hyper::{
        service::{make_service_fn, service_fn},
        Body, Response, Server,
    };
    use tokio::time::Duration;

    use super::*;
    use crate::{
        event::MetricValue,
        test_util::{
            components::{run_and_assert_source_compliance, SOURCE_TAGS},
            next_addr, wait_for_tcp,
        },
        Error,
    };

    #[tokio::test]
    async fn test_aws_ecs_metrics_source() {
        let in_addr = next_addr();

        let make_svc = make_service_fn(|_| async {
            Ok::<_, Error>(service_fn(|_| async {
                Ok::<_, Error>(Response::new(Body::from(
                    r#"
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
                    "#,
                )))
            }))
        });

        tokio::spawn(async move {
            if let Err(error) = Server::bind(&in_addr).serve(make_svc).await {
                error!(message = "Server error.", %error);
            }
        });
        wait_for_tcp(in_addr).await;

        let config = AwsEcsMetricsSourceConfig {
            endpoint: format!("http://{}", in_addr),
            version: Version::V4,
            scrape_interval_secs: Duration::from_secs(1),
            namespace: default_namespace(),
        };

        let events =
            run_and_assert_source_compliance(config, Duration::from_secs(1), &SOURCE_TAGS).await;
        assert!(!events.is_empty());

        let metrics = events
            .into_iter()
            .map(|e| e.into_metric())
            .collect::<Vec<_>>();

        match metrics
            .iter()
            .find(|m| m.name() == "network_receive_bytes_total")
        {
            Some(m) => {
                assert_eq!(m.value(), &MetricValue::Counter { value: 329932716.0 });
                assert_eq!(m.namespace(), Some("awsecs"));

                match m.tags() {
                    Some(tags) => assert_eq!(tags.get("device"), Some("eth1")),
                    None => panic!("No tags for metric. {:?}", m),
                }
            }
            None => panic!(
                "Could not find 'network_receive_bytes_total' in {:?}.",
                metrics
            ),
        }
    }
}

#[cfg(feature = "aws-ecs-metrics-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use tokio::time::Duration;

    use super::*;
    use crate::test_util::components::{run_and_assert_source_compliance, SOURCE_TAGS};

    fn ecs_address() -> String {
        env::var("ECS_ADDRESS").unwrap_or_else(|_| "http://localhost:9088".into())
    }

    fn ecs_url(version: &str) -> String {
        format!("{}/{}", ecs_address(), version)
    }

    async fn scrape_metrics(endpoint: String, version: Version) {
        let config = AwsEcsMetricsSourceConfig {
            endpoint,
            version,
            scrape_interval_secs: Duration::from_secs(1),
            namespace: default_namespace(),
        };

        let events =
            run_and_assert_source_compliance(config, Duration::from_secs(5), &SOURCE_TAGS).await;
        assert!(!events.is_empty());
    }

    #[tokio::test]
    async fn scrapes_metrics_v2() {
        scrape_metrics(ecs_url("v2"), Version::V2).await;
    }

    #[tokio::test]
    async fn scrapes_metrics_v3() {
        scrape_metrics(ecs_url("v3"), Version::V3).await;
    }

    #[tokio::test]
    async fn scrapes_metrics_v4() {
        // mock uses same endpoint for v4 as v3
        // https://github.com/awslabs/amazon-ecs-local-container-endpoints/blob/mainline/docs/features.md#task-metadata-v4
        scrape_metrics(ecs_url("v3"), Version::V4).await;
    }
}

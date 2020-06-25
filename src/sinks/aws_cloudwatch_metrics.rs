use crate::{
    dns::Resolver,
    event::metric::{Metric, MetricKind, MetricValue},
    region::RegionOrEndpoint,
    sinks::util::{
        retries2::RetryLogic, rusoto, service2::TowerRequestConfig, BatchEventsConfig, MetricBuffer,
    },
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use chrono::{DateTime, SecondsFormat, Utc};
use futures::{future::BoxFuture, FutureExt, TryFutureExt};
use futures01::Sink;
use lazy_static::lazy_static;
use rusoto_cloudwatch::{
    CloudWatch, CloudWatchClient, Dimension, MetricDatum, PutMetricDataError, PutMetricDataInput,
};
use rusoto_core::{Region, RusotoError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::convert::TryInto;
use std::task::{Context, Poll};
use tower03::Service;

#[derive(Clone)]
pub struct CloudWatchMetricsSvc {
    client: CloudWatchClient,
    config: CloudWatchMetricsSinkConfig,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct CloudWatchMetricsSinkConfig {
    pub namespace: String,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    #[serde(default)]
    pub batch: BatchEventsConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub assume_role: Option<String>,
}

lazy_static! {
    static ref REQUEST_DEFAULTS: TowerRequestConfig = TowerRequestConfig {
        timeout_secs: Some(30),
        rate_limit_num: Some(150),
        ..Default::default()
    };
}

inventory::submit! {
    SinkDescription::new::<CloudWatchMetricsSinkConfig>("aws_cloudwatch_metrics")
}

#[typetag::serde(name = "aws_cloudwatch_metrics")]
impl SinkConfig for CloudWatchMetricsSinkConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let healthcheck = CloudWatchMetricsSvc::healthcheck(self.clone(), cx.resolver())
            .boxed()
            .compat();
        let sink = CloudWatchMetricsSvc::new(self.clone(), cx)?;
        Ok((sink, Box::new(healthcheck)))
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }

    fn sink_type(&self) -> &'static str {
        "aws_cloudwatch_metrics"
    }
}

impl CloudWatchMetricsSvc {
    pub fn new(
        config: CloudWatchMetricsSinkConfig,
        cx: SinkContext,
    ) -> crate::Result<super::RouterSink> {
        let client = Self::create_client(
            (&config.region).try_into()?,
            config.assume_role.clone(),
            cx.resolver(),
        )?;

        let batch = config.batch.unwrap_or(20, 1);
        let request = config.request.unwrap_with(&REQUEST_DEFAULTS);

        let cloudwatch_metrics = CloudWatchMetricsSvc { client, config };

        let sink = request
            .batch_sink(
                CloudWatchMetricsRetryLogic,
                cloudwatch_metrics,
                MetricBuffer::new(),
                batch,
                cx.acker(),
            )
            .sink_map_err(|e| error!("CloudwatchMetrics sink error: {}", e));

        Ok(Box::new(sink))
    }

    async fn healthcheck(
        config: CloudWatchMetricsSinkConfig,
        resolver: Resolver,
    ) -> crate::Result<()> {
        let client = Self::create_client(
            (&config.region).try_into()?,
            config.assume_role.clone(),
            resolver,
        )?;

        let datum = MetricDatum {
            metric_name: "healthcheck".into(),
            value: Some(1.0),
            ..Default::default()
        };
        let request = PutMetricDataInput {
            namespace: config.namespace.clone(),
            metric_data: vec![datum],
        };

        client.put_metric_data(request).await.map_err(Into::into)
    }

    fn create_client(
        region: Region,
        assume_role: Option<String>,
        resolver: Resolver,
    ) -> crate::Result<CloudWatchClient> {
        let region = if cfg!(test) {
            // Moto (used for mocking AWS) doesn't recognize 'custom' as valid region name
            match region {
                Region::Custom { endpoint, .. } => Region::Custom {
                    name: "us-east-1".into(),
                    endpoint,
                },
                _ => panic!("Only Custom regions are supported for CloudWatchClient testing"),
            }
        } else {
            region
        };
        let d = rusoto::client(resolver)?;
        let p = rusoto::AwsCredentialsProvider::new(&region, assume_role)?;

        Ok(CloudWatchClient::new_with(d, p, region))
    }

    fn encode_events(&mut self, events: Vec<Metric>) -> PutMetricDataInput {
        let metric_data: Vec<_> = events
            .into_iter()
            .filter_map(|event| {
                let metric_name = event.name.to_string();
                let timestamp = event.timestamp.map(timestamp_to_string);
                let dimensions = event.tags.clone().map(tags_to_dimensions);
                match event.kind {
                    MetricKind::Incremental => match event.value {
                        MetricValue::Counter { value } => Some(MetricDatum {
                            metric_name,
                            value: Some(value),
                            timestamp,
                            dimensions,
                            ..Default::default()
                        }),
                        MetricValue::Distribution {
                            values,
                            sample_rates,
                        } => Some(MetricDatum {
                            metric_name,
                            values: Some(values.to_vec()),
                            counts: Some(sample_rates.iter().cloned().map(f64::from).collect()),
                            timestamp,
                            dimensions,
                            ..Default::default()
                        }),
                        MetricValue::Set { values } => Some(MetricDatum {
                            metric_name,
                            value: Some(values.len() as f64),
                            timestamp,
                            dimensions,
                            ..Default::default()
                        }),
                        _ => None,
                    },
                    MetricKind::Absolute => match event.value {
                        MetricValue::Gauge { value } => Some(MetricDatum {
                            metric_name,
                            value: Some(value),
                            timestamp,
                            dimensions,
                            ..Default::default()
                        }),
                        _ => None,
                    },
                }
            })
            .collect();

        let namespace = self.config.namespace.clone();

        PutMetricDataInput {
            namespace,
            metric_data,
        }
    }
}

impl Service<Vec<Metric>> for CloudWatchMetricsSvc {
    type Response = ();
    type Error = RusotoError<PutMetricDataError>;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, items: Vec<Metric>) -> Self::Future {
        let client = self.client.clone();
        let input = self.encode_events(items);

        Box::pin(async move {
            if input.metric_data.is_empty() {
                Ok(())
            } else {
                debug!(message = "sending data.", ?input);
                client.put_metric_data(input).await
            }
        })
    }
}

#[derive(Debug, Clone)]
struct CloudWatchMetricsRetryLogic;

impl RetryLogic for CloudWatchMetricsRetryLogic {
    type Error = RusotoError<PutMetricDataError>;
    type Response = ();

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            RusotoError::HttpDispatch(_) => true,
            RusotoError::Service(PutMetricDataError::InternalServiceFault(_)) => true,
            RusotoError::Unknown(res)
                if res.status.is_server_error()
                    || res.status == http::StatusCode::TOO_MANY_REQUESTS =>
            {
                true
            }
            _ => false,
        }
    }
}

fn timestamp_to_string(timestamp: DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn tags_to_dimensions(tags: BTreeMap<String, String>) -> Vec<Dimension> {
    // according to the API, up to 10 dimensions per metric can be provided
    tags.iter()
        .take(10)
        .map(|(k, v)| Dimension {
            name: k.to_string(),
            value: v.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dns::Resolver;
    use crate::event::metric::{Metric, MetricKind, MetricValue};
    use chrono::offset::TimeZone;
    use pretty_assertions::assert_eq;
    use rusoto_cloudwatch::PutMetricDataInput;

    fn config() -> CloudWatchMetricsSinkConfig {
        CloudWatchMetricsSinkConfig {
            namespace: "vector".into(),
            region: RegionOrEndpoint::with_endpoint("local".to_owned()),
            ..Default::default()
        }
    }

    fn svc() -> CloudWatchMetricsSvc {
        let resolver = Resolver;
        let config = config();
        let region = (&config.region).try_into().unwrap();
        let client = CloudWatchMetricsSvc::create_client(region, None, resolver).unwrap();

        CloudWatchMetricsSvc { client, config }
    }

    #[test]
    fn encode_events_basic_counter() {
        let events = vec![
            Metric {
                name: "exception_total".into(),
                timestamp: None,
                tags: None,
                kind: MetricKind::Incremental,
                value: MetricValue::Counter { value: 1.0 },
            },
            Metric {
                name: "bytes_out".into(),
                timestamp: Some(Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 123456789)),
                tags: None,
                kind: MetricKind::Incremental,
                value: MetricValue::Counter { value: 2.5 },
            },
            Metric {
                name: "healthcheck".into(),
                timestamp: Some(Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 123456789)),
                tags: Some(
                    vec![("region".to_owned(), "local".to_owned())]
                        .into_iter()
                        .collect(),
                ),
                kind: MetricKind::Incremental,
                value: MetricValue::Counter { value: 1.0 },
            },
        ];

        assert_eq!(
            svc().encode_events(events),
            PutMetricDataInput {
                namespace: "vector".into(),
                metric_data: vec![
                    MetricDatum {
                        metric_name: "exception_total".into(),
                        value: Some(1.0),
                        ..Default::default()
                    },
                    MetricDatum {
                        metric_name: "bytes_out".into(),
                        value: Some(2.5),
                        timestamp: Some("2018-11-14T08:09:10.123Z".into()),
                        ..Default::default()
                    },
                    MetricDatum {
                        metric_name: "healthcheck".into(),
                        value: Some(1.0),
                        timestamp: Some("2018-11-14T08:09:10.123Z".into()),
                        dimensions: Some(vec![Dimension {
                            name: "region".into(),
                            value: "local".into()
                        }]),
                        ..Default::default()
                    },
                ],
            }
        );
    }

    #[test]
    fn encode_events_absolute_gauge() {
        let events = vec![Metric {
            name: "temperature".into(),
            timestamp: None,
            tags: None,
            kind: MetricKind::Absolute,
            value: MetricValue::Gauge { value: 10.0 },
        }];

        assert_eq!(
            svc().encode_events(events),
            PutMetricDataInput {
                namespace: "vector".into(),
                metric_data: vec![MetricDatum {
                    metric_name: "temperature".into(),
                    value: Some(10.0),
                    ..Default::default()
                }],
            }
        );
    }

    #[test]
    fn encode_events_distribution() {
        let events = vec![Metric {
            name: "latency".into(),
            timestamp: None,
            tags: None,
            kind: MetricKind::Incremental,
            value: MetricValue::Distribution {
                values: vec![11.0, 12.0],
                sample_rates: vec![100, 50],
            },
        }];

        assert_eq!(
            svc().encode_events(events),
            PutMetricDataInput {
                namespace: "vector".into(),
                metric_data: vec![MetricDatum {
                    metric_name: "latency".into(),
                    values: Some(vec![11.0, 12.0]),
                    counts: Some(vec![100.0, 50.0]),
                    ..Default::default()
                }],
            }
        );
    }

    #[test]
    fn encode_events_set() {
        let events = vec![Metric {
            name: "users".into(),
            timestamp: None,
            tags: None,
            kind: MetricKind::Incremental,
            value: MetricValue::Set {
                values: vec!["alice".into(), "bob".into()].into_iter().collect(),
            },
        }];

        assert_eq!(
            svc().encode_events(events),
            PutMetricDataInput {
                namespace: "vector".into(),
                metric_data: vec![MetricDatum {
                    metric_name: "users".into(),
                    value: Some(2.0),
                    ..Default::default()
                }],
            }
        );
    }
}

#[cfg(feature = "aws-cloudwatch-metrics-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::event::Event;
    use crate::region::RegionOrEndpoint;
    use crate::test_util::{random_string, runtime};
    use crate::topology::config::SinkContext;
    use chrono::offset::TimeZone;
    use futures01::{stream, Sink};

    fn config() -> CloudWatchMetricsSinkConfig {
        CloudWatchMetricsSinkConfig {
            namespace: "vector".into(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:4582".to_owned()),
            ..Default::default()
        }
    }

    #[test]
    fn cloudwatch_metrics_healthchecks() {
        let mut rt = runtime();
        let resolver = Resolver;
        let _ = rt.block_on_std(CloudWatchMetricsSvc::healthcheck(config(), resolver));
    }

    #[test]
    fn cloudwatch_metrics_put_data() {
        let mut rt = runtime();
        let cx = SinkContext::new_test(rt.executor());
        let sink = CloudWatchMetricsSvc::new(config(), cx).unwrap();

        let mut events = Vec::new();

        for i in 0..100 {
            let event = Event::Metric(Metric {
                name: format!("counter-{}", 0),
                timestamp: None,
                tags: Some(
                    vec![
                        ("region".to_owned(), "us-west-1".to_owned()),
                        ("production".to_owned(), "true".to_owned()),
                        ("e".to_owned(), "".to_owned()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                kind: MetricKind::Incremental,
                value: MetricValue::Counter { value: i as f64 },
            });
            events.push(event);
        }

        let gauge_name = random_string(10);
        for i in 0..10 {
            let event = Event::Metric(Metric {
                name: format!("gauge-{}", gauge_name),
                timestamp: None,
                tags: None,
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge { value: i as f64 },
            });
            events.push(event);
        }

        let distribution_name = random_string(10);
        for i in 0..10 {
            let event = Event::Metric(Metric {
                name: format!("distribution-{}", distribution_name),
                timestamp: Some(Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 123456789)),
                tags: None,
                kind: MetricKind::Incremental,
                value: MetricValue::Distribution {
                    values: vec![i as f64],
                    sample_rates: vec![100],
                },
            });
            events.push(event);
        }

        let stream = stream::iter_ok(events.clone().into_iter());

        let pump = sink.send_all(stream);
        let _ = rt.block_on(pump).unwrap();
    }
}

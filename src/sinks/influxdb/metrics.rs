use crate::{
    config::{DataType, SinkConfig, SinkContext, SinkDescription},
    event::metric::{Metric, MetricValue, StatisticKind},
    sinks::{
        influxdb::{
            encode_namespace, encode_timestamp, healthcheck, influx_line_protocol,
            influxdb_settings, Field, InfluxDB1Settings, InfluxDB2Settings, ProtocolVersion,
        },
        util::{
            http::{HttpBatchService, HttpClient, HttpRetryLogic},
            statistic::DistributionStatistic,
            BatchConfig, BatchSettings, MetricBuffer, TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
};
use bytes::Bytes;
use futures::future::{ready, BoxFuture};
use futures01::Sink;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::task::Poll;
use tower::Service;

#[derive(Clone)]
struct InfluxDBSvc {
    config: InfluxDBConfig,
    protocol_version: ProtocolVersion,
    inner: HttpBatchService<BoxFuture<'static, crate::Result<hyper::Request<Vec<u8>>>>>,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct InfluxDBConfig {
    pub namespace: Option<String>,
    pub endpoint: String,
    #[serde(flatten)]
    pub influxdb1_settings: Option<InfluxDB1Settings>,
    #[serde(flatten)]
    pub influxdb2_settings: Option<InfluxDB2Settings>,
    #[serde(default)]
    pub batch: BatchConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
}

lazy_static! {
    static ref REQUEST_DEFAULTS: TowerRequestConfig = TowerRequestConfig {
        retry_attempts: Some(5),
        ..Default::default()
    };
}

// https://v2.docs.influxdata.com/v2.0/write-data/#influxdb-api
#[derive(Debug, Clone, PartialEq, Serialize)]
struct InfluxDBRequest {
    series: Vec<String>,
}

inventory::submit! {
    SinkDescription::new::<InfluxDBConfig>("influxdb_metrics")
}

impl_generate_config_from_default!(InfluxDBConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "influxdb_metrics")]
impl SinkConfig for InfluxDBConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = HttpClient::new(cx.resolver(), None)?;
        let healthcheck = healthcheck(
            self.clone().endpoint,
            self.clone().influxdb1_settings,
            self.clone().influxdb2_settings,
            client.clone(),
        )?;
        let sink = InfluxDBSvc::new(self.clone(), cx, client)?;
        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }

    fn sink_type(&self) -> &'static str {
        "influxdb_metrics"
    }
}

impl InfluxDBSvc {
    pub fn new(
        config: InfluxDBConfig,
        cx: SinkContext,
        client: HttpClient,
    ) -> crate::Result<VectorSink> {
        let settings = influxdb_settings(
            config.influxdb1_settings.clone(),
            config.influxdb2_settings.clone(),
        )?;

        let endpoint = config.endpoint.clone();
        let token = settings.token();
        let protocol_version = settings.protocol_version();

        let batch = BatchSettings::default()
            .events(20)
            .timeout(1)
            .parse_config(config.batch)?;
        let request = config.request.unwrap_with(&REQUEST_DEFAULTS);

        let uri = settings.write_uri(endpoint)?;

        let http_service = HttpBatchService::new(client, create_build_request(uri, token));

        let influxdb_http_service = InfluxDBSvc {
            config,
            protocol_version,
            inner: http_service,
        };

        let sink = request
            .batch_sink(
                HttpRetryLogic,
                influxdb_http_service,
                MetricBuffer::new(batch.size),
                batch.timeout,
                cx.acker(),
            )
            .sink_map_err(|e| error!("Fatal influxdb sink error: {}", e));

        Ok(VectorSink::Futures01Sink(Box::new(sink)))
    }
}

impl Service<Vec<Metric>> for InfluxDBSvc {
    type Response = http::Response<Bytes>;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut std::task::Context) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, items: Vec<Metric>) -> Self::Future {
        let input = encode_events(
            self.protocol_version,
            items,
            self.config.namespace.as_deref(),
        );
        let body: Vec<u8> = input.into_bytes();

        self.inner.call(body)
    }
}

fn create_build_request(
    uri: http::Uri,
    token: String,
) -> impl Fn(Vec<u8>) -> BoxFuture<'static, crate::Result<hyper::Request<Vec<u8>>>> + Sync + Send + 'static
{
    let auth = format!("Token {}", token);
    move |body| {
        Box::pin(ready(
            hyper::Request::post(uri.clone())
                .header("Content-Type", "text/plain")
                .header("Authorization", auth.clone())
                .body(body)
                .map_err(Into::into),
        ))
    }
}

fn encode_events(
    protocol_version: ProtocolVersion,
    events: Vec<Metric>,
    namespace: Option<&str>,
) -> String {
    let mut output = String::new();
    for event in events.into_iter() {
        let fullname = encode_namespace(namespace, '.', &event.name);
        let ts = encode_timestamp(event.timestamp);
        let tags = event.tags.clone();
        match event.value {
            MetricValue::Counter { value } => {
                let fields = to_fields(value);

                influx_line_protocol(
                    protocol_version,
                    fullname,
                    "counter",
                    tags,
                    Some(fields),
                    ts,
                    &mut output,
                )
            }
            MetricValue::Gauge { value } => {
                let fields = to_fields(value);

                influx_line_protocol(
                    protocol_version,
                    fullname,
                    "gauge",
                    tags,
                    Some(fields),
                    ts,
                    &mut output,
                );
            }
            MetricValue::Set { values } => {
                let fields = to_fields(values.len() as f64);

                influx_line_protocol(
                    protocol_version,
                    fullname,
                    "set",
                    tags,
                    Some(fields),
                    ts,
                    &mut output,
                );
            }
            MetricValue::AggregatedHistogram {
                buckets,
                counts,
                count,
                sum,
            } => {
                let mut fields: HashMap<String, Field> = buckets
                    .iter()
                    .zip(counts.iter())
                    .map(|pair| (format!("bucket_{}", pair.0), Field::UnsignedInt(*pair.1)))
                    .collect();
                fields.insert("count".to_owned(), Field::UnsignedInt(count));
                fields.insert("sum".to_owned(), Field::Float(sum));

                influx_line_protocol(
                    protocol_version,
                    fullname,
                    "histogram",
                    tags,
                    Some(fields),
                    ts,
                    &mut output,
                );
            }
            MetricValue::AggregatedSummary {
                quantiles,
                values,
                count,
                sum,
            } => {
                let mut fields: HashMap<String, Field> = quantiles
                    .iter()
                    .zip(values.iter())
                    .map(|pair| (format!("quantile_{}", pair.0), Field::Float(*pair.1)))
                    .collect();
                fields.insert("count".to_owned(), Field::UnsignedInt(count));
                fields.insert("sum".to_owned(), Field::Float(sum));

                influx_line_protocol(
                    protocol_version,
                    fullname,
                    "summary",
                    tags,
                    Some(fields),
                    ts,
                    &mut output,
                );
            }
            MetricValue::Distribution {
                values,
                sample_rates,
                statistic,
            } => {
                let quantiles = match statistic {
                    StatisticKind::Histogram => &[0.95] as &[_],
                    StatisticKind::Summary => &[0.5, 0.75, 0.9, 0.95, 0.99] as &[_],
                };
                let fields = encode_distribution(&values, &sample_rates, quantiles);

                influx_line_protocol(
                    protocol_version,
                    fullname,
                    "distribution",
                    tags,
                    fields,
                    ts,
                    &mut output,
                );
            }
        }
    }

    // remove last '\n'
    output.pop();
    output
}

fn encode_distribution(
    values: &[f64],
    counts: &[u32],
    quantiles: &[f64],
) -> Option<HashMap<String, Field>> {
    let statistic = DistributionStatistic::new(values, counts, quantiles)?;

    let fields: HashMap<String, Field> = vec![
        ("min".to_owned(), Field::Float(statistic.min)),
        ("max".to_owned(), Field::Float(statistic.max)),
        ("median".to_owned(), Field::Float(statistic.median)),
        ("avg".to_owned(), Field::Float(statistic.avg)),
        ("sum".to_owned(), Field::Float(statistic.sum)),
        ("count".to_owned(), Field::Float(statistic.count as f64)),
    ]
    .into_iter()
    .chain(
        statistic
            .quantiles
            .iter()
            .map(|&(p, val)| (format!("quantile_{:.2}", p), Field::Float(val))),
    )
    .collect();

    Some(fields)
}

fn to_fields(value: f64) -> HashMap<String, Field> {
    let fields: HashMap<String, Field> = vec![("value".to_owned(), Field::Float(value))]
        .into_iter()
        .collect();
    fields
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::metric::{Metric, MetricKind, MetricValue, StatisticKind};
    use crate::sinks::influxdb::test_util::{assert_fields, split_line_protocol, tags, ts};
    use pretty_assertions::assert_eq;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<InfluxDBConfig>();
    }

    #[test]
    fn test_encode_counter() {
        let events = vec![
            Metric {
                name: "total".into(),
                timestamp: Some(ts()),
                tags: None,
                kind: MetricKind::Incremental,
                value: MetricValue::Counter { value: 1.5 },
            },
            Metric {
                name: "check".into(),
                timestamp: Some(ts()),
                tags: Some(tags()),
                kind: MetricKind::Incremental,
                value: MetricValue::Counter { value: 1.0 },
            },
        ];

        let line_protocols = encode_events(ProtocolVersion::V2, events, Some("ns"));
        assert_eq!(
            line_protocols,
            "ns.total,metric_type=counter value=1.5 1542182950000000011\n\
            ns.check,metric_type=counter,normal_tag=value,true_tag=true value=1 1542182950000000011"
        );
    }

    #[test]
    fn test_encode_gauge() {
        let events = vec![Metric {
            name: "meter".to_owned(),
            timestamp: Some(ts()),
            tags: Some(tags()),
            kind: MetricKind::Incremental,
            value: MetricValue::Gauge { value: -1.5 },
        }];

        let line_protocols = encode_events(ProtocolVersion::V2, events, Some("ns"));
        assert_eq!(
            line_protocols,
            "ns.meter,metric_type=gauge,normal_tag=value,true_tag=true value=-1.5 1542182950000000011"
        );
    }

    #[test]
    fn test_encode_set() {
        let events = vec![Metric {
            name: "users".into(),
            timestamp: Some(ts()),
            tags: Some(tags()),
            kind: MetricKind::Incremental,
            value: MetricValue::Set {
                values: vec!["alice".into(), "bob".into()].into_iter().collect(),
            },
        }];

        let line_protocols = encode_events(ProtocolVersion::V2, events, Some("ns"));
        assert_eq!(
            line_protocols,
            "ns.users,metric_type=set,normal_tag=value,true_tag=true value=2 1542182950000000011"
        );
    }

    #[test]
    fn test_encode_histogram_v1() {
        let events = vec![Metric {
            name: "requests".to_owned(),
            timestamp: Some(ts()),
            tags: Some(tags()),
            kind: MetricKind::Absolute,
            value: MetricValue::AggregatedHistogram {
                buckets: vec![1.0, 2.1, 3.0],
                counts: vec![1, 2, 3],
                count: 6,
                sum: 12.5,
            },
        }];

        let line_protocols = encode_events(ProtocolVersion::V1, events, Some("ns"));
        let line_protocols: Vec<&str> = line_protocols.split('\n').collect();
        assert_eq!(line_protocols.len(), 1);

        let line_protocol1 = split_line_protocol(line_protocols[0]);
        assert_eq!("ns.requests", line_protocol1.0);
        assert_eq!(
            "metric_type=histogram,normal_tag=value,true_tag=true",
            line_protocol1.1
        );
        assert_fields(
            line_protocol1.2.to_string(),
            [
                "bucket_1=1i",
                "bucket_2.1=2i",
                "bucket_3=3i",
                "count=6i",
                "sum=12.5",
            ]
            .to_vec(),
        );
        assert_eq!("1542182950000000011", line_protocol1.3);
    }

    #[test]
    fn test_encode_histogram() {
        let events = vec![Metric {
            name: "requests".to_owned(),
            timestamp: Some(ts()),
            tags: Some(tags()),
            kind: MetricKind::Absolute,
            value: MetricValue::AggregatedHistogram {
                buckets: vec![1.0, 2.1, 3.0],
                counts: vec![1, 2, 3],
                count: 6,
                sum: 12.5,
            },
        }];

        let line_protocols = encode_events(ProtocolVersion::V2, events, Some("ns"));
        let line_protocols: Vec<&str> = line_protocols.split('\n').collect();
        assert_eq!(line_protocols.len(), 1);

        let line_protocol1 = split_line_protocol(line_protocols[0]);
        assert_eq!("ns.requests", line_protocol1.0);
        assert_eq!(
            "metric_type=histogram,normal_tag=value,true_tag=true",
            line_protocol1.1
        );
        assert_fields(
            line_protocol1.2.to_string(),
            [
                "bucket_1=1u",
                "bucket_2.1=2u",
                "bucket_3=3u",
                "count=6u",
                "sum=12.5",
            ]
            .to_vec(),
        );
        assert_eq!("1542182950000000011", line_protocol1.3);
    }

    #[test]
    fn test_encode_summary_v1() {
        let events = vec![Metric {
            name: "requests_sum".to_owned(),
            timestamp: Some(ts()),
            tags: Some(tags()),
            kind: MetricKind::Absolute,
            value: MetricValue::AggregatedSummary {
                quantiles: vec![0.01, 0.5, 0.99],
                values: vec![1.5, 2.0, 3.0],
                count: 6,
                sum: 12.0,
            },
        }];

        let line_protocols = encode_events(ProtocolVersion::V1, events, Some("ns"));
        let line_protocols: Vec<&str> = line_protocols.split('\n').collect();
        assert_eq!(line_protocols.len(), 1);

        let line_protocol1 = split_line_protocol(line_protocols[0]);
        assert_eq!("ns.requests_sum", line_protocol1.0);
        assert_eq!(
            "metric_type=summary,normal_tag=value,true_tag=true",
            line_protocol1.1
        );
        assert_fields(
            line_protocol1.2.to_string(),
            [
                "count=6i",
                "quantile_0.01=1.5",
                "quantile_0.5=2",
                "quantile_0.99=3",
                "sum=12",
            ]
            .to_vec(),
        );
        assert_eq!("1542182950000000011", line_protocol1.3);
    }

    #[test]
    fn test_encode_summary() {
        let events = vec![Metric {
            name: "requests_sum".to_owned(),
            timestamp: Some(ts()),
            tags: Some(tags()),
            kind: MetricKind::Absolute,
            value: MetricValue::AggregatedSummary {
                quantiles: vec![0.01, 0.5, 0.99],
                values: vec![1.5, 2.0, 3.0],
                count: 6,
                sum: 12.0,
            },
        }];

        let line_protocols = encode_events(ProtocolVersion::V2, events, Some("ns"));
        let line_protocols: Vec<&str> = line_protocols.split('\n').collect();
        assert_eq!(line_protocols.len(), 1);

        let line_protocol1 = split_line_protocol(line_protocols[0]);
        assert_eq!("ns.requests_sum", line_protocol1.0);
        assert_eq!(
            "metric_type=summary,normal_tag=value,true_tag=true",
            line_protocol1.1
        );
        assert_fields(
            line_protocol1.2.to_string(),
            [
                "count=6u",
                "quantile_0.01=1.5",
                "quantile_0.5=2",
                "quantile_0.99=3",
                "sum=12",
            ]
            .to_vec(),
        );
        assert_eq!("1542182950000000011", line_protocol1.3);
    }

    #[test]
    fn test_encode_distribution() {
        let events = vec![
            Metric {
                name: "requests".into(),
                timestamp: Some(ts()),
                tags: Some(tags()),
                kind: MetricKind::Incremental,
                value: MetricValue::Distribution {
                    values: vec![1.0, 2.0, 3.0],
                    sample_rates: vec![3, 3, 2],
                    statistic: StatisticKind::Histogram,
                },
            },
            Metric {
                name: "dense_stats".into(),
                timestamp: Some(ts()),
                tags: None,
                kind: MetricKind::Incremental,
                value: MetricValue::Distribution {
                    values: (0..20).map(f64::from).collect::<Vec<_>>(),
                    sample_rates: vec![1; 20],
                    statistic: StatisticKind::Histogram,
                },
            },
            Metric {
                name: "sparse_stats".into(),
                timestamp: Some(ts()),
                tags: None,
                kind: MetricKind::Incremental,
                value: MetricValue::Distribution {
                    values: (1..5).map(f64::from).collect::<Vec<_>>(),
                    sample_rates: (1..5).collect::<Vec<_>>(),
                    statistic: StatisticKind::Histogram,
                },
            },
        ];

        let line_protocols = encode_events(ProtocolVersion::V2, events, Some("ns"));
        let line_protocols: Vec<&str> = line_protocols.split('\n').collect();
        assert_eq!(line_protocols.len(), 3);

        let line_protocol1 = split_line_protocol(line_protocols[0]);
        assert_eq!("ns.requests", line_protocol1.0);
        assert_eq!(
            "metric_type=distribution,normal_tag=value,true_tag=true",
            line_protocol1.1
        );
        assert_fields(
            line_protocol1.2.to_string(),
            [
                "avg=1.875",
                "count=8",
                "max=3",
                "median=2",
                "min=1",
                "quantile_0.95=3",
                "sum=15",
            ]
            .to_vec(),
        );
        assert_eq!("1542182950000000011", line_protocol1.3);

        let line_protocol2 = split_line_protocol(line_protocols[1]);
        assert_eq!("ns.dense_stats", line_protocol2.0);
        assert_eq!("metric_type=distribution", line_protocol2.1);
        assert_fields(
            line_protocol2.2.to_string(),
            [
                "avg=9.5",
                "count=20",
                "max=19",
                "median=9",
                "min=0",
                "quantile_0.95=18",
                "sum=190",
            ]
            .to_vec(),
        );
        assert_eq!("1542182950000000011", line_protocol2.3);

        let line_protocol3 = split_line_protocol(line_protocols[2]);
        assert_eq!("ns.sparse_stats", line_protocol3.0);
        assert_eq!("metric_type=distribution", line_protocol3.1);
        assert_fields(
            line_protocol3.2.to_string(),
            [
                "avg=3",
                "count=10",
                "max=4",
                "median=3",
                "min=1",
                "quantile_0.95=4",
                "sum=30",
            ]
            .to_vec(),
        );
        assert_eq!("1542182950000000011", line_protocol3.3);
    }

    #[test]
    fn test_encode_distribution_empty_stats() {
        let events = vec![Metric {
            name: "requests".into(),
            timestamp: Some(ts()),
            tags: Some(tags()),
            kind: MetricKind::Incremental,
            value: MetricValue::Distribution {
                values: vec![],
                sample_rates: vec![],
                statistic: StatisticKind::Histogram,
            },
        }];

        let line_protocols = encode_events(ProtocolVersion::V2, events, Some("ns"));
        assert_eq!(line_protocols.len(), 0);
    }

    #[test]
    fn test_encode_distribution_zero_counts_stats() {
        let events = vec![Metric {
            name: "requests".into(),
            timestamp: Some(ts()),
            tags: Some(tags()),
            kind: MetricKind::Incremental,
            value: MetricValue::Distribution {
                values: vec![1.0, 2.0],
                sample_rates: vec![0, 0],
                statistic: StatisticKind::Histogram,
            },
        }];

        let line_protocols = encode_events(ProtocolVersion::V2, events, Some("ns"));
        assert_eq!(line_protocols.len(), 0);
    }

    #[test]
    fn test_encode_distribution_unequal_stats() {
        let events = vec![Metric {
            name: "requests".into(),
            timestamp: Some(ts()),
            tags: Some(tags()),
            kind: MetricKind::Incremental,
            value: MetricValue::Distribution {
                values: vec![1.0],
                sample_rates: vec![1, 2, 3],
                statistic: StatisticKind::Histogram,
            },
        }];

        let line_protocols = encode_events(ProtocolVersion::V2, events, Some("ns"));
        assert_eq!(line_protocols.len(), 0);
    }

    #[test]
    fn test_encode_distribution_summary() {
        let events = vec![Metric {
            name: "requests".into(),
            timestamp: Some(ts()),
            tags: Some(tags()),
            kind: MetricKind::Incremental,
            value: MetricValue::Distribution {
                values: vec![1.0, 2.0, 3.0],
                sample_rates: vec![3, 3, 2],
                statistic: StatisticKind::Summary,
            },
        }];

        let line_protocols = encode_events(ProtocolVersion::V2, events, Some("ns"));
        let line_protocols: Vec<&str> = line_protocols.split('\n').collect();
        assert_eq!(line_protocols.len(), 1);

        let line_protocol = split_line_protocol(line_protocols[0]);
        assert_eq!("ns.requests", line_protocol.0);
        assert_eq!(
            "metric_type=distribution,normal_tag=value,true_tag=true",
            line_protocol.1
        );
        assert_fields(
            line_protocol.2.to_string(),
            [
                "avg=1.875",
                "count=8",
                "max=3",
                "median=2",
                "min=1",
                "sum=15",
                "quantile_0.50=2",
                "quantile_0.75=2",
                "quantile_0.90=3",
                "quantile_0.95=3",
                "quantile_0.99=3",
            ]
            .to_vec(),
        );
        assert_eq!("1542182950000000011", line_protocol.3);
    }
}

#[cfg(feature = "influxdb-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use crate::{
        config::SinkContext,
        event::metric::{Metric, MetricKind, MetricValue},
        sinks::{
            influxdb::{
                metrics::{InfluxDBConfig, InfluxDBSvc},
                test_util::{onboarding_v2, BUCKET, ORG, TOKEN},
                InfluxDB2Settings,
            },
            util::http::HttpClient,
        },
        Event,
    };
    use chrono::Utc;
    use futures::stream;

    //    fn onboarding_v1() {
    //        let client = reqwest::Client::builder()
    //            .danger_accept_invalid_certs(true)
    //            .build()
    //            .unwrap();
    //
    //        let res = client
    //            .get("http://localhost:8086/query")
    //            .query(&[("q", "CREATE DATABASE my-database")])
    //            .send()
    //            .unwrap();
    //
    //        let status = res.status();
    //
    //        assert!(
    //            status == http::StatusCode::OK,
    //            format!("UnexpectedStatus: {}", status)
    //        );
    //    }

    #[tokio::test]
    async fn influxdb2_metrics_put_data() {
        onboarding_v2().await;

        let cx = SinkContext::new_test();

        let config = InfluxDBConfig {
            namespace: Some("ns".to_string()),
            endpoint: "http://localhost:9999".to_string(),
            influxdb1_settings: None,
            influxdb2_settings: Some(InfluxDB2Settings {
                org: ORG.to_string(),
                bucket: BUCKET.to_string(),
                token: TOKEN.to_string(),
            }),
            batch: Default::default(),
            request: Default::default(),
        };

        let metric = format!("counter-{}", Utc::now().timestamp_nanos());
        let mut events = Vec::new();
        for i in 0..10 {
            let event = Event::Metric(Metric {
                name: metric.to_string(),
                timestamp: None,
                tags: Some(
                    vec![
                        ("region".to_owned(), "us-west-1".to_owned()),
                        ("production".to_owned(), "true".to_owned()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                kind: MetricKind::Incremental,
                value: MetricValue::Counter { value: i as f64 },
            });
            events.push(event);
        }

        let client = HttpClient::new(cx.resolver(), None).unwrap();
        let sink = InfluxDBSvc::new(config, cx, client).unwrap();
        sink.run(stream::iter(events)).await.unwrap();

        let mut body = std::collections::HashMap::new();
        body.insert("query", format!("from(bucket:\"my-bucket\") |> range(start: 0) |> filter(fn: (r) => r._measurement == \"ns.{}\")", metric));
        body.insert("type", "flux".to_owned());

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();

        let res = client
            .post("http://localhost:9999/api/v2/query?org=my-org")
            .json(&body)
            .header("accept", "application/json")
            .header("Authorization", "Token my-token")
            .send()
            .await
            .unwrap();
        let string = res.text().await.unwrap();

        let lines = string.split('\n').collect::<Vec<&str>>();
        let header = lines[0].split(',').collect::<Vec<&str>>();
        let record = lines[1].split(',').collect::<Vec<&str>>();

        assert_eq!(
            record[header
                .iter()
                .position(|&r| r.trim() == "metric_type")
                .unwrap()]
            .trim(),
            "counter"
        );
        assert_eq!(
            record[header
                .iter()
                .position(|&r| r.trim() == "production")
                .unwrap()]
            .trim(),
            "true"
        );
        assert_eq!(
            record[header.iter().position(|&r| r.trim() == "region").unwrap()].trim(),
            "us-west-1"
        );
        assert_eq!(
            record[header
                .iter()
                .position(|&r| r.trim() == "_measurement")
                .unwrap()]
            .trim(),
            format!("ns.{}", metric)
        );
        assert_eq!(
            record[header.iter().position(|&r| r.trim() == "_field").unwrap()].trim(),
            "value"
        );
        assert_eq!(
            record[header.iter().position(|&r| r.trim() == "_value").unwrap()].trim(),
            "45"
        );
    }
}

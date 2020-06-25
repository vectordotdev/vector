use crate::{
    dns::Resolver,
    event::{
        metric::{Metric, MetricKind, MetricValue},
        Event,
    },
    sinks::util::{
        http::{BatchedHttpSink, HttpClient, HttpSink},
        service2::TowerRequestConfig,
        BatchEventsConfig, MetricBuffer,
    },
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use chrono::{DateTime, Utc};
use futures::{FutureExt, TryFutureExt};
use futures01::Sink;
use http::{uri::InvalidUri, Request, StatusCode, Uri};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicI64, Ordering::SeqCst};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Invalid host {:?}: {:?}", host, source))]
    InvalidHost { host: String, source: InvalidUri },
}

#[derive(Clone)]
struct DatadogState {
    last_sent_timestamp: i64,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct DatadogConfig {
    pub namespace: String,
    #[serde(default = "default_host")]
    pub host: String,
    pub api_key: String,
    #[serde(default)]
    pub batch: BatchEventsConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
}

struct DatadogSink {
    config: DatadogConfig,
    last_sent_timestamp: AtomicI64,
    uri: Uri,
}

lazy_static! {
    static ref REQUEST_DEFAULTS: TowerRequestConfig = TowerRequestConfig {
        retry_attempts: Some(5),
        ..Default::default()
    };
}

// https://docs.datadoghq.com/api/?lang=bash#post-timeseries-points
#[derive(Debug, Clone, PartialEq, Serialize)]
struct DatadogRequest {
    series: Vec<DatadogMetric>,
}

pub fn default_host() -> String {
    String::from("https://api.datadoghq.com")
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct DatadogMetric {
    metric: String,
    r#type: DatadogMetricType,
    interval: Option<i64>,
    points: Vec<DatadogPoint>,
    tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DatadogMetricType {
    Gauge,
    Count,
    Rate,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct DatadogPoint(i64, f64);

#[derive(Debug, Clone, PartialEq)]
struct DatadogStats {
    min: f64,
    max: f64,
    median: f64,
    avg: f64,
    sum: f64,
    count: f64,
    quantiles: Vec<(f64, f64)>,
}

inventory::submit! {
    SinkDescription::new::<DatadogConfig>("datadog_metrics")
}

#[typetag::serde(name = "datadog_metrics")]
impl SinkConfig for DatadogConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let healthcheck = healthcheck(self.clone(), cx.resolver()).boxed().compat();

        let batch = self.batch.unwrap_or(20, 1);
        let request = self.request.unwrap_with(&REQUEST_DEFAULTS);

        let uri = build_uri(&self.host)?;
        let timestamp = Utc::now().timestamp();

        let sink = DatadogSink {
            config: self.clone(),
            uri,
            last_sent_timestamp: AtomicI64::new(timestamp),
        };

        let sink = BatchedHttpSink::new(sink, MetricBuffer::new(), request, batch, None, &cx)
            .sink_map_err(|e| error!("Fatal datadog error: {}", e));

        Ok((Box::new(sink), Box::new(healthcheck)))
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }

    fn sink_type(&self) -> &'static str {
        "datadog_metrics"
    }
}

#[async_trait::async_trait]
impl HttpSink for DatadogSink {
    type Input = Event;
    type Output = Vec<Metric>;

    fn encode_event(&self, event: Event) -> Option<Self::Input> {
        Some(event)
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<Request<Vec<u8>>> {
        let now = Utc::now().timestamp();
        let interval = now - self.last_sent_timestamp.load(SeqCst);
        self.last_sent_timestamp.store(now, SeqCst);

        let input = encode_events(events, interval, &self.config.namespace);
        let body = serde_json::to_vec(&input).unwrap();

        Request::post(self.uri.clone())
            .header("Content-Type", "application/json")
            .header("DD-API-KEY", self.config.api_key.clone())
            .body(body)
            .map_err(Into::into)
    }
}

fn build_uri(host: &str) -> crate::Result<Uri> {
    let uri = format!("{}/api/v1/series", host)
        .parse::<Uri>()
        .context(super::UriParseError)?;

    Ok(uri)
}

async fn healthcheck(config: DatadogConfig, resolver: Resolver) -> crate::Result<()> {
    let uri = format!("{}/api/v1/validate", config.host)
        .parse::<Uri>()
        .context(super::UriParseError)?;

    let request = Request::get(uri)
        .header("DD-API-KEY", config.api_key)
        .body(hyper::Body::empty())
        .unwrap();

    let mut client = HttpClient::new(resolver, None)?;
    let response = client.send(request).await?;

    match response.status() {
        StatusCode::OK => Ok(()),
        other => Err(super::HealthcheckError::UnexpectedStatus { status: other }.into()),
    }
}

fn encode_tags(tags: BTreeMap<String, String>) -> Vec<String> {
    let mut pairs: Vec<_> = tags
        .iter()
        .map(|(name, value)| format!("{}:{}", name, value))
        .collect();
    pairs.sort();
    pairs
}

fn encode_timestamp(timestamp: Option<DateTime<Utc>>) -> i64 {
    if let Some(ts) = timestamp {
        ts.timestamp()
    } else {
        Utc::now().timestamp()
    }
}

fn encode_namespace(namespace: &str, name: &str) -> String {
    if !namespace.is_empty() {
        format!("{}.{}", namespace, name)
    } else {
        name.to_string()
    }
}

fn stats(values: &[f64], counts: &[u32]) -> Option<DatadogStats> {
    if values.len() != counts.len() {
        return None;
    }

    let mut samples = Vec::new();
    for (v, c) in values.iter().zip(counts.iter()) {
        for _ in 0..*c {
            samples.push(*v);
        }
    }

    if samples.is_empty() {
        return None;
    }

    if samples.len() == 1 {
        let val = samples[0];
        return Some(DatadogStats {
            min: val,
            max: val,
            median: val,
            avg: val,
            sum: val,
            count: 1.0,
            quantiles: vec![(0.95, val)],
        });
    }

    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));

    let length = samples.len() as f64;
    let min = samples.first().unwrap();
    let max = samples.last().unwrap();

    let p50 = samples[(0.50 * length - 1.0).round() as usize];
    let p95 = samples[(0.95 * length - 1.0).round() as usize];

    let sum = samples.iter().sum();
    let avg = sum / length;

    Some(DatadogStats {
        min: *min,
        max: *max,
        median: p50,
        avg,
        sum,
        count: length,
        quantiles: vec![(0.95, p95)],
    })
}

fn encode_events(events: Vec<Metric>, interval: i64, namespace: &str) -> DatadogRequest {
    let series = events
        .into_iter()
        .filter_map(|event| {
            let fullname = encode_namespace(namespace, &event.name);
            let ts = encode_timestamp(event.timestamp);
            let tags = event.tags.clone().map(encode_tags);
            match event.kind {
                MetricKind::Incremental => match event.value {
                    MetricValue::Counter { value } => Some(vec![DatadogMetric {
                        metric: fullname,
                        r#type: DatadogMetricType::Count,
                        interval: Some(interval),
                        points: vec![DatadogPoint(ts, value)],
                        tags,
                    }]),
                    MetricValue::Distribution {
                        values,
                        sample_rates,
                    } => {
                        // https://docs.datadoghq.com/developers/metrics/metrics_type/?tab=histogram#metric-type-definition
                        if let Some(s) = stats(&values, &sample_rates) {
                            let mut result = vec![
                                DatadogMetric {
                                    metric: format!("{}.min", &fullname),
                                    r#type: DatadogMetricType::Gauge,
                                    interval: Some(interval),
                                    points: vec![DatadogPoint(ts, s.min)],
                                    tags: tags.clone(),
                                },
                                DatadogMetric {
                                    metric: format!("{}.avg", &fullname),
                                    r#type: DatadogMetricType::Gauge,
                                    interval: Some(interval),
                                    points: vec![DatadogPoint(ts, s.avg)],
                                    tags: tags.clone(),
                                },
                                DatadogMetric {
                                    metric: format!("{}.count", &fullname),
                                    r#type: DatadogMetricType::Rate,
                                    interval: Some(interval),
                                    points: vec![DatadogPoint(ts, s.count)],
                                    tags: tags.clone(),
                                },
                                DatadogMetric {
                                    metric: format!("{}.median", &fullname),
                                    r#type: DatadogMetricType::Gauge,
                                    interval: Some(interval),
                                    points: vec![DatadogPoint(ts, s.median)],
                                    tags: tags.clone(),
                                },
                                DatadogMetric {
                                    metric: format!("{}.max", &fullname),
                                    r#type: DatadogMetricType::Gauge,
                                    interval: Some(interval),
                                    points: vec![DatadogPoint(ts, s.max)],
                                    tags: tags.clone(),
                                },
                            ];
                            for (q, v) in s.quantiles {
                                result.push(DatadogMetric {
                                    metric: format!(
                                        "{}.{}percentile",
                                        &fullname,
                                        (q * 100.0) as u32
                                    ),
                                    r#type: DatadogMetricType::Gauge,
                                    interval: Some(interval),
                                    points: vec![DatadogPoint(ts, v)],
                                    tags: tags.clone(),
                                })
                            }
                            Some(result)
                        } else {
                            None
                        }
                    }
                    MetricValue::Set { values } => Some(vec![DatadogMetric {
                        metric: fullname,
                        r#type: DatadogMetricType::Gauge,
                        interval: None,
                        points: vec![DatadogPoint(ts, values.len() as f64)],
                        tags,
                    }]),
                    _ => None,
                },
                MetricKind::Absolute => match event.value {
                    MetricValue::Gauge { value } => Some(vec![DatadogMetric {
                        metric: fullname,
                        r#type: DatadogMetricType::Gauge,
                        interval: None,
                        points: vec![DatadogPoint(ts, value)],
                        tags,
                    }]),
                    _ => None,
                },
            }
        })
        .flatten()
        .collect::<Vec<_>>();

    DatadogRequest { series }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::metric::{Metric, MetricKind, MetricValue};
    use crate::sinks::util::{http::HttpSink, test::load_sink};
    use crate::test_util::runtime;
    use chrono::offset::TimeZone;
    use chrono::Utc;
    use http::{Method, Uri};
    use pretty_assertions::assert_eq;
    use std::sync::atomic::AtomicI64;

    fn ts() -> DateTime<Utc> {
        Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 11)
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
    fn test_request() {
        let (sink, _, _) = load_sink::<DatadogConfig>(
            r#"
            namespace = "test"
            api_key = "test"
        "#,
        )
        .unwrap();

        let timestamp = Utc::now().timestamp();
        let sink = DatadogSink {
            config: sink,
            uri: build_uri(&default_host()).unwrap(),
            last_sent_timestamp: AtomicI64::new(timestamp),
        };

        let events = vec![
            Metric {
                name: "total".into(),
                timestamp: None,
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
            Metric {
                name: "unsupported".into(),
                timestamp: Some(ts()),
                tags: Some(tags()),
                kind: MetricKind::Absolute,
                value: MetricValue::Counter { value: 1.0 },
            },
        ];

        let mut rt = runtime();
        let req = rt
            .block_on_std(async move { sink.build_request(events).await })
            .unwrap();

        assert_eq!(req.method(), Method::POST);
        assert_eq!(
            req.uri(),
            &Uri::from_static("https://api.datadoghq.com/api/v1/series")
        );
    }

    #[test]
    fn test_encode_tags() {
        assert_eq!(
            encode_tags(tags()),
            vec!["empty_tag:", "normal_tag:value", "true_tag:true"]
        );
    }

    #[test]
    fn test_encode_timestamp() {
        assert_eq!(encode_timestamp(None), Utc::now().timestamp());
        assert_eq!(encode_timestamp(Some(ts())), 1542182950);
    }

    #[test]
    fn encode_counter() {
        let now = Utc::now().timestamp();
        let interval = 60;
        let events = vec![
            Metric {
                name: "total".into(),
                timestamp: None,
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
            Metric {
                name: "unsupported".into(),
                timestamp: Some(ts()),
                tags: Some(tags()),
                kind: MetricKind::Absolute,
                value: MetricValue::Counter { value: 1.0 },
            },
        ];
        let input = encode_events(events, interval, "ns");
        let json = serde_json::to_string(&input).unwrap();

        assert_eq!(
            json,
            format!("{{\"series\":[{{\"metric\":\"ns.total\",\"type\":\"count\",\"interval\":60,\"points\":[[{},1.5]],\"tags\":null}},{{\"metric\":\"ns.check\",\"type\":\"count\",\"interval\":60,\"points\":[[1542182950,1.0]],\"tags\":[\"empty_tag:\",\"normal_tag:value\",\"true_tag:true\"]}}]}}", now)
        );
    }

    #[test]
    fn encode_gauge() {
        let events = vec![
            Metric {
                name: "unsupported".into(),
                timestamp: Some(ts()),
                tags: None,
                kind: MetricKind::Incremental,
                value: MetricValue::Gauge { value: 0.1 },
            },
            Metric {
                name: "volume".into(),
                timestamp: Some(ts()),
                tags: None,
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge { value: -1.1 },
            },
        ];
        let input = encode_events(events, 60, "");
        let json = serde_json::to_string(&input).unwrap();

        assert_eq!(
            json,
            r#"{"series":[{"metric":"volume","type":"gauge","interval":null,"points":[[1542182950,-1.1]],"tags":null}]}"#
        );
    }

    #[test]
    fn encode_set() {
        let events = vec![Metric {
            name: "users".into(),
            timestamp: Some(ts()),
            tags: None,
            kind: MetricKind::Incremental,
            value: MetricValue::Set {
                values: vec!["alice".into(), "bob".into()].into_iter().collect(),
            },
        }];
        let input = encode_events(events, 60, "");
        let json = serde_json::to_string(&input).unwrap();

        assert_eq!(
            json,
            r#"{"series":[{"metric":"users","type":"gauge","interval":null,"points":[[1542182950,2.0]],"tags":null}]}"#
        );
    }

    #[test]
    fn test_dense_stats() {
        // https://github.com/DataDog/dd-agent/blob/master/tests/core/test_histogram.py
        let values = (0..20).into_iter().map(f64::from).collect::<Vec<_>>();
        let counts = vec![1; 20];

        assert_eq!(
            stats(&values, &counts),
            Some(DatadogStats {
                min: 0.0,
                max: 19.0,
                median: 9.0,
                avg: 9.5,
                sum: 190.0,
                count: 20.0,
                quantiles: vec![(0.95, 18.0)],
            })
        );
    }

    #[test]
    fn test_sparse_stats() {
        let values = (1..5).into_iter().map(f64::from).collect::<Vec<_>>();
        let counts = (1..5).into_iter().collect::<Vec<_>>();

        assert_eq!(
            stats(&values, &counts),
            Some(DatadogStats {
                min: 1.0,
                max: 4.0,
                median: 3.0,
                avg: 3.0,
                sum: 30.0,
                count: 10.0,
                quantiles: vec![(0.95, 4.0)],
            })
        );
    }

    #[test]
    fn test_single_value_stats() {
        let values = vec![10.0];
        let counts = vec![1];

        assert_eq!(
            stats(&values, &counts),
            Some(DatadogStats {
                min: 10.0,
                max: 10.0,
                median: 10.0,
                avg: 10.0,
                sum: 10.0,
                count: 1.0,
                quantiles: vec![(0.95, 10.0)],
            })
        );
    }
    #[test]
    fn test_nan_stats() {
        let values = vec![1.0, std::f64::NAN];
        let counts = vec![1, 1];
        assert!(stats(&values, &counts).is_some());
    }

    #[test]
    fn test_unequal_stats() {
        let values = vec![1.0];
        let counts = vec![1, 2, 3];
        assert!(stats(&values, &counts).is_none());
    }

    #[test]
    fn test_empty_stats() {
        let values = vec![];
        let counts = vec![];
        assert!(stats(&values, &counts).is_none());
    }

    #[test]
    fn test_zero_counts_stats() {
        let values = vec![1.0, 2.0];
        let counts = vec![0, 0];
        assert!(stats(&values, &counts).is_none());
    }

    #[test]
    fn encode_distribution() {
        // https://docs.datadoghq.com/developers/metrics/metrics_type/?tab=histogram#metric-type-definition
        let events = vec![Metric {
            name: "requests".into(),
            timestamp: Some(ts()),
            tags: None,
            kind: MetricKind::Incremental,
            value: MetricValue::Distribution {
                values: vec![1.0, 2.0, 3.0],
                sample_rates: vec![3, 3, 2],
            },
        }];
        let input = encode_events(events, 60, "");
        let json = serde_json::to_string(&input).unwrap();

        assert_eq!(
            json,
            r#"{"series":[{"metric":"requests.min","type":"gauge","interval":60,"points":[[1542182950,1.0]],"tags":null},{"metric":"requests.avg","type":"gauge","interval":60,"points":[[1542182950,1.875]],"tags":null},{"metric":"requests.count","type":"rate","interval":60,"points":[[1542182950,8.0]],"tags":null},{"metric":"requests.median","type":"gauge","interval":60,"points":[[1542182950,2.0]],"tags":null},{"metric":"requests.max","type":"gauge","interval":60,"points":[[1542182950,3.0]],"tags":null},{"metric":"requests.95percentile","type":"gauge","interval":60,"points":[[1542182950,3.0]],"tags":null}]}"#
        );
    }
}

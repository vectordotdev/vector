use crate::{
    dns::Resolver,
    event::metric::{Metric, MetricValue},
    sinks::util::{
        http::{
            Error as HttpError, HttpBatchService, HttpClient, HttpRetryLogic,
            Response as HttpResponse,
        },
        BatchEventsConfig, MetricBuffer, TowerRequestConfig,
    },
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use chrono::{DateTime, Utc};
use futures01::{Future, Poll, Sink};
use http::{Method, StatusCode, Uri};
use hyper;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use tower::Service;

pub enum Field {
    /// string
    String(String),
    /// float
    Float(f64),
    /// unsigned integer
    UnsignedInt(u32),
}

#[derive(Debug, Snafu)]
enum ConfigError {
    #[snafu(display("InfluxDB v1 or v2 should be configured as endpoint."))]
    MissingConfiguration {},
    #[snafu(display(
        "Unclear settings. Both version configured v1: {:?}, v2: {:?}.",
        v1_settings,
        v2_settings
    ))]
    BothConfiguration {
        v1_settings: InfluxDB1Settings,
        v2_settings: InfluxDB2Settings,
    },
}

#[derive(Clone)]
struct InfluxDBSvc {
    config: InfluxDBConfig,
    inner: HttpBatchService,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct InfluxDBConfig {
    pub namespace: String,
    pub endpoint: String,
    #[serde(flatten)]
    pub influxdb1_settings: Option<InfluxDB1Settings>,
    #[serde(flatten)]
    pub influxdb2_settings: Option<InfluxDB2Settings>,
    #[serde(default)]
    pub batch: BatchEventsConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct InfluxDB1Settings {
    database: String,
    consistency: Option<String>,
    retention_policy_name: Option<String>,
    username: Option<String>,
    password: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct InfluxDB2Settings {
    org: String,
    bucket: String,
    token: String,
}

trait InfluxDBSettings {
    fn write_uri(self: &Self, endpoint: String) -> crate::Result<Uri>;
    fn healthcheck_uri(self: &Self, endpoint: String) -> crate::Result<Uri>;
    fn token(self: &Self) -> String;
}

impl InfluxDBSettings for InfluxDB1Settings {
    fn write_uri(self: &Self, endpoint: String) -> crate::Result<Uri> {
        encode_uri(
            &endpoint,
            "write",
            &mut [
                ("consistency", self.consistency.clone()),
                ("db", Some(self.database.clone())),
                ("rp", self.retention_policy_name.clone()),
                ("p", self.password.clone()),
                ("u", self.username.clone()),
                ("precision", Some("ns".to_owned())),
            ],
        )
    }

    fn healthcheck_uri(self: &Self, endpoint: String) -> crate::Result<Uri> {
        encode_uri(&endpoint, "ping", &mut [])
    }

    fn token(self: &Self) -> String {
        "".to_string()
    }
}

impl InfluxDBSettings for InfluxDB2Settings {
    fn write_uri(self: &Self, endpoint: String) -> crate::Result<Uri> {
        encode_uri(
            &endpoint,
            "api/v2/write",
            &mut [
                ("org", Some(self.org.clone())),
                ("bucket", Some(self.bucket.clone())),
                ("precision", Some("ns".to_owned())),
            ],
        )
    }

    fn healthcheck_uri(self: &Self, endpoint: String) -> crate::Result<Uri> {
        encode_uri(&endpoint, "health", &mut [])
    }

    fn token(self: &Self) -> String {
        self.token.clone()
    }
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

#[typetag::serde(name = "influxdb_metrics")]
impl SinkConfig for InfluxDBConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let healthcheck = InfluxDBSvc::healthcheck(self.clone(), cx.resolver())?;
        let sink = InfluxDBSvc::new(self.clone(), cx)?;
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
    pub fn new(config: InfluxDBConfig, cx: SinkContext) -> crate::Result<super::RouterSink> {
        let settings = InfluxDBSvc::influxdb_settings(config.clone())?;

        let endpoint = config.endpoint.clone();
        let token = settings.token();

        let batch = config.batch.unwrap_or(20, 1);
        let request = config.request.unwrap_with(&REQUEST_DEFAULTS);

        let uri = settings.write_uri(endpoint)?;

        let build_request = move |body: Vec<u8>| {
            let mut builder = hyper::Request::builder();
            builder.method(Method::POST);
            builder.uri(uri.clone());

            builder.header("Content-Type", "text/plain");
            builder.header("Authorization", format!("Token {}", token));
            builder.body(body).unwrap()
        };

        let http_service = HttpBatchService::new(cx.resolver(), None, build_request);

        let influxdb_http_service = InfluxDBSvc {
            config,
            inner: http_service,
        };

        let sink = request
            .batch_sink(
                HttpRetryLogic,
                influxdb_http_service,
                MetricBuffer::new(),
                batch,
                cx.acker(),
            )
            .sink_map_err(|e| error!("Fatal influxdb sink error: {}", e));

        Ok(Box::new(sink))
    }

    // V1: https://docs.influxdata.com/influxdb/v1.7/tools/api/#ping-http-endpoint
    // V2: https://v2.docs.influxdata.com/v2.0/api/#operation/GetHealth
    fn healthcheck(
        config: InfluxDBConfig,
        resolver: Resolver,
    ) -> crate::Result<super::Healthcheck> {
        let settings = InfluxDBSvc::influxdb_settings(config.clone())?;

        let endpoint = config.endpoint.clone();

        let uri = settings.healthcheck_uri(endpoint)?;

        let request = hyper::Request::get(uri).body(hyper::Body::empty()).unwrap();

        let mut client = HttpClient::new(resolver, None)?;

        let healthcheck = client
            .call(request)
            .map_err(|err| err.into())
            .and_then(|response| match response.status() {
                StatusCode::OK => Ok(()),
                StatusCode::NO_CONTENT => Ok(()),
                other => Err(super::HealthcheckError::UnexpectedStatus { status: other }.into()),
            });

        Ok(Box::new(healthcheck))
    }

    fn influxdb_settings(
        config: InfluxDBConfig,
    ) -> Result<Box<dyn InfluxDBSettings>, crate::Error> {
        if config.influxdb1_settings.is_some() & config.influxdb2_settings.is_some() {
            return Err(ConfigError::BothConfiguration {
                v1_settings: config.influxdb1_settings.unwrap(),
                v2_settings: config.influxdb2_settings.unwrap(),
            }
            .into());
        }

        if config.influxdb1_settings.is_none() & config.influxdb2_settings.is_none() {
            return Err(ConfigError::MissingConfiguration {}.into());
        }

        if let Some(settings) = config.influxdb1_settings {
            Ok(Box::new(settings))
        } else {
            Ok(Box::new(config.influxdb2_settings.unwrap()))
        }
    }
}

fn encode_uri(
    endpoint: &str,
    path: &str,
    pairs: &mut [(&str, Option<String>)],
) -> crate::Result<Uri> {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());

    for pair in pairs {
        if let Some(v) = &pair.1 {
            serializer.append_pair(pair.0, v);
        }
    }

    let mut url = if endpoint.ends_with('/') {
        format!("{}{}?{}", endpoint, path, serializer.finish())
    } else {
        format!("{}/{}?{}", endpoint, path, serializer.finish())
    };

    if url.ends_with("?") {
        url.pop();
    }

    Ok(url.parse::<Uri>().context(super::UriParseError)?)
}

impl Service<Vec<Metric>> for InfluxDBSvc {
    type Response = HttpResponse;
    type Error = HttpError;
    type Future = Box<dyn Future<Item = Self::Response, Error = Self::Error> + Send + 'static>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready()
    }

    fn call(&mut self, items: Vec<Metric>) -> Self::Future {
        let input = encode_events(items, &self.config.namespace);
        let body: Vec<u8> = input.into_bytes();

        self.inner.call(body)
    }
}

fn encode_events(events: Vec<Metric>, namespace: &str) -> String {
    let mut output = String::new();
    for event in events.into_iter() {
        let fullname = encode_namespace(namespace, &event.name);
        let ts = encode_timestamp(event.timestamp);
        let tags = event.tags.clone();
        match event.value {
            MetricValue::Counter { value } => {
                let fields = to_fields(value);

                influx_line_protocol(fullname, "counter", tags, Some(fields), ts, &mut output)
            }
            MetricValue::Gauge { value } => {
                let fields = to_fields(value);

                influx_line_protocol(fullname, "gauge", tags, Some(fields), ts, &mut output);
            }
            MetricValue::Set { values } => {
                let fields = to_fields(values.len() as f64);

                influx_line_protocol(fullname, "set", tags, Some(fields), ts, &mut output);
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

                influx_line_protocol(fullname, "histogram", tags, Some(fields), ts, &mut output);
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

                influx_line_protocol(fullname, "summary", tags, Some(fields), ts, &mut output);
            }
            MetricValue::Distribution {
                values,
                sample_rates,
            } => {
                let fields = encode_distribution(&values, &sample_rates);

                influx_line_protocol(fullname, "distribution", tags, fields, ts, &mut output);
            }
        }
    }

    // remove last '\n'
    output.pop();

    return output;
}

fn encode_distribution(values: &[f64], counts: &[u32]) -> Option<HashMap<String, Field>> {
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
        return Some(
            vec![
                ("min".to_owned(), Field::Float(val)),
                ("max".to_owned(), Field::Float(val)),
                ("median".to_owned(), Field::Float(val)),
                ("avg".to_owned(), Field::Float(val)),
                ("sum".to_owned(), Field::Float(val)),
                ("count".to_owned(), Field::Float(1.0)),
                ("quantile_0.95".to_owned(), Field::Float(val)),
            ]
            .into_iter()
            .collect(),
        );
    }

    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));

    let length = samples.len() as f64;
    let min = samples.first().unwrap();
    let max = samples.last().unwrap();

    let p50 = samples[(0.50 * length - 1.0).round() as usize];
    let p95 = samples[(0.95 * length - 1.0).round() as usize];

    let sum = samples.iter().sum();
    let avg = sum / length;

    let fields: HashMap<String, Field> = vec![
        ("min".to_owned(), Field::Float(*min)),
        ("max".to_owned(), Field::Float(*max)),
        ("median".to_owned(), Field::Float(p50)),
        ("avg".to_owned(), Field::Float(avg)),
        ("sum".to_owned(), Field::Float(sum)),
        ("count".to_owned(), Field::Float(length)),
        ("quantile_0.95".to_owned(), Field::Float(p95)),
    ]
    .into_iter()
    .collect();

    Some(fields)
}

// https://v2.docs.influxdata.com/v2.0/reference/syntax/line-protocol/
fn influx_line_protocol(
    measurement: String,
    metric_type: &str,
    tags: Option<BTreeMap<String, String>>,
    fields: Option<HashMap<String, Field>>,
    timestamp: i64,
    line_protocol: &mut String,
) {
    // Fields
    let unwrapped_fields = fields.unwrap_or_else(|| HashMap::new());
    // LineProtocol should have a field
    if unwrapped_fields.is_empty() {
        return;
    }

    encode_string(measurement, line_protocol);
    line_protocol.push(',');

    // Tags
    let mut unwrapped_tags = tags.unwrap_or_else(|| BTreeMap::new());
    unwrapped_tags.insert("metric_type".to_owned(), metric_type.to_owned());
    encode_tags(unwrapped_tags, line_protocol);
    line_protocol.push(' ');

    // Fields
    encode_fields(unwrapped_fields, line_protocol);
    line_protocol.push(' ');

    // Timestamp
    line_protocol.push_str(&timestamp.to_string());
    line_protocol.push('\n');
}

fn encode_string(key: String, output: &mut String) {
    for c in key.chars() {
        if "\\, =".contains(c) {
            output.push('\\');
        }
        output.push(c);
    }
}

fn encode_tags(tags: BTreeMap<String, String>, output: &mut String) {
    let sorted = tags
        // sort by key
        .iter()
        .collect::<BTreeMap<_, _>>();

    for (key, value) in sorted {
        if key.is_empty() || value.is_empty() {
            continue;
        }
        encode_string(key.to_string(), output);
        output.push('=');
        encode_string(value.to_string(), output);
        output.push(',');
    }

    // remove last ','
    output.pop();
}

fn encode_fields(fields: HashMap<String, Field>, output: &mut String) {
    for (key, value) in fields.into_iter() {
        encode_string(key.to_string(), output);
        output.push('=');
        match value {
            Field::String(s) => {
                output.push('"');
                for c in s.chars() {
                    if "\\\"".contains(c) {
                        output.push('\\');
                    }
                    output.push(c);
                }
                output.push('"');
            }
            Field::Float(f) => output.push_str(&f.to_string()),
            Field::UnsignedInt(i) => {
                output.push_str(&i.to_string());
                output.push('u');
            }
        };
        output.push(',');
    }

    // remove last ','
    output.pop();
}

fn encode_timestamp(timestamp: Option<DateTime<Utc>>) -> i64 {
    if let Some(ts) = timestamp {
        ts.timestamp_nanos()
    } else {
        encode_timestamp(Some(Utc::now()))
    }
}

fn encode_namespace(namespace: &str, name: &str) -> String {
    if !namespace.is_empty() {
        format!("{}.{}", namespace, name)
    } else {
        name.to_string()
    }
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
    use crate::event::metric::{Metric, MetricKind, MetricValue};
    use chrono::offset::TimeZone;
    use pretty_assertions::assert_eq;

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
    fn test_encode_uri_valid() {
        let uri = encode_uri(
            "http://localhost:9999",
            "api/v2/write",
            &mut [
                ("org", Some("my-org".to_owned())),
                ("bucket", Some("my-bucket".to_owned())),
                ("precision", Some("ns".to_owned())),
            ],
        )
        .unwrap();
        assert_eq!(
            uri,
            "http://localhost:9999/api/v2/write?org=my-org&bucket=my-bucket&precision=ns"
        );

        let uri = encode_uri(
            "http://localhost:9999/",
            "api/v2/write",
            &mut [
                ("org", Some("my-org".to_owned())),
                ("bucket", Some("my-bucket".to_owned())),
            ],
        )
        .unwrap();
        assert_eq!(
            uri,
            "http://localhost:9999/api/v2/write?org=my-org&bucket=my-bucket"
        );

        let uri = encode_uri(
            "http://localhost:9999",
            "api/v2/write",
            &mut [
                ("org", Some("Orgazniation name".to_owned())),
                ("bucket", Some("Bucket=name".to_owned())),
                ("none", None),
            ],
        )
        .unwrap();
        assert_eq!(
            uri,
            "http://localhost:9999/api/v2/write?org=Orgazniation+name&bucket=Bucket%3Dname"
        );
    }

    #[test]
    fn test_encode_uri_invalid() {
        encode_uri(
            "localhost:9999",
            "api/v2/write",
            &mut [
                ("org", Some("my-org".to_owned())),
                ("bucket", Some("my-bucket".to_owned())),
            ],
        )
        .unwrap_err();
    }

    #[test]
    fn test_influxdb1_test_write_uri() {
        let settings = InfluxDB1Settings {
            consistency: Some("quorum".to_owned()),
            database: "vector_db".to_owned(),
            retention_policy_name: Some("autogen".to_owned()),
            username: Some("writer".to_owned()),
            password: Some("secret".to_owned()),
        };

        let uri = settings
            .write_uri("http://localhost:8086".to_owned())
            .unwrap();
        assert_eq!("http://localhost:8086/write?consistency=quorum&db=vector_db&rp=autogen&p=secret&u=writer&precision=ns", uri.to_string())
    }

    #[test]
    fn test_influxdb2_test_write_uri() {
        let settings = InfluxDB2Settings {
            org: "my-org".to_owned(),
            bucket: "my-bucket".to_owned(),
            token: "my-token".to_owned(),
        };

        let uri = settings
            .write_uri("http://localhost:9999".to_owned())
            .unwrap();
        assert_eq!(
            "http://localhost:9999/api/v2/write?org=my-org&bucket=my-bucket&precision=ns",
            uri.to_string()
        )
    }

    #[test]
    fn test_influxdb1_test_healthcheck_uri() {
        let settings = InfluxDB1Settings {
            consistency: Some("quorum".to_owned()),
            database: "vector_db".to_owned(),
            retention_policy_name: Some("autogen".to_owned()),
            username: Some("writer".to_owned()),
            password: Some("secret".to_owned()),
        };

        let uri = settings
            .healthcheck_uri("http://localhost:8086".to_owned())
            .unwrap();
        assert_eq!("http://localhost:8086/ping", uri.to_string())
    }

    #[test]
    fn test_influxdb2_test_healthcheck_uri() {
        let settings = InfluxDB2Settings {
            org: "my-org".to_owned(),
            bucket: "my-bucket".to_owned(),
            token: "my-token".to_owned(),
        };

        let uri = settings
            .healthcheck_uri("http://localhost:9999".to_owned())
            .unwrap();
        assert_eq!("http://localhost:9999/health", uri.to_string())
    }

    #[test]
    fn test_influxdb_settings_both() {
        let config = r#"
        namespace = "service"
        endpoint = "https://us-west-2-1.aws.cloud2.influxdata.com"
        bucket = "my-bucket"
        org = "my-org"
        token = "my-token"
        database = "my-database"
    "#;
        let config: InfluxDBConfig = toml::from_str(&config).unwrap();
        let settings = InfluxDBSvc::influxdb_settings(config);
        match settings {
            Ok(_) => assert!(false, "Expected error"),
            Err(e) => assert_eq!(format!("{}",e), "Unclear settings. Both version configured v1: InfluxDB1Settings { database: \"my-database\", consistency: None, retention_policy_name: None, username: None, password: None }, v2: InfluxDB2Settings { org: \"my-org\", bucket: \"my-bucket\", token: \"my-token\" }.".to_owned())
        }
    }

    #[test]
    fn test_influxdb_settings_missing() {
        let config = r#"
        namespace = "service"
        endpoint = "https://us-west-2-1.aws.cloud2.influxdata.com"
    "#;
        let config: InfluxDBConfig = toml::from_str(&config).unwrap();
        let settings = InfluxDBSvc::influxdb_settings(config);
        match settings {
            Ok(_) => assert!(false, "Expected error"),
            Err(e) => assert_eq!(
                format!("{}", e),
                "InfluxDB v1 or v2 should be configured as endpoint.".to_owned()
            ),
        }
    }

    #[test]
    fn test_influxdb1_settings() {
        let config = r#"
        namespace = "service"
        endpoint = "https://us-west-2-1.aws.cloud2.influxdata.com"
        database = "my-database"
    "#;
        let config: InfluxDBConfig = toml::from_str(&config).unwrap();
        let _ = InfluxDBSvc::influxdb_settings(config).unwrap();
    }

    #[test]
    fn test_influxdb2_settings() {
        let config = r#"
        namespace = "service"
        endpoint = "https://us-west-2-1.aws.cloud2.influxdata.com"
        bucket = "my-bucket"
        org = "my-org"
        token = "my-token"
    "#;
        let config: InfluxDBConfig = toml::from_str(&config).unwrap();
        let _ = InfluxDBSvc::influxdb_settings(config).unwrap();
    }

    #[test]
    fn test_encode_timestamp() {
        let start = Utc::now().timestamp_nanos();
        assert_eq!(encode_timestamp(Some(ts())), 1542182950000000011);
        assert!(encode_timestamp(None) >= start)
    }

    #[test]
    fn test_encode_namespace() {
        assert_eq!(encode_namespace("services", "status"), "services.status");
        assert_eq!(encode_namespace("", "status"), "status")
    }

    #[test]
    fn test_encode_key() {
        let mut value = String::new();
        encode_string("measurement_name".to_string(), &mut value);
        assert_eq!(value, "measurement_name");

        let mut value = String::new();
        encode_string("measurement name".to_string(), &mut value);
        assert_eq!(value, "measurement\\ name");

        let mut value = String::new();
        encode_string("measurement=name".to_string(), &mut value);
        assert_eq!(value, "measurement\\=name");

        let mut value = String::new();
        encode_string("measurement,name".to_string(), &mut value);
        assert_eq!(value, "measurement\\,name");
    }

    #[test]
    fn test_encode_tags() {
        let mut value = String::new();
        encode_tags(tags(), &mut value);

        assert_eq!(value, "normal_tag=value,true_tag=true");

        let tags_to_escape = vec![
            ("tag".to_owned(), "val=ue".to_owned()),
            ("name escape".to_owned(), "true".to_owned()),
            ("value_escape".to_owned(), "value escape".to_owned()),
            ("a_first_place".to_owned(), "10".to_owned()),
        ]
        .into_iter()
        .collect();

        let mut value = String::new();
        encode_tags(tags_to_escape, &mut value);
        assert_eq!(
            value,
            "a_first_place=10,name\\ escape=true,tag=val\\=ue,value_escape=value\\ escape"
        );
    }

    #[test]
    fn test_encode_fields() {
        let fields = vec![
            (
                "field_string".to_owned(),
                Field::String("string value".to_owned()),
            ),
            (
                "field_string_escape".to_owned(),
                Field::String("string\\val\"ue".to_owned()),
            ),
            ("field_float".to_owned(), Field::Float(123.45)),
            ("field_unsigned_int".to_owned(), Field::UnsignedInt(657)),
            ("escape key".to_owned(), Field::Float(10.0)),
        ]
        .into_iter()
        .collect();

        let mut value = String::new();
        encode_fields(fields, &mut value);
        assert_fields(
            value,
            [
                "escape\\ key=10",
                "field_float=123.45",
                "field_string=\"string value\"",
                "field_string_escape=\"string\\\\val\\\"ue\"",
                "field_unsigned_int=657u",
            ]
            .to_vec(),
        )
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

        let line_protocols = encode_events(events, "ns");
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

        let line_protocols = encode_events(events, "ns");
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

        let line_protocols = encode_events(events, "ns");
        assert_eq!(
            line_protocols,
            "ns.users,metric_type=set,normal_tag=value,true_tag=true value=2 1542182950000000011"
        );
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

        let line_protocols = encode_events(events, "ns");
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

        let line_protocols = encode_events(events, "ns");
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
                },
            },
            Metric {
                name: "dense_stats".into(),
                timestamp: Some(ts()),
                tags: None,
                kind: MetricKind::Incremental,
                value: MetricValue::Distribution {
                    values: (0..20).into_iter().map(f64::from).collect::<Vec<_>>(),
                    sample_rates: vec![1; 20],
                },
            },
            Metric {
                name: "sparse_stats".into(),
                timestamp: Some(ts()),
                tags: None,
                kind: MetricKind::Incremental,
                value: MetricValue::Distribution {
                    values: (1..5).into_iter().map(f64::from).collect::<Vec<_>>(),
                    sample_rates: (1..5).into_iter().collect::<Vec<_>>(),
                },
            },
        ];

        let line_protocols = encode_events(events, "ns");
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
            },
        }];

        let line_protocols = encode_events(events, "ns");
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
            },
        }];

        let line_protocols = encode_events(events, "ns");
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
            },
        }];

        let line_protocols = encode_events(events, "ns");
        assert_eq!(line_protocols.len(), 0);
    }

    fn assert_fields(value: String, fields: Vec<&str>) {
        let encoded_fields: Vec<&str> = value.split(',').collect();

        assert_eq!(fields.len(), encoded_fields.len());

        for field in fields.into_iter() {
            assert!(
                encoded_fields.contains(&field),
                format!("Fields: {} has to have: {}", value, field)
            )
        }
    }

    // ns.requests,metric_type=distribution,normal_tag=value,true_tag=true avg=1.875,count=8,max=3,median=2,min=1,quantile_0.95=3,sum=15 1542182950000000011
    //
    // =>
    //
    // ns.requests
    // metric_type=distribution,normal_tag=value,true_tag=true
    // avg=1.875,count=8,max=3,median=2,min=1,quantile_0.95=3,sum=15
    // 1542182950000000011
    //
    fn split_line_protocol(line_protocol: &str) -> (&str, &str, String, &str) {
        let mut split = line_protocol.splitn(2, ',').collect::<Vec<&str>>();
        let measurement = split[0];

        split = split[1].splitn(3, ' ').collect::<Vec<&str>>();

        return (measurement, split[0], split[1].to_string(), split[2]);
    }
}

#[cfg(feature = "influxdb-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use crate::event::metric::{MetricKind, MetricValue};
    use crate::event::Metric;
    use crate::runtime::Runtime;
    use crate::sinks::influxdb_metrics::{
        InfluxDB1Settings, InfluxDB2Settings, InfluxDBConfig, InfluxDBSvc,
    };
    use crate::topology::SinkContext;
    use crate::Event;
    use chrono::Utc;
    use futures01::{stream, Sink};

    const ORG: &str = "my-org";
    const BUCKET: &str = "my-bucket";
    const TOKEN: &str = "my-token";
    const DATABASE: &str = "my-database";

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

    fn onboarding_v2() {
        let mut body = std::collections::HashMap::new();
        body.insert("username", "my-user");
        body.insert("password", "my-password");
        body.insert("org", ORG);
        body.insert("bucket", BUCKET);
        body.insert("token", TOKEN);

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();

        let res = client
            .post("http://localhost:9999/api/v2/setup")
            .json(&body)
            .header("accept", "application/json")
            .send()
            .unwrap();

        let status = res.status();

        assert!(
            status == http::StatusCode::CREATED || status == http::StatusCode::UNPROCESSABLE_ENTITY,
            format!("UnexpectedStatus: {}", status)
        );
    }

    #[test]
    fn influxdb2_metrics_healthchecks_ok() {
        onboarding_v2();

        let mut rt = Runtime::new().unwrap();
        let cx = SinkContext::new_test(rt.executor());
        let config = InfluxDBConfig {
            namespace: "ns".to_string(),
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

        let healthcheck = InfluxDBSvc::healthcheck(config, cx.resolver()).unwrap();
        rt.block_on(healthcheck).unwrap();
    }

    #[test]
    fn influxdb2_metrics_healthchecks_fail() {
        onboarding_v2();

        let mut rt = Runtime::new().unwrap();
        let cx = SinkContext::new_test(rt.executor());
        let config = InfluxDBConfig {
            namespace: "ns".to_string(),
            endpoint: "http://not_exist:9999".to_string(),
            influxdb1_settings: None,
            influxdb2_settings: Some(InfluxDB2Settings {
                org: ORG.to_string(),
                bucket: BUCKET.to_string(),
                token: TOKEN.to_string(),
            }),
            batch: Default::default(),
            request: Default::default(),
        };

        let healthcheck = InfluxDBSvc::healthcheck(config, cx.resolver()).unwrap();
        rt.block_on(healthcheck).unwrap_err();
    }

    #[test]
    fn influxdb2_metrics_put_data() {
        onboarding_v2();

        let mut rt = Runtime::new().unwrap();
        let cx = SinkContext::new_test(rt.executor());

        let config = InfluxDBConfig {
            namespace: "ns".to_string(),
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

        let sink = InfluxDBSvc::new(config, cx).unwrap();

        let stream = stream::iter_ok(events.clone().into_iter());

        let pump = sink.send_all(stream);
        let _ = rt.block_on(pump).unwrap();

        let mut body = std::collections::HashMap::new();
        body.insert("query", format!("from(bucket:\"my-bucket\") |> range(start: 0) |> filter(fn: (r) => r._measurement == \"ns.{}\")", metric));
        body.insert("type", "flux".to_owned());

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();

        let mut res = client
            .post("http://localhost:9999/api/v2/query?org=my-org")
            .json(&body)
            .header("accept", "application/json")
            .header("Authorization", "Token my-token")
            .send()
            .unwrap();
        let result = res.text();
        let string = result.unwrap();

        let lines = string.split("\n").collect::<Vec<&str>>();
        let header = lines[0].split(",").collect::<Vec<&str>>();
        let record = lines[1].split(",").collect::<Vec<&str>>();

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

    #[test]
    fn influxdb1_metrics_healthchecks_ok() {
        let mut rt = Runtime::new().unwrap();
        let cx = SinkContext::new_test(rt.executor());

        let config = InfluxDBConfig {
            namespace: "ns".to_string(),
            endpoint: "http://localhost:8086".to_string(),
            influxdb1_settings: Some(InfluxDB1Settings {
                database: DATABASE.to_string(),
                consistency: None,
                retention_policy_name: None,
                username: None,
                password: None,
            }),
            influxdb2_settings: None,
            batch: Default::default(),
            request: Default::default(),
        };
        let healthcheck = InfluxDBSvc::healthcheck(config, cx.resolver()).unwrap();
        rt.block_on(healthcheck).unwrap();
    }

    #[test]
    fn influxdb1_metrics_healthchecks_fail() {
        let mut rt = Runtime::new().unwrap();
        let cx = SinkContext::new_test(rt.executor());
        let config = InfluxDBConfig {
            namespace: "ns".to_string(),
            endpoint: "http://not_exist:8086".to_string(),
            influxdb1_settings: Some(InfluxDB1Settings {
                database: DATABASE.to_string(),
                consistency: None,
                retention_policy_name: None,
                username: None,
                password: None,
            }),
            influxdb2_settings: None,
            batch: Default::default(),
            request: Default::default(),
        };

        let healthcheck = InfluxDBSvc::healthcheck(config, cx.resolver()).unwrap();
        rt.block_on(healthcheck).unwrap_err();
    }
}

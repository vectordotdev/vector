use crate::{
    config::{DataType, GenerateConfig, SinkConfig, SinkContext},
    event::{Event, Metric, MetricValue},
    internal_events::SplunkInvalidMetricReceived,
    internal_events::{SplunkEventEncodeError, SplunkEventSent},
    sinks::splunk_hec::conn,
    sinks::util::{encode_namespace, http::HttpSink, BatchConfig, Compression, TowerRequestConfig},
    sinks::{Healthcheck, VectorSink},
    template::Template,
    tls::TlsOptions,
};
use http::Request;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::iter;

use super::common::{build_request, host_key, render_template_string};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct HecSinkMetricsConfig {
    pub default_namespace: Option<String>,
    pub token: String,
    pub endpoint: String,
    #[serde(default = "host_key")]
    pub host_key: String,
    pub index: Option<Template>,
    pub sourcetype: Option<Template>,
    pub source: Option<Template>,
    #[serde(default)]
    pub compression: Compression,
    #[serde(default)]
    pub batch: BatchConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub tls: Option<TlsOptions>,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(untagged)]
enum FieldValue<'a> {
    Float(f64),
    Str(Cow<'a, str>),
}

impl<'a> From<&'a str> for FieldValue<'a> {
    fn from(s: &'a str) -> Self {
        FieldValue::Str(Cow::Borrowed(s))
    }
}

impl<'a> From<String> for FieldValue<'a> {
    fn from(s: String) -> Self {
        FieldValue::Str(Cow::Owned(s))
    }
}

impl<'a> From<f64> for FieldValue<'a> {
    fn from(f: f64) -> Self {
        FieldValue::Float(f)
    }
}

type FieldMap<'a> = BTreeMap<&'a str, FieldValue<'a>>;

#[derive(Serialize, Debug)]
struct HecEvent<'a> {
    time: f64,

    #[serde(skip_serializing_if = "Option::is_none")]
    host: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    index: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    sourcetype: Option<String>,

    fields: FieldMap<'a>,

    event: &'a str,
}

impl GenerateConfig for HecSinkMetricsConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            default_namespace: None,
            token: "${VECTOR_SPLUNK_HEC_TOKEN}".to_owned(),
            endpoint: "http://localhost:8088".to_owned(),
            host_key: host_key(),
            index: None,
            sourcetype: None,
            source: None,
            compression: Compression::default(),
            batch: BatchConfig::default(),
            request: TowerRequestConfig::default(),
            tls: None,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "splunk_hec_metrics")]
impl SinkConfig for HecSinkMetricsConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        conn::build_sink(
            self.clone(),
            &self.request,
            &self.tls,
            cx.proxy(),
            self.batch,
            self.compression,
            cx.acker(),
            &self.endpoint,
            &self.token,
        )
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }

    fn sink_type(&self) -> &'static str {
        "splunk_hec_metrics"
    }
}

#[async_trait::async_trait]
impl HttpSink for HecSinkMetricsConfig {
    type Input = Vec<u8>;
    type Output = Vec<u8>;

    fn encode_event(&self, event: Event) -> Option<Self::Input> {
        let event = event.into_metric();

        let hec_event = HecEvent {
            time: Self::extract_timestamp(&event),
            host: self.extract_host(&event),
            source: self.extract_source(&event),
            sourcetype: self.extract_sourcetype(&event),
            index: self.extract_index(&event),
            fields: self.extract_fields(&event)?,
            event: "metric",
        };

        let body = json!(hec_event);

        match serde_json::to_vec(&body) {
            Ok(value) => {
                emit!(&SplunkEventSent {
                    byte_size: value.len()
                });
                Some(value)
            }
            Err(error) => {
                emit!(&SplunkEventEncodeError { error });
                None
            }
        }
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<Request<Vec<u8>>> {
        build_request(&self.endpoint, &self.token, self.compression, events).await
    }
}

impl HecSinkMetricsConfig {
    fn extract_timestamp(metric: &Metric) -> f64 {
        metric
            .timestamp()
            .unwrap_or_else(chrono::Utc::now)
            .timestamp_millis() as f64
            / 1000f64
    }

    fn extract_host(&self, metric: &Metric) -> Option<String> {
        metric.tag_value(&self.host_key)
    }

    fn extract_source(&self, metric: &Metric) -> Option<String> {
        render_template_string(self.source.as_ref()?, metric, "source")
    }

    fn extract_sourcetype(&self, metric: &Metric) -> Option<String> {
        render_template_string(self.sourcetype.as_ref()?, metric, "sourcetype")
    }

    fn extract_index(&self, metric: &Metric) -> Option<String> {
        render_template_string(self.index.as_ref()?, metric, "index")
    }

    fn extract_fields<'a>(&'a self, metric: &'a Metric) -> Option<FieldMap> {
        let templated_field_keys = self.extract_templated_field_keys();

        let metric_name = self.extract_metric_name(metric);
        let metric_value = Self::extract_metric_value(metric)?;

        Some(
            metric
                .tags()
                .into_iter()
                .flatten()
                .filter(|(k, _)| !templated_field_keys.contains(k))
                .map(|(k, v)| (k.as_str(), FieldValue::from(v.as_str())))
                .chain(iter::once(("metric_name", FieldValue::from(metric_name))))
                .chain(iter::once(("_value", FieldValue::from(metric_value))))
                .collect::<FieldMap>(),
        )
    }

    fn extract_metric_name(&self, metric: &Metric) -> String {
        encode_namespace(
            metric
                .namespace()
                .or_else(|| self.default_namespace.as_deref()),
            '.',
            metric.name(),
        )
    }

    fn extract_templated_field_keys(&self) -> Vec<String> {
        [
            self.index.as_ref(),
            self.source.as_ref(),
            self.sourcetype.as_ref(),
        ]
        .iter()
        .flatten()
        .filter_map(|t| t.get_fields())
        .flatten()
        .map(|f| f.replace("tags.", ""))
        .collect::<Vec<_>>()
    }

    fn extract_metric_value(metric: &Metric) -> Option<f64> {
        match *metric.value() {
            MetricValue::Counter { value } => Some(value),
            MetricValue::Gauge { value } => Some(value),
            _ => {
                emit!(&SplunkInvalidMetricReceived {
                    value: metric.value(),
                    kind: &metric.kind(),
                });
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Metric, MetricKind, MetricValue};
    use crate::sinks::util::{http::HttpSink, test::load_sink};
    use chrono::{DateTime, Utc};
    use serde_json::Value as JsonValue;
    use shared::btreemap;
    use std::collections::BTreeSet;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<HecSinkMetricsConfig>();
    }

    #[test]
    fn test_encode_event_templated_counter_returns_expected_json() {
        let timestamp = DateTime::parse_from_rfc3339("2005-12-12T14:12:55.123-00:00")
            .unwrap()
            .with_timezone(&Utc);

        let metric = Metric::new(
            "example-counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 26.8 },
        )
        .with_timestamp(Some(timestamp))
        .with_tags(Some(btreemap! {
            "template_index".to_string() => "index_value".to_string(),
            "template_source".to_string() => "source_value".to_string(),
            "template_sourcetype".to_string() => "sourcetype_value".to_string(),
            "tag_one".to_string() => "tag_one_value".to_string(),
            "tag_two".to_string() => "tag_two_value".to_string(),
            "host".to_string() => "host_value".to_string(),
        }));

        let (config, _cx) = load_sink::<HecSinkMetricsConfig>(
            r#"
            endpoint = "https://splunk-hec.com/"
            token = "alksjdfo"
            host_key = "host"
            index = "{{ tags.template_index }}"
            source = "{{ tags.template_source }}"
            sourcetype = "{{ tags.template_sourcetype }}"
        "#,
        )
        .unwrap();

        let expected = json!({
            "time": 1134396775.123,
            "host": "host_value",
            "index": "index_value",
            "source": "source_value",
            "sourcetype": "sourcetype_value",
            "fields": {
                "host": "host_value",
                "tag_one": "tag_one_value",
                "tag_two": "tag_two_value",
                "metric_name": "example-counter",
                "_value": 26.8,
            },
            "event": "metric",
        });

        let actual =
            serde_json::from_slice::<JsonValue>(&config.encode_event(metric.into()).unwrap()[..])
                .unwrap();

        assert_eq!(expected, actual);
    }

    #[test]
    fn test_encode_event_static_counter_returns_expected_json() {
        let timestamp = DateTime::parse_from_rfc3339("2005-12-12T14:12:55.123-00:00")
            .unwrap()
            .with_timezone(&Utc);

        let metric = Metric::new(
            "example-counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 26.8 },
        )
        .with_timestamp(Some(timestamp))
        .with_tags(Some(btreemap! {
            "template_index".to_string() => "index_value".to_string(),
            "template_source".to_string() => "source_value".to_string(),
            "template_sourcetype".to_string() => "sourcetype_value".to_string(),
            "tag_one".to_string() => "tag_one_value".to_string(),
            "tag_two".to_string() => "tag_two_value".to_string(),
            "host".to_string() => "host_value".to_string(),
        }));

        let (config, _cx) = load_sink::<HecSinkMetricsConfig>(
            r#"
            endpoint = "https://splunk-hec.com/"
            token = "alksjdfo"
            host_key = "host"
            index = "index_value"
            source = "source_value"
            sourcetype = "sourcetype_value"
        "#,
        )
        .unwrap();

        let expected = json!({
            "time": 1134396775.123,
            "host": "host_value",
            "index": "index_value",
            "source": "source_value",
            "sourcetype": "sourcetype_value",
            "fields": {
                "host": "host_value",
                "tag_one": "tag_one_value",
                "tag_two": "tag_two_value",
                "template_index": "index_value",
                "template_source": "source_value",
                "template_sourcetype": "sourcetype_value",
                "metric_name": "example-counter",
                "_value": 26.8,
            },
            "event": "metric",
        });

        let actual =
            serde_json::from_slice::<JsonValue>(&config.encode_event(metric.into()).unwrap()[..])
                .unwrap();

        assert_eq!(expected, actual);
    }

    #[test]
    fn test_encode_event_gauge_no_namespace_returns_expected_json() {
        let timestamp = DateTime::parse_from_rfc3339("2005-12-12T14:12:55.123-00:00")
            .unwrap()
            .with_timezone(&Utc);

        let metric = Metric::new(
            "example-gauge",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 26.8 },
        )
        .with_timestamp(Some(timestamp));

        let (config, _cx) = load_sink::<HecSinkMetricsConfig>(
            r#"
            endpoint = "https://splunk-hec.com/"
            token = "alksjdfo"
        "#,
        )
        .unwrap();

        let expected = json!({
            "time": 1134396775.123,
            "fields": {
                "metric_name": "example-gauge",
                "_value": 26.8,
            },
            "event": "metric",
        });

        let actual =
            serde_json::from_slice::<JsonValue>(&config.encode_event(metric.into()).unwrap()[..])
                .unwrap();

        assert_eq!(expected, actual);
    }

    #[test]
    fn test_encode_event_gauge_with_namespace_returns_expected_json() {
        let timestamp = DateTime::parse_from_rfc3339("2005-12-12T14:12:55.123-00:00")
            .unwrap()
            .with_timezone(&Utc);

        let metric = Metric::new(
            "example-gauge",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 26.8 },
        )
        .with_timestamp(Some(timestamp))
        .with_namespace(Some("namespace"));

        let (config, _cx) = load_sink::<HecSinkMetricsConfig>(
            r#"
            endpoint = "https://splunk-hec.com/"
            token = "alksjdfo"
        "#,
        )
        .unwrap();

        let expected = json!({
            "time": 1134396775.123,
            "fields": {
                "metric_name": "namespace.example-gauge",
                "_value": 26.8,
            },
            "event": "metric",
        });

        let actual =
            serde_json::from_slice::<JsonValue>(&config.encode_event(metric.into()).unwrap()[..])
                .unwrap();

        assert_eq!(expected, actual);
    }

    #[test]
    fn test_encode_event_gauge_default_namespace_returns_expected_json() {
        let timestamp = DateTime::parse_from_rfc3339("2005-12-12T14:12:55.123-00:00")
            .unwrap()
            .with_timezone(&Utc);

        let metric = Metric::new(
            "example-gauge",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 26.8 },
        )
        .with_timestamp(Some(timestamp));

        let (config, _cx) = load_sink::<HecSinkMetricsConfig>(
            r#"
            default_namespace = "default"
            endpoint = "https://splunk-hec.com/"
            token = "alksjdfo"
        "#,
        )
        .unwrap();

        let expected = json!({
            "time": 1134396775.123,
            "fields": {
                "metric_name": "default.example-gauge",
                "_value": 26.8,
            },
            "event": "metric",
        });

        let actual =
            serde_json::from_slice::<JsonValue>(&config.encode_event(metric.into()).unwrap()[..])
                .unwrap();

        assert_eq!(expected, actual);
    }

    #[test]
    fn test_encode_event_gauge_overridden_namespace_returns_expected_json() {
        let timestamp = DateTime::parse_from_rfc3339("2005-12-12T14:12:55.123-00:00")
            .unwrap()
            .with_timezone(&Utc);

        let metric = Metric::new(
            "example-gauge",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 26.8 },
        )
        .with_timestamp(Some(timestamp))
        .with_namespace(Some("overridden"));

        let (config, _cx) = load_sink::<HecSinkMetricsConfig>(
            r#"
            default_namespace = "default"
            endpoint = "https://splunk-hec.com/"
            token = "alksjdfo"
        "#,
        )
        .unwrap();

        let expected = json!({
            "time": 1134396775.123,
            "fields": {
                "metric_name": "overridden.example-gauge",
                "_value": 26.8,
            },
            "event": "metric",
        });

        let actual =
            serde_json::from_slice::<JsonValue>(&config.encode_event(metric.into()).unwrap()[..])
                .unwrap();

        assert_eq!(expected, actual);
    }

    #[test]
    fn test_encode_event_unsupported_type_returns_none() {
        let mut values = BTreeSet::new();
        values.insert(String::from("value1"));

        let metric = Metric::new(
            "example-gauge",
            MetricKind::Absolute,
            MetricValue::Set { values },
        );

        let (config, _cx) = load_sink::<HecSinkMetricsConfig>(
            r#"
            endpoint = "https://splunk-hec.com/"
            token = "alksjdfo"
        "#,
        )
        .unwrap();

        assert!(config.encode_event(metric.into()).is_none());
    }
}

#[cfg(all(test, feature = "splunk-integration-tests"))]
mod integration_tests {
    use super::*;
    use crate::{
        config::{SinkConfig, SinkContext},
        event::{Metric, MetricKind},
        sinks::splunk_hec::common::integration_test_helpers::get_token,
        test_util::components::{self, HTTP_SINK_TAGS},
    };
    use serde_json::Value as JsonValue;
    use shared::btreemap;
    use std::convert::TryFrom;
    use vector_core::event::{BatchNotifier, BatchStatus};
    const USERNAME: &str = "admin";
    const PASSWORD: &str = "password";

    #[tokio::test]
    async fn splunk_insert_counter_metric() {
        let cx = SinkContext::new_test();

        let mut config = config().await;
        config.index = Template::try_from("testmetrics".to_string()).ok();
        let (sink, _) = config.build(cx).await.unwrap();

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let event = Metric::new(
            "example-counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 26.28 },
        )
        .with_tags(Some(
            btreemap! {"tag_one".to_string() => "tag_one_value".to_string()},
        ))
        .with_batch_notifier(&batch)
        .into();
        drop(batch);
        components::run_sink_event(sink, event, &HTTP_SINK_TAGS).await;
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        assert!(
            metric_dimensions_exist(
                "example-counter",
                &["host", "source", "sourcetype", "tag_one"],
            )
            .await
        );
    }

    #[tokio::test]
    async fn splunk_insert_gauge_metric() {
        let cx = SinkContext::new_test();

        let mut config = config().await;
        config.index = Template::try_from("testmetrics".to_string()).ok();
        let (sink, _) = config.build(cx).await.unwrap();

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let event = Metric::new(
            "example-gauge",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 26.28 },
        )
        .with_tags(Some(
            btreemap! {"tag_two".to_string() => "tag_two_value".to_string()},
        ))
        .with_batch_notifier(&batch)
        .into();
        drop(batch);
        components::run_sink_event(sink, event, &HTTP_SINK_TAGS).await;
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        assert!(
            metric_dimensions_exist(
                "example-gauge",
                &["host", "source", "sourcetype", "tag_two"],
            )
            .await
        );
    }

    // It usually takes ~1 second for the metric to show up in search with all dimensions, so poll
    // multiple times.
    async fn metric_dimensions_exist(metric_name: &str, expected_dimensions: &[&str]) -> bool {
        for _ in 0..20usize {
            let resp = metric_dimensions(metric_name).await;
            let actual_dimensions = resp
                .iter()
                .map(|d| d["name"].as_str().unwrap())
                .collect::<Vec<_>>();

            if expected_dimensions
                .iter()
                .all(|d| actual_dimensions.contains(d))
            {
                return true;
            }

            // if all dimensions not present, sleep and continue
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        false
    }

    async fn metric_dimensions(metric_name: &str) -> Vec<JsonValue> {
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();

        let res = client
            .get(format!(
                "https://localhost:8089/services/catalog/metricstore/dimensions?output_mode=json&metric_name={}",
                metric_name
            ))
            .basic_auth(USERNAME, Some(PASSWORD))
            .send()
            .await
            .unwrap();

        let json: JsonValue = res.json().await.unwrap();

        json["entry"].as_array().unwrap().clone()
    }

    async fn config() -> HecSinkMetricsConfig {
        HecSinkMetricsConfig {
            default_namespace: None,
            token: get_token().await,
            endpoint: "http://localhost:8088/".into(),
            host_key: "host".into(),
            index: None,
            sourcetype: None,
            source: None,
            compression: Compression::None,
            batch: BatchConfig {
                max_events: Some(1),
                ..Default::default()
            },
            request: TowerRequestConfig::default(),
            tls: None,
        }
    }
}

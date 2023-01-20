use std::collections::HashMap;

use bytes::Bytes;
use futures::{FutureExt, SinkExt};
use http::{Request, Uri};
use hyper::Body;
use serde_json::{json, map};
use snafu::Snafu;
use vector_config::configurable_component;

use crate::{
    codecs::Transformer,
    config::{log_schema, AcknowledgementsConfig, Input, SinkConfig, SinkContext},
    event::{Event, Value},
    gcp::{GcpAuthConfig, GcpAuthenticator, Scope},
    http::HttpClient,
    sinks::{
        gcs_common::config::healthcheck_response,
        util::{
            http::{BatchedHttpSink, HttpEventEncoder, HttpSink},
            BatchConfig, BoxedRawValue, JsonArrayBuffer, RealtimeSizeBasedDefaultBatchSettings,
            TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    template::{Template, TemplateRenderingError},
    tls::{TlsConfig, TlsSettings},
};

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("Resource not found"))]
    NotFound,
}

/// Configuration for the `gcp_stackdriver_logs` sink.
#[configurable_component(sink("gcp_stackdriver_logs"))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct StackdriverConfig {
    #[serde(skip, default = "default_endpoint")]
    endpoint: String,

    #[serde(flatten)]
    pub log_name: StackdriverLogName,

    /// The log ID to which to publish logs.
    ///
    /// This is a name you create to identify this log stream.
    pub log_id: Template,

    /// The monitored resource to associate the logs with.
    pub resource: StackdriverResource,

    /// The field of the log event from which to take the outgoing log’s `severity` field.
    ///
    /// The named field is removed from the log event if present, and must be either an integer
    /// between 0 and 800 or a string containing one of the [severity level names][sev_names] (case
    /// is ignored) or a common prefix such as `err`.
    ///
    /// If no severity key is specified, the severity of outgoing records is set to 0 (`DEFAULT`).
    ///
    /// See the [GCP Stackdriver Logging LogSeverity description][logsev_docs] for more details on
    /// the value of the `severity` field.
    ///
    /// [sev_names]: https://cloud.google.com/logging/docs/reference/v2/rest/v2/LogEntry#logseverity
    /// [logsev_docs]: https://cloud.google.com/logging/docs/reference/v2/rest/v2/LogEntry#logseverity
    pub severity_key: Option<String>,

    #[serde(flatten)]
    pub auth: GcpAuthConfig,

    #[configurable(derived)]
    #[serde(
        default,
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub encoding: Transformer,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

fn default_endpoint() -> String {
    "https://logging.googleapis.com/v2/entries:write".to_string()
}

#[derive(Clone, Debug)]
struct StackdriverSink {
    config: StackdriverConfig,
    auth: GcpAuthenticator,
    severity_key: Option<String>,
    uri: Uri,
}

// 10MB limit for entries.write: https://cloud.google.com/logging/quotas#api-limits
const MAX_BATCH_PAYLOAD_SIZE: usize = 10_000_000;

/// Logging locations.
#[configurable_component]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
pub enum StackdriverLogName {
    /// The billing account ID to which to publish logs.
    #[serde(rename = "billing_account_id")]
    BillingAccount(String),

    /// The folder ID to which to publish logs.
    ///
    /// See the [Google Cloud Platform folder documentation][folder_docs] for more details.
    ///
    /// [folder_docs]: https://cloud.google.com/resource-manager/docs/creating-managing-folders
    #[serde(rename = "folder_id")]
    Folder(String),

    /// The organization ID to which to publish logs.
    ///
    /// This would be the identifier assigned to your organization on Google Cloud Platform.
    #[serde(rename = "organization_id")]
    Organization(String),

    /// The project ID to which to publish logs.
    ///
    /// See the [Google Cloud Platform project management documentation][project_docs] for more details.
    ///
    /// [project_docs]: https://cloud.google.com/resource-manager/docs/creating-managing-projects
    #[derivative(Default)]
    #[serde(rename = "project_id")]
    Project(String),
}

/// A monitored resource.
///
/// Monitored resources in GCP allow associating logs and metrics specifically with native resources
/// within Google Cloud Platform. This takes the form of a "type" field which identifies the
/// resource, and a set of type-specific labels to uniquely identify a resource of that type.
///
/// See [Monitored resource types][mon_docs] for more information.
///
/// [mon_docs]: https://cloud.google.com/monitoring/api/resources
// TODO: this type is specific to the stackdrivers log sink because it allows for template-able
// label values, but we should consider replacing `sinks::gcp::GcpTypedResource` with this so both
// the stackdriver metrics _and_ logs sink can have template-able label values, and less duplication
#[configurable_component]
#[derive(Clone, Debug, Default)]
pub struct StackdriverResource {
    /// The monitored resource type.
    ///
    /// For example, the type of a Compute Engine VM instance is `gce_instance`.
    #[serde(rename = "type")]
    pub type_: String,

    /// Type-specific labels.
    #[serde(flatten)]
    #[configurable(metadata(docs::additional_props_description = "A type-specific label."))]
    pub labels: HashMap<String, Template>,
}

impl_generate_config_from_default!(StackdriverConfig);

#[async_trait::async_trait]
impl SinkConfig for StackdriverConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let auth = self.auth.build(Scope::LoggingWrite).await?;

        let batch = self
            .batch
            .validate()?
            .limit_max_bytes(MAX_BATCH_PAYLOAD_SIZE)?
            .into_batch_settings()?;
        let request = self.request.unwrap_with(&TowerRequestConfig {
            rate_limit_num: Some(1000),
            rate_limit_duration_secs: Some(1),
            ..Default::default()
        });
        let tls_settings = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(tls_settings, cx.proxy())?;

        let sink = StackdriverSink {
            config: self.clone(),
            auth,
            severity_key: self.severity_key.clone(),
            uri: self.endpoint.parse().unwrap(),
        };

        let healthcheck = healthcheck(client.clone(), sink.clone()).boxed();

        let sink = BatchedHttpSink::new(
            sink,
            JsonArrayBuffer::new(batch.size),
            request,
            batch.timeout,
            client,
        )
        .sink_map_err(|error| error!(message = "Fatal gcp_stackdriver_logs sink error.", %error));

        Ok((VectorSink::from_event_sink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

struct StackdriverEventEncoder {
    config: StackdriverConfig,
    severity_key: Option<String>,
}

impl HttpEventEncoder<serde_json::Value> for StackdriverEventEncoder {
    fn encode_event(&mut self, event: Event) -> Option<serde_json::Value> {
        let mut labels = HashMap::with_capacity(self.config.resource.labels.len());
        for (key, template) in &self.config.resource.labels {
            let value = template
                .render_string(&event)
                .map_err(|error| {
                    emit!(crate::internal_events::TemplateRenderingError {
                        error,
                        field: Some("resource.labels"),
                        drop_event: true,
                    });
                })
                .ok()?;
            labels.insert(key.clone(), value);
        }
        let log_name = self
            .config
            .log_name(&event)
            .map_err(|error| {
                emit!(crate::internal_events::TemplateRenderingError {
                    error,
                    field: Some("log_id"),
                    drop_event: true,
                });
            })
            .ok()?;

        let mut log = event.into_log();
        let severity = self
            .severity_key
            .as_ref()
            .and_then(|key| log.remove(key.as_str()))
            .map(remap_severity)
            .unwrap_or_else(|| 0.into());

        let mut event = Event::Log(log);
        self.config.encoding.transform(&mut event);

        let log = event.into_log();

        let mut entry = map::Map::with_capacity(5);
        entry.insert("logName".into(), json!(log_name));
        entry.insert("jsonPayload".into(), json!(log));
        entry.insert("severity".into(), json!(severity));
        entry.insert(
            "resource".into(),
            json!({
                "type": self.config.resource.type_,
                "labels": labels,
            }),
        );

        // If the event contains a timestamp, send it in the main message so gcp can pick it up.
        if let Some(timestamp) = log.get(log_schema().timestamp_key()) {
            entry.insert("timestamp".into(), json!(timestamp));
        }

        Some(json!(entry))
    }
}

#[async_trait::async_trait]
impl HttpSink for StackdriverSink {
    type Input = serde_json::Value;
    type Output = Vec<BoxedRawValue>;
    type Encoder = StackdriverEventEncoder;

    fn build_encoder(&self) -> Self::Encoder {
        StackdriverEventEncoder {
            config: self.config.clone(),
            severity_key: self.severity_key.clone(),
        }
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<Request<Bytes>> {
        let events = serde_json::json!({ "entries": events });

        let body = crate::serde::json::to_bytes(&events).unwrap().freeze();

        let mut request = Request::post(self.uri.clone())
            .header("Content-Type", "application/json")
            .body(body)
            .unwrap();
        self.auth.apply(&mut request);

        Ok(request)
    }
}

fn remap_severity(severity: Value) -> Value {
    let n = match severity {
        Value::Integer(n) => n - n % 100,
        Value::Bytes(s) => {
            let s = String::from_utf8_lossy(&s);
            match s.parse::<usize>() {
                Ok(n) => (n - n % 100) as i64,
                Err(_) => match s.to_uppercase() {
                    s if s.starts_with("EMERG") || s.starts_with("FATAL") => 800,
                    s if s.starts_with("ALERT") => 700,
                    s if s.starts_with("CRIT") => 600,
                    s if s.starts_with("ERR") || s == "ER" => 500,
                    s if s.starts_with("WARN") => 400,
                    s if s.starts_with("NOTICE") => 300,
                    s if s.starts_with("INFO") => 200,
                    s if s.starts_with("DEBUG") || s.starts_with("TRACE") => 100,
                    s if s.starts_with("DEFAULT") => 0,
                    _ => {
                        warn!(
                            message = "Unknown severity value string, using DEFAULT.",
                            value = %s,
                            internal_log_rate_limit = true
                        );
                        0
                    }
                },
            }
        }
        value => {
            warn!(
                message = "Unknown severity value type, using DEFAULT.",
                ?value,
                internal_log_rate_limit = true
            );
            0
        }
    };
    Value::Integer(n)
}

async fn healthcheck(client: HttpClient, sink: StackdriverSink) -> crate::Result<()> {
    let request = sink.build_request(vec![]).await?.map(Body::from);

    let response = client.send(request).await?;
    healthcheck_response(
        response,
        sink.auth.clone(),
        HealthcheckError::NotFound.into(),
    )
}

impl StackdriverConfig {
    fn log_name(&self, event: &Event) -> Result<String, TemplateRenderingError> {
        use StackdriverLogName::*;

        let log_id = self.log_id.render_string(event)?;

        Ok(match &self.log_name {
            BillingAccount(acct) => format!("billingAccounts/{}/logs/{}", acct, log_id),
            Folder(folder) => format!("folders/{}/logs/{}", folder, log_id),
            Organization(org) => format!("organizations/{}/logs/{}", org, log_id),
            Project(project) => format!("projects/{}/logs/{}", project, log_id),
        })
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use futures::{future::ready, stream};
    use indoc::indoc;
    use serde_json::value::RawValue;

    use super::*;
    use crate::{
        config::{GenerateConfig, SinkConfig, SinkContext},
        event::{LogEvent, Value},
        test_util::{
            components::{run_and_assert_sink_compliance, SINK_TAGS},
            http::{always_200_response, spawn_blackhole_http_server},
        },
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<StackdriverConfig>();
    }

    #[tokio::test]
    async fn component_spec_compliance() {
        let mock_endpoint = spawn_blackhole_http_server(always_200_response).await;

        let config = StackdriverConfig::generate_config().to_string();
        let mut config =
            toml::from_str::<StackdriverConfig>(&config).expect("config should be valid");

        // If we don't override the credentials path/API key, it tries to directly call out to the Google Instance
        // Metadata API, which we clearly don't have in unit tests. :)
        config.auth.credentials_path = None;
        config.auth.api_key = Some("fake".to_string().into());
        config.endpoint = mock_endpoint.to_string();

        let context = SinkContext::new_test();
        let (sink, _healthcheck) = config.build(context).await.unwrap();

        let event = Event::Log(LogEvent::from("simple message"));
        run_and_assert_sink_compliance(sink, stream::once(ready(event)), &SINK_TAGS).await;
    }

    #[test]
    fn encode_valid() {
        let config: StackdriverConfig = toml::from_str(indoc! {r#"
            project_id = "project"
            log_id = "{{ log_id }}"
            resource.type = "generic_node"
            resource.namespace = "office"
            resource.node_id = "{{ node_id }}"
            encoding.except_fields = ["anumber", "node_id", "log_id"]
        "#})
        .unwrap();

        let sink = StackdriverSink {
            config,
            auth: GcpAuthenticator::None,
            severity_key: Some("anumber".into()),
            uri: default_endpoint().parse().unwrap(),
        };
        let mut encoder = sink.build_encoder();

        let log = [
            ("message", "hello world"),
            ("anumber", "100"),
            ("node_id", "10.10.10.1"),
            ("log_id", "testlogs"),
        ]
        .iter()
        .copied()
        .collect::<LogEvent>();
        let json = encoder.encode_event(Event::from(log)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "logName":"projects/project/logs/testlogs",
                "jsonPayload":{"message":"hello world"},
                "severity":100,
                "resource":{
                    "type":"generic_node",
                    "labels":{"namespace":"office","node_id":"10.10.10.1"}
                }
            })
        );
    }

    #[test]
    fn encode_inserts_timestamp() {
        let config: StackdriverConfig = toml::from_str(indoc! {r#"
            project_id = "project"
            log_id = "testlogs"
            resource.type = "generic_node"
            resource.namespace = "office"
        "#})
        .unwrap();

        let sink = StackdriverSink {
            config,
            auth: GcpAuthenticator::None,
            severity_key: Some("anumber".into()),
            uri: default_endpoint().parse().unwrap(),
        };
        let mut encoder = sink.build_encoder();

        let mut log = LogEvent::default();
        log.insert("message", Value::Bytes("hello world".into()));
        log.insert("anumber", Value::Bytes("100".into()));
        log.insert(
            "timestamp",
            Value::Timestamp(
                Utc.ymd(2020, 1, 1)
                    .and_hms_opt(12, 30, 0)
                    .expect("invalid timestamp"),
            ),
        );

        let json = encoder.encode_event(Event::from(log)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "logName":"projects/project/logs/testlogs",
                "jsonPayload":{"message":"hello world","timestamp":"2020-01-01T12:30:00Z"},
                "severity":100,
                "resource":{
                    "type":"generic_node",
                    "labels":{"namespace":"office"}},
                "timestamp":"2020-01-01T12:30:00Z"
            })
        );
    }

    #[test]
    fn severity_remaps_strings() {
        for &(s, n) in &[
            ("EMERGENCY", 800), // Handles full upper case
            ("EMERG", 800),     // Handles abbreviations
            ("FATAL", 800),     // Handles highest alternate
            ("alert", 700),     // Handles lower case
            ("CrIt1c", 600),    // Handles mixed case and suffixes
            ("err404", 500),    // Handles lower case and suffixes
            ("warnings", 400),
            ("notice", 300),
            ("info", 200),
            ("DEBUG2", 100), // Handles upper case and suffixes
            ("trace", 100),  // Handles lowest alternate
            ("nothing", 0),  // Maps unknown terms to DEFAULT
            ("123", 100),    // Handles numbers in strings
            ("-100", 0),     // Maps negatives to DEFAULT
        ] {
            assert_eq!(
                remap_severity(s.into()),
                Value::Integer(n),
                "remap_severity({:?}) != {}",
                s,
                n
            );
        }
    }

    #[tokio::test]
    async fn correct_request() {
        let config: StackdriverConfig = toml::from_str(indoc! {r#"
            project_id = "project"
            log_id = "testlogs"
            resource.type = "generic_node"
            resource.namespace = "office"
        "#})
        .unwrap();

        let sink = StackdriverSink {
            config,
            auth: GcpAuthenticator::None,
            severity_key: None,
            uri: default_endpoint().parse().unwrap(),
        };
        let mut encoder = sink.build_encoder();

        let log1 = [("message", "hello")].iter().copied().collect::<LogEvent>();
        let log2 = [("message", "world")].iter().copied().collect::<LogEvent>();
        let event1 = encoder.encode_event(Event::from(log1)).unwrap();
        let event2 = encoder.encode_event(Event::from(log2)).unwrap();

        let json1 = serde_json::to_string(&event1).unwrap();
        let json2 = serde_json::to_string(&event2).unwrap();
        let raw1 = RawValue::from_string(json1).unwrap();
        let raw2 = RawValue::from_string(json2).unwrap();

        let events = vec![raw1, raw2];

        let request = sink.build_request(events).await.unwrap();

        let (parts, body) = request.into_parts();

        let json: serde_json::Value = serde_json::from_slice(&body[..]).unwrap();

        assert_eq!(
            &parts.uri.to_string(),
            "https://logging.googleapis.com/v2/entries:write"
        );
        assert_eq!(
            json,
            serde_json::json!({
                "entries": [
                    {
                        "logName": "projects/project/logs/testlogs",
                        "severity": 0,
                        "jsonPayload": {
                            "message": "hello"
                        },
                        "resource": {
                            "type": "generic_node",
                            "labels": {
                                "namespace": "office"
                            }
                        }
                    },
                    {
                        "logName": "projects/project/logs/testlogs",
                        "severity": 0,
                        "jsonPayload": {
                            "message": "world"
                        },
                        "resource": {
                            "type": "generic_node",
                            "labels": {
                                "namespace": "office"
                            }
                        }
                    }
                ]
            })
        );
    }

    #[tokio::test]
    async fn fails_missing_creds() {
        let config: StackdriverConfig = toml::from_str(indoc! {r#"
            project_id = "project"
            log_id = "testlogs"
            resource.type = "generic_node"
            resource.namespace = "office"
        "#})
        .unwrap();
        if config.build(SinkContext::new_test()).await.is_ok() {
            panic!("config.build failed to error");
        }
    }

    #[test]
    fn fails_invalid_log_names() {
        toml::from_str::<StackdriverConfig>(indoc! {r#"
            log_id = "testlogs"
            resource.type = "generic_node"
            resource.namespace = "office"
        "#})
        .expect_err("Config parsing failed to error with missing ids");

        toml::from_str::<StackdriverConfig>(indoc! {r#"
            project_id = "project"
            folder_id = "folder"
            log_id = "testlogs"
            resource.type = "generic_node"
            resource.namespace = "office"
        "#})
        .expect_err("Config parsing failed to error with extraneous ids");
    }
}

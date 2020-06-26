use super::{healthcheck_response, GcpAuthConfig, GcpCredentials, Scope};
use crate::{
    event::{Event, Value},
    sinks::{
        util::{
            encoding::{EncodingConfigWithDefault, EncodingConfiguration},
            http::{BatchedHttpSink, HttpClient, HttpSink},
            service2::TowerRequestConfig,
            BatchBytesConfig, BoxedRawValue, JsonArrayBuffer,
        },
        Healthcheck, RouterSink,
    },
    tls::{TlsOptions, TlsSettings},
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use futures::{FutureExt, TryFutureExt};
use futures01::Sink;
use http::{Request, Uri};
use hyper::Body;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::collections::HashMap;
use string_cache::DefaultAtom as Atom;

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("Resource not found"))]
    NotFound,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct StackdriverConfig {
    #[serde(flatten)]
    pub log_name: StackdriverLogName,
    pub log_id: String,

    pub resource: StackdriverResource,
    pub severity_key: Option<String>,

    #[serde(flatten)]
    pub auth: GcpAuthConfig,
    #[serde(
        skip_serializing_if = "crate::serde::skip_serializing_if_default",
        default
    )]
    pub encoding: EncodingConfigWithDefault<Encoding>,

    #[serde(default)]
    pub batch: BatchBytesConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,

    pub tls: Option<TlsOptions>,
}

#[derive(Clone, Debug)]
struct StackdriverSink {
    config: StackdriverConfig,
    creds: Option<GcpCredentials>,
    severity_key: Option<Atom>,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Default,
}

#[derive(Clone, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
pub enum StackdriverLogName {
    #[serde(rename = "billing_account_id")]
    BillingAccount(String),
    #[serde(rename = "folder_id")]
    Folder(String),
    #[serde(rename = "organization_id")]
    Organization(String),
    #[derivative(Default)]
    #[serde(rename = "project_id")]
    Project(String),
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct StackdriverResource {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(flatten)]
    pub labels: HashMap<String, String>,
}

inventory::submit! {
    SinkDescription::new::<StackdriverConfig>("gcp_stackdriver_logs")
}

lazy_static! {
    static ref REQUEST_DEFAULTS: TowerRequestConfig = TowerRequestConfig {
        rate_limit_num: Some(1000),
        rate_limit_duration_secs: Some(1),
        ..Default::default()
    };
    static ref URI: Uri = "https://logging.googleapis.com/v2/entries:write"
        .parse()
        .unwrap();
}

#[typetag::serde(name = "gcp_stackdriver_logs")]
impl SinkConfig for StackdriverConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(RouterSink, Healthcheck)> {
        let creds = self.auth.make_credentials(Scope::LoggingWrite)?;

        let batch = self.batch.unwrap_or(bytesize::kib(5000u64), 1);
        let request = self.request.unwrap_with(&REQUEST_DEFAULTS);
        let tls_settings = TlsSettings::from_options(&self.tls)?;

        let sink = StackdriverSink {
            config: self.clone(),
            creds,
            severity_key: self
                .severity_key
                .as_ref()
                .map(|key| Atom::from(key.as_str())),
        };

        let healthcheck = healthcheck(
            cx.clone(),
            sink.clone(),
            TlsSettings::from_options(&self.tls)?,
        )
        .boxed()
        .compat();

        let sink = BatchedHttpSink::new(
            sink,
            JsonArrayBuffer::default(),
            request,
            batch,
            tls_settings,
            &cx,
        )
        .sink_map_err(|e| error!("Fatal stackdriver sink error: {}", e));

        Ok((Box::new(sink), Box::new(healthcheck)))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "gcp_stackdriver_logs"
    }
}

#[async_trait::async_trait]
impl HttpSink for StackdriverSink {
    type Input = serde_json::Value;
    type Output = Vec<BoxedRawValue>;

    fn encode_event(&self, mut event: Event) -> Option<Self::Input> {
        self.config.encoding.apply_rules(&mut event);
        let mut log = event.into_log();
        let severity = self
            .severity_key
            .as_ref()
            .and_then(|key| log.remove(key))
            .map(remap_severity)
            .unwrap_or(0.into());

        let entry = serde_json::json!({
            "jsonPayload": log,
            "severity": severity,
        });

        Some(entry)
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<Request<Vec<u8>>> {
        let events = serde_json::json!({
            "log_name": self.config.log_name(),
            "entries": events,
            "resource": {
                "type": self.config.resource.type_,
                "labels": self.config.resource.labels,
            }
        });

        let body = serde_json::to_vec(&events).unwrap();

        let mut request = Request::post(URI.clone())
            .header("Content-Type", "application/json")
            .body(body)
            .unwrap();

        if let Some(creds) = &self.creds {
            creds.apply(&mut request);
        }

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
                    s if s.starts_with("ERR") => 500,
                    s if s.starts_with("WARN") => 400,
                    s if s.starts_with("NOTICE") => 300,
                    s if s.starts_with("INFO") => 200,
                    s if s.starts_with("DEBUG") || s.starts_with("TRACE") => 100,
                    s if s.starts_with("DEFAULT") => 0,
                    _ => {
                        warn!(
                            message = "Unknown severity value string, using DEFAULT",
                            value = %s,
                            rate_limit_secs = 10
                        );
                        0
                    }
                },
            }
        }
        value => {
            warn!(
                message = "Unknown severity value type, using DEFAULT",
                ?value,
                rate_limit_secs = 10
            );
            0
        }
    };
    Value::Integer(n)
}

async fn healthcheck(
    cx: SinkContext,
    sink: StackdriverSink,
    tls: TlsSettings,
) -> crate::Result<()> {
    let request = sink.build_request(vec![]).await?.map(Body::from);

    let mut client = HttpClient::new(cx.resolver(), tls)?;
    let response = client.send(request).await?;
    healthcheck_response(sink.creds.clone(), HealthcheckError::NotFound.into())(response)
}

impl StackdriverConfig {
    fn log_name(&self) -> String {
        use StackdriverLogName::*;
        match &self.log_name {
            BillingAccount(acct) => format!("billingAccounts/{}/logs/{}", acct, self.log_id),
            Folder(folder) => format!("folders/{}/logs/{}", folder, self.log_id),
            Organization(org) => format!("organizations/{}/logs/{}", org, self.log_id),
            Project(project) => format!("projects/{}/logs/{}", project, self.log_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::{LogEvent, Value},
        test_util::runtime,
    };
    use serde_json::value::RawValue;
    use std::iter::FromIterator;

    #[test]
    fn encode_valid() {
        let config: StackdriverConfig = toml::from_str(
            r#"
           project_id = "project"
           log_id = "testlogs"
           resource.type = "generic_node"
           resource.namespace = "office"
        "#,
        )
        .unwrap();

        let sink = StackdriverSink {
            config,
            creds: None,
            severity_key: Some("anumber".into()),
        };

        let log = LogEvent::from_iter(
            [("message", "hello world"), ("anumber", "100")]
                .iter()
                .map(|&s| s),
        );
        let json = sink.encode_event(Event::from(log)).unwrap();
        let body = serde_json::to_string(&json).unwrap();
        assert_eq!(
            body,
            "{\"jsonPayload\":{\"message\":\"hello world\"},\"severity\":100}"
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

    #[test]
    fn correct_request() {
        let config: StackdriverConfig = toml::from_str(
            r#"
           project_id = "project"
           log_id = "testlogs"
           resource.type = "generic_node"
           resource.namespace = "office"
        "#,
        )
        .unwrap();

        let sink = StackdriverSink {
            config,
            creds: None,
            severity_key: None,
        };

        let log1 = LogEvent::from_iter([("message", "hello")].iter().map(|&s| s));
        let log2 = LogEvent::from_iter([("message", "world")].iter().map(|&s| s));
        let event1 = sink.encode_event(Event::from(log1)).unwrap();
        let event2 = sink.encode_event(Event::from(log2)).unwrap();

        let json1 = serde_json::to_string(&event1).unwrap();
        let json2 = serde_json::to_string(&event2).unwrap();
        let raw1 = RawValue::from_string(json1).unwrap();
        let raw2 = RawValue::from_string(json2).unwrap();

        let events = vec![raw1, raw2];

        let mut rt = runtime();
        let request = rt
            .block_on_std(async move { sink.build_request(events).await })
            .unwrap();

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
                        "severity": 0,
                        "jsonPayload": {
                            "message": "hello"
                        }
                    },
                    {
                        "severity": 0,
                        "jsonPayload": {
                            "message": "world"
                        }
                    }
                ],
                "log_name": "projects/project/logs/testlogs",
                "resource": {
                    "labels": {
                        "namespace": "office",
                    },
                    "type": "generic_node"
                }
            })
        );
    }

    #[test]
    fn fails_missing_creds() {
        let config: StackdriverConfig = toml::from_str(
            r#"
           project_id = "project"
           log_id = "testlogs"
           resource.type = "generic_node"
           resource.namespace = "office"
        "#,
        )
        .unwrap();
        if config
            .build(SinkContext::new_test(runtime().executor()))
            .is_ok()
        {
            panic!("config.build failed to error");
        }
    }

    #[test]
    fn fails_invalid_log_names() {
        toml::from_str::<StackdriverConfig>(
            r#"
           log_id = "testlogs"
           resource.type = "generic_node"
           resource.namespace = "office"
        "#,
        )
        .expect_err("Config parsing failed to error with missing ids");

        toml::from_str::<StackdriverConfig>(
            r#"
           project_id = "project"
           folder_id = "folder"
           log_id = "testlogs"
           resource.type = "generic_node"
           resource.namespace = "office"
        "#,
        )
        .expect_err("Config parsing failed to error with extraneous ids");
    }
}

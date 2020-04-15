use super::{healthcheck_response, GcpAuthConfig, GcpCredentials, Scope};
use crate::{
    event::Event,
    sinks::{
        util::{
            encoding::{EncodingConfigWithDefault, EncodingConfiguration},
            http::{BatchedHttpSink, HttpClient, HttpSink},
            BatchBytesConfig, BoxedRawValue, JsonArrayBuffer, TowerRequestConfig,
        },
        Healthcheck, RouterSink,
    },
    tls::{TlsOptions, TlsSettings},
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use futures01::{Future, Sink};
use http::{Request, Uri};
use hyper::Body;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::collections::HashMap;
use tower::Service;

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
        };

        let healthcheck = self.healthcheck(&cx, sink.clone())?;

        let sink = BatchedHttpSink::new(
            sink,
            JsonArrayBuffer::default(),
            request,
            batch,
            tls_settings,
            &cx,
        )
        .sink_map_err(|e| error!("Fatal stackdriver sink error: {}", e));

        Ok((Box::new(sink), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "gcp_stackdriver_logs"
    }
}

impl HttpSink for StackdriverSink {
    type Input = serde_json::Value;
    type Output = Vec<BoxedRawValue>;

    fn encode_event(&self, mut event: Event) -> Option<Self::Input> {
        self.config.encoding.apply_rules(&mut event);

        let entry = serde_json::json!({
            "jsonPayload": event.into_log(),
        });

        Some(entry)
    }

    fn build_request(&self, events: Self::Output) -> http::Request<Vec<u8>> {
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

        request
    }
}

impl StackdriverConfig {
    fn healthcheck(&self, cx: &SinkContext, sink: StackdriverSink) -> crate::Result<Healthcheck> {
        let request = sink.build_request(vec![]).map(Body::from);

        let mut client = HttpClient::new(cx.resolver(), TlsSettings::from_options(&self.tls)?)?;

        let healthcheck = client
            .call(request)
            .map_err(Into::into)
            .and_then(healthcheck_response(
                sink.creds.clone(),
                HealthcheckError::NotFound.into(),
            ));

        Ok(Box::new(healthcheck))
    }

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
    use crate::{event::LogEvent, test_util::runtime};
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
        };

        let log = LogEvent::from_iter([("message", "hello world")].iter().map(|&s| s));
        let json = sink.encode_event(Event::from(log)).unwrap();
        let body = serde_json::to_string(&json).unwrap();
        assert_eq!(body, "{\"jsonPayload\":{\"message\":\"hello world\"}}");
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

        let request = sink.build_request(events);

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
                        "jsonPayload": {
                            "message": "hello"
                        }
                    },
                    {
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

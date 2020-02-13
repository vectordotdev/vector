use super::{healthcheck_response, GcpAuthConfig, GcpCredentials, Scope};
use crate::{
    event::{Event, Unflatten},
    sinks::{
        util::{
            http::{https_client, HttpRetryLogic, HttpService},
            tls::{TlsOptions, TlsSettings},
            BatchBytesConfig, Buffer, SinkExt, TowerRequestConfig,
        },
        Healthcheck, RouterSink,
    },
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use futures::{stream::iter_ok, Future, Sink};
use http::{Method, Uri};
use hyper::{Body, Request};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::collections::HashMap;

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

    #[serde(default)]
    pub batch: BatchBytesConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,

    pub tls: Option<TlsOptions>,
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
    SinkDescription::new::<StackdriverConfig>("gcp_stackdriver_logging")
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

#[typetag::serde(name = "gcp_stackdriver_logging")]
impl SinkConfig for StackdriverConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(RouterSink, Healthcheck)> {
        let creds = self.auth.make_credentials(Scope::LoggingWrite)?;
        let sink = self.service(&cx, &creds)?;
        let healthcheck = self.healthcheck(&cx, &creds)?;

        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "gcp_stackdriver_logging"
    }
}

impl StackdriverConfig {
    fn service(
        &self,
        cx: &SinkContext,
        creds: &Option<GcpCredentials>,
    ) -> crate::Result<RouterSink> {
        let batch = self.batch.unwrap_or(bytesize::kib(5000u64), 1);
        let request = self.request.unwrap_with(&REQUEST_DEFAULTS);

        let tls_settings = TlsSettings::from_options(&self.tls)?;
        let creds = creds.clone();

        // We need to cap the maximum length of the encoded request, so
        // we encode each event into JSON separately and then splice the
        // result into a relatively short wrapper.
        let wrapper = self.write_request();
        let wrapper_splice = wrapper
            .find(SPLICE_MAGIC)
            .expect("Unexpected encoded wrapper format")
            + SPLICE_OFFSET;

        let http_service = HttpService::builder(cx.resolver())
            .tls_settings(tls_settings)
            .build(move |mut logs: Vec<u8>| {
                logs.pop(); // Strip the trailing comma

                let mut body = wrapper.clone().into_bytes();
                body.splice(wrapper_splice..wrapper_splice, logs);

                let mut request = make_request(body);
                if let Some(creds) = creds.as_ref() {
                    creds.apply(&mut request);
                }

                request
            });

        let sink = request
            .batch_sink(HttpRetryLogic, http_service, cx.acker())
            .batched_with_min(Buffer::new(false), &batch)
            .with_flat_map(|event| iter_ok(Some(encode_event(event))));

        Ok(Box::new(sink))
    }

    fn healthcheck(
        &self,
        cx: &SinkContext,
        creds: &Option<GcpCredentials>,
    ) -> crate::Result<Healthcheck> {
        let mut request = make_request(Body::from(self.write_request().into_bytes()));

        if let Some(creds) = creds.as_ref() {
            creds.apply(&mut request);
        }

        let client = https_client(cx.resolver(), TlsSettings::from_options(&self.tls)?)?;
        let creds = creds.clone();
        let healthcheck =
            client
                .request(request)
                .map_err(Into::into)
                .and_then(healthcheck_response(
                    creds,
                    HealthcheckError::NotFound.into(),
                ));

        Ok(Box::new(healthcheck))
    }

    fn write_request(&self) -> String {
        let request = WriteRequest {
            log_name: self.log_name(),
            entries: vec![],
            resource: MonitoredResource {
                type_: self.resource.type_.clone(),
                labels: self.resource.labels.clone(),
            },
        };
        serde_json::to_string(&request).expect("Encoding write request failed")
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

fn make_request<T>(body: T) -> Request<T> {
    let mut builder = Request::builder();
    builder.method(Method::POST);
    builder.uri(URI.clone());
    builder.header("Content-Type", "application/json");
    builder.body(body).unwrap()
}

fn encode_event(event: Event) -> Vec<u8> {
    let entry = LogEntry {
        json_payload: event.into_log().unflatten(),
    };
    let mut json = serde_json::to_vec(&entry).unwrap();
    json.push(b',');
    json
}

// This is the magic search string within a JSON encoded
// `WriteRequest`. The batched entries will be spliced into the array.
const SPLICE_MAGIC: &str = "\"entries\":[]";
const SPLICE_OFFSET: usize = 11;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WriteRequest {
    log_name: String,
    entries: Vec<LogEntry>,
    resource: MonitoredResource,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LogEntry {
    json_payload: Unflatten,
}

#[derive(Serialize)]
struct MonitoredResource {
    #[serde(rename = "type")]
    type_: String,
    labels: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{event::LogEvent, test_util::runtime};
    use std::iter::FromIterator;

    #[test]
    fn valid_write_request() {
        let config: StackdriverConfig = toml::from_str(
            r#"
           project_id = "project"
           log_id = "testlogs"
           resource.type = "generic_node"
           resource.namespace = "office"
        "#,
        )
        .unwrap();
        let request = config.write_request();
        request
            .find("\"entries\":[]")
            .expect("Could not find required entries list");
    }

    #[test]
    fn encode_valid1() {
        let log = LogEvent::from_iter([("message", "hello world")].iter().map(|&s| s));
        let body = encode_event(log.into());
        let body = String::from_utf8_lossy(&body);
        assert_eq!(body, "{\"jsonPayload\":{\"message\":\"hello world\"}},");
    }

    #[test]
    fn encode_valid2() {
        let log1 = LogEvent::from_iter([("message", "hello world")].iter().map(|&s| s));
        let log2 = LogEvent::from_iter([("message", "killroy was here")].iter().map(|&s| s));
        let mut event = encode_event(log1.into());
        event.extend(encode_event(log2.into()));
        let body = String::from_utf8_lossy(&event);
        assert_eq!(
            body,
            "{\"jsonPayload\":{\"message\":\"hello world\"}},{\"jsonPayload\":{\"message\":\"killroy was here\"}},"
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

use crate::{
    event::Event,
    sinks::util::{
        http::{https_client, HttpRetryLogic, HttpService},
        tls::{TlsOptions, TlsSettings},
        BatchBytesConfig, Buffer, SinkExt, TowerRequestConfig,
    },
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use bytes::{BufMut, BytesMut};
use futures::{stream::iter_ok, Future, Sink, Stream};
use goauth::{auth::JwtClaims, auth::Token, credentials::Credentials, error::GOErr, scopes::Scope};
use http::{Method, Uri};
use hyper::{
    header::{HeaderValue, AUTHORIZATION},
    Body, Request,
};
use serde::{Deserialize, Serialize};
use smpl_jwt::Jwt;
use snafu::{ResultExt, Snafu};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::timer::Interval;

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct PubsubConfig {
    pub project: String,
    pub topic: String,
    pub emulator_host: Option<String>,
    pub api_key: Option<String>,
    pub credentials_path: Option<String>,

    #[serde(default)]
    pub batch: BatchBytesConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,

    pub tls: Option<TlsOptions>,
}

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("GCP pubsub sink requires one of api_key or credentials_path to be defined"))]
    MissingAuth,
    #[snafu(display("Invalid GCP credentials"))]
    InvalidCredentials { source: GOErr },
    #[snafu(display("Invalid RSA key in GCP credentials"))]
    InvalidRsaKey { source: GOErr },
    #[snafu(display("Failed to get OAuth token"))]
    GetTokenFailed { source: GOErr },
}

inventory::submit! {
    SinkDescription::new::<PubsubConfig>("gcp_pubsub")
}

#[typetag::serde(name = "gcp_pubsub")]
impl SinkConfig for PubsubConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        // We only need to load the credentials if we are not targetting an emulator.
        let creds = if self.emulator_host.is_none() {
            if self.api_key.is_none() && self.credentials_path.is_none() {
                return Err(BuildError::MissingAuth.into());
            }

            match self.credentials_path.as_ref() {
                Some(path) => Some(PubsubCreds::new(path)?),
                None => None,
            }
        } else {
            None
        };

        let sink = self.service(&cx, &creds)?;
        let healthcheck = self.healthcheck(&cx, &creds)?;

        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "gcp_pubsub"
    }
}

impl PubsubConfig {
    fn service(
        &self,
        cx: &SinkContext,
        creds: &Option<PubsubCreds>,
    ) -> crate::Result<super::RouterSink> {
        let batch = self.batch.unwrap_or(bytesize::mib(10u64), 1);
        let request = self.request.unwrap_with(&Default::default());

        let uri = self.uri(":publish")?;
        let tls_settings = TlsSettings::from_options(&self.tls)?;
        let creds = creds.clone();

        let http_service = HttpService::builder(cx.resolver())
            .tls_settings(tls_settings)
            .build(move |logs: Vec<u8>| {
                let mut builder = hyper::Request::builder();
                builder.method(Method::POST);
                builder.uri(uri.clone());
                builder.header("Content-Type", "application/json");

                let mut request = builder.body(make_body(logs)).unwrap();
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
        creds: &Option<PubsubCreds>,
    ) -> crate::Result<super::Healthcheck> {
        let uri = self.uri("")?;
        let mut request = Request::get(uri).body(Body::empty()).unwrap();
        if let Some(creds) = creds.as_ref() {
            creds.apply(&mut request);
        }

        let tls = TlsSettings::from_options(&self.tls)?;
        let client = https_client(cx.resolver(), tls)?;
        let creds = creds.clone();
        let healthcheck = client
            .request(request)
            .map_err(|err| err.into())
            .and_then(|response| match response.status() {
                hyper::StatusCode::OK => {
                    // If there are credentials configured, the
                    // generated OAuth token needs to be periodically
                    // regenerated. Since the health check runs at
                    // startup, after a successful health check is a
                    // good place to create the regeneration task.
                    creds.map(|creds| creds.spawn_regenerate_token());
                    Ok(())
                }
                status => Err(super::HealthcheckError::UnexpectedStatus { status }.into()),
            });

        Ok(Box::new(healthcheck))
    }

    fn uri(&self, suffix: &str) -> crate::Result<Uri> {
        let base = match self.emulator_host.as_ref() {
            Some(host) => format!("http://{}", host),
            None => "https://pubsub.googleapis.com".into(),
        };
        let uri = format!(
            "{}/v1/projects/{}/topics/{}{}",
            base, self.project, self.topic, suffix
        );
        let uri = match &self.api_key {
            Some(key) => format!("{}?key={}", uri, key),
            None => uri,
        };
        uri.parse::<Uri>()
            .context(super::UriParseError)
            .map_err(Into::into)
    }
}

#[derive(Clone)]
struct PubsubCreds {
    creds: Credentials,
    token: Arc<RwLock<Token>>,
}

impl PubsubCreds {
    fn new(path: &str) -> crate::Result<Self> {
        let creds = Credentials::from_file(path).context(InvalidCredentials)?;
        let jwt = make_jwt(&creds)?;
        let token = goauth::get_token_with_creds(&jwt, &creds).context(GetTokenFailed)?;
        let token = Arc::new(RwLock::new(token));
        Ok(Self { creds, token })
    }

    fn apply<T>(&self, request: &mut Request<T>) {
        let token = self.token.read().unwrap();
        let value = format!("{} {}", token.token_type(), token.access_token());
        request
            .headers_mut()
            .insert(AUTHORIZATION, HeaderValue::from_str(&value).unwrap());
    }

    fn regenerate_token(&self) -> crate::Result<()> {
        let jwt = make_jwt(&self.creds).unwrap(); // Errors caught above
        let token = goauth::get_token_with_creds(&jwt, &self.creds)?;
        *self.token.write().unwrap() = token;
        Ok(())
    }

    fn spawn_regenerate_token(&self) {
        let interval = self.token.read().unwrap().expires_in() as u64 / 2;
        let copy = self.clone();
        let renew_task = Interval::new_interval(Duration::from_secs(interval))
            .for_each(move |_instant| {
                debug!("Renewing GCP pubsub token");
                if let Err(error) = copy.regenerate_token() {
                    error!(message = "Failed to update GCP pubsub token", %error);
                }
                Ok(())
            })
            .map_err(
                |error| error!(message = "GCP pubsub token regenerate interval failed", %error),
            );

        tokio::spawn(renew_task);
    }
}

fn make_jwt(creds: &Credentials) -> crate::Result<Jwt<JwtClaims>> {
    let claims = JwtClaims::new(creds.iss(), &Scope::PubSub, creds.token_uri(), None, None);
    let rsa_key = creds.rsa_key().context(InvalidRsaKey)?;
    Ok(Jwt::new(claims, rsa_key, None))
}

const BODY_PREFIX: &str = "{\"messages\":[";
const BODY_SUFFIX: &str = "]}";

fn make_body(logs: Vec<u8>) -> Vec<u8> {
    // It would be cleaner to use serde_json, but doing it manually is
    // more efficient and not much more complicated.
    let mut body = BytesMut::with_capacity(logs.len() + BODY_PREFIX.len() + BODY_SUFFIX.len());
    body.put(BODY_PREFIX);
    if logs.len() > 0 {
        body.put(&logs[..logs.len() - 1]);
    }
    body.put(BODY_SUFFIX);

    body.into_iter().collect()
}

fn encode_event(event: Event) -> Vec<u8> {
    // Each event needs to be base64 encoded, and put into a JSON object
    // as the `data` item. A trailing comma is added to support multiple
    // events per request, and is stripped in `make_body`.
    let json = serde_json::to_string(&event.into_log().unflatten()).unwrap();
    format!("{{\"data\":\"{}\"}},", base64::encode(&json)).into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{assert_downcast_matches, event::LogEvent, test_util::runtime};
    use std::iter::FromIterator;

    #[test]
    fn encode_valid1() {
        let log = LogEvent::from_iter([("message", "hello world")].iter().map(|&s| s));
        let body = make_body(encode_event(log.into()));
        let body = String::from_utf8_lossy(&body);
        assert_eq!(
            body,
            "{\"messages\":[{\"data\":\"eyJtZXNzYWdlIjoiaGVsbG8gd29ybGQifQ==\"}]}"
        );
    }

    #[test]
    fn encode_valid2() {
        let log1 = LogEvent::from_iter([("message", "hello world")].iter().map(|&s| s));
        let log2 = LogEvent::from_iter([("message", "killroy was here")].iter().map(|&s| s));
        let mut event = encode_event(log1.into());
        event.extend(encode_event(log2.into()));
        let body = make_body(event);
        let body = String::from_utf8_lossy(&body);
        assert_eq!(
            body,
            "{\"messages\":[{\"data\":\"eyJtZXNzYWdlIjoiaGVsbG8gd29ybGQifQ==\"},{\"data\":\"eyJtZXNzYWdlIjoia2lsbHJveSB3YXMgaGVyZSJ9\"}]}"
        );
    }

    #[test]
    fn fails_missing_creds() {
        let config: PubsubConfig = toml::from_str(
            r#"
           project = "project"
           topic = "topic"
        "#,
        )
        .unwrap();
        match config.build(SinkContext::new_test(runtime().executor())) {
            Ok(_) => panic!("config.build failed to error"),
            Err(err) => assert_downcast_matches!(err, BuildError, BuildError::MissingAuth),
        }
    }
}

#[cfg(test)]
#[cfg(feature = "gcp-pubsub-integration-tests")]
mod integration_tests {
    use super::*;
    use crate::{
        runtime::Runtime,
        test_util::{block_on, random_events_with_stream, random_string},
    };
    use reqwest::{Client, Method, Response};
    use serde_json::{json, Value};

    const EMULATOR_HOST: &str = "localhost:8681";
    const PROJECT: &str = "testproject";

    fn config(topic: &str) -> PubsubConfig {
        PubsubConfig {
            emulator_host: Some(EMULATOR_HOST.into()),
            project: PROJECT.into(),
            topic: topic.into(),
            ..Default::default()
        }
    }

    fn config_build(
        rt: &Runtime,
        topic: &str,
    ) -> (crate::sinks::RouterSink, crate::sinks::Healthcheck) {
        let cx = SinkContext::new_test(rt.executor());
        config(topic).build(cx).expect("Building sink failed")
    }

    #[test]
    fn publish_events() {
        crate::test_util::trace_init();

        let rt = Runtime::new().unwrap();
        let (topic, subscription) = create_topic_subscription();
        let (sink, healthcheck) = config_build(&rt, &topic);

        block_on(healthcheck).expect("Health check failed");

        let (input, events) = random_events_with_stream(100, 100);

        let pump = sink.send_all(events);
        let _ = block_on(pump).expect("Sending events failed");

        let response = pull_messages(&subscription, 1000);
        let messages = response
            .receivedMessages
            .as_ref()
            .expect("Response is missing messages");
        assert_eq!(input.len(), messages.len());
        for i in 0..input.len() {
            let data = messages[i].message.decode_data();
            let data = serde_json::to_value(data).unwrap();
            let expected = serde_json::to_value(input[i].as_log().all_fields()).unwrap();
            assert_eq!(data, expected);
        }
    }

    #[test]
    fn checks_for_valid_topic() {
        let rt = Runtime::new().unwrap();
        let (topic, _subscription) = create_topic_subscription();
        let topic = format!("BAD{}", topic);
        let (_sink, healthcheck) = config_build(&rt, &topic);
        block_on(healthcheck).expect_err("Health check did not fail");
    }

    fn create_topic_subscription() -> (String, String) {
        let topic = format!("topic-{}", random_string(10));
        let subscription = format!("subscription-{}", random_string(10));
        request(Method::PUT, &format!("topics/{}", topic), json!({}))
            .json::<Value>()
            .expect("Creating new topic failed");
        request(
            Method::PUT,
            &format!("subscriptions/{}", subscription),
            json!({ "topic": format!("projects/{}/topics/{}", PROJECT, topic) }),
        )
        .json::<Value>()
        .expect("Creating new subscription failed");
        (topic, subscription)
    }

    fn request(method: Method, path: &str, json: Value) -> Response {
        let url = format!("http://{}/v1/projects/{}/{}", EMULATOR_HOST, PROJECT, path);
        Client::new()
            .request(method.clone(), &url)
            .json(&json)
            .send()
            .expect(&format!("Sending {} request to {} failed", method, url))
    }

    fn pull_messages(subscription: &str, count: usize) -> PullResponse {
        request(
            Method::POST,
            &format!("subscriptions/{}:pull", subscription),
            json!({
                "returnImmediately": true,
                "maxMessages": count
            }),
        )
        .json::<PullResponse>()
        .expect("Extracting pull data failed")
    }

    #[derive(Debug, Deserialize)]
    #[allow(non_snake_case)]
    struct PullResponse {
        receivedMessages: Option<Vec<PullMessageOuter>>,
    }

    #[derive(Debug, Deserialize)]
    #[allow(non_snake_case)]
    struct PullMessageOuter {
        ackId: String,
        message: PullMessage,
    }

    #[derive(Debug, Deserialize)]
    #[allow(non_snake_case)]
    struct PullMessage {
        data: String,
        messageId: String,
        publishTime: String,
    }

    impl PullMessage {
        fn decode_data(&self) -> TestMessage {
            let data = base64::decode(&self.data).expect("Invalid base64 data");
            let data = String::from_utf8_lossy(&data);
            serde_json::from_str(&data).expect("Invalid message structure")
        }
    }

    #[derive(Debug, Deserialize, Serialize)]
    struct TestMessage {
        timestamp: String,
        message: String,
    }
}

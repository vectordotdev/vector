use crate::{
    buffers::Acker,
    event::Event,
    sinks::util::{
        http::{HttpRetryLogic, HttpService},
        retries::FixedRetryPolicy,
        tls::{TlsOptions, TlsSettings},
        BatchConfig, BatchServiceSink, Buffer, SinkExt,
    },
    topology::config::{DataType, SinkConfig, SinkDescription},
};
use bytes::{BufMut, BytesMut};
use futures::{stream::iter_ok, Future, Sink, Stream};
use goauth::{auth::JwtClaims, auth::Token, credentials::Credentials, error::GOErr, scopes::Scope};
use http::{Method, Uri};
use hyper::{
    header::{HeaderValue, AUTHORIZATION},
    Body, Client, Request,
};
use hyper_tls::HttpsConnector;
use serde::{Deserialize, Serialize};
use smpl_jwt::Jwt;
use snafu::{ResultExt, Snafu};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::timer::Interval;
use tower::ServiceBuilder;

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct PubsubConfig {
    pub project: String,
    pub topic: String,
    pub api_key: Option<String>,
    pub credentials_path: Option<String>,

    #[serde(default, flatten)]
    pub batch: BatchConfig,

    // Tower Request based configuration
    pub request_in_flight_limit: Option<usize>,
    pub request_timeout_secs: Option<u64>,
    pub request_rate_limit_duration_secs: Option<u64>,
    pub request_rate_limit_num: Option<u64>,
    pub request_retry_attempts: Option<usize>,
    pub request_retry_backoff_secs: Option<u64>,

    pub tls: Option<TlsOptions>,
}

#[derive(Debug, Snafu)]
enum BuildError {
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
    fn build(&self, acker: Acker) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let creds = match self.credentials_path.as_ref() {
            Some(path) => Some(PubsubCreds::new(path)?),
            None => None,
        };

        let sink = self.service(acker, &creds)?;
        let healthcheck = self.healthcheck(&creds)?;

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
        acker: Acker,
        creds: &Option<PubsubCreds>,
    ) -> crate::Result<super::RouterSink> {
        let batch = self.batch.unwrap_or(bytesize::mib(10u64), 1);

        let timeout = self.request_timeout_secs.unwrap_or(60);
        let in_flight_limit = self.request_in_flight_limit.unwrap_or(5);
        let rate_limit_duration = self.request_rate_limit_duration_secs.unwrap_or(1);
        let rate_limit_num = self.request_rate_limit_num.unwrap_or(5);
        let retry_attempts = self.request_retry_attempts.unwrap_or(usize::max_value());
        let retry_backoff_secs = self.request_retry_backoff_secs.unwrap_or(1);

        let policy = FixedRetryPolicy::new(
            retry_attempts,
            Duration::from_secs(retry_backoff_secs),
            HttpRetryLogic,
        );

        let uri = self.uri(":publish")?;
        let tls_settings = TlsSettings::from_options(&self.tls)?;
        let creds = creds.clone();

        let http_service =
            HttpService::builder()
                .tls_settings(tls_settings)
                .build(move |logs: Vec<u8>| {
                    let mut builder = hyper::Request::builder();
                    builder.method(Method::POST);
                    builder.uri(uri.clone());
                    builder.header("Content-Type", "application/x-json");

                    let mut request = builder.body(make_body(logs)).unwrap();
                    if let Some(creds) = creds.as_ref() {
                        creds.apply(&mut request);
                    }

                    request
                });

        let service = ServiceBuilder::new()
            .concurrency_limit(in_flight_limit)
            .rate_limit(rate_limit_num, Duration::from_secs(rate_limit_duration))
            .retry(policy)
            .timeout(Duration::from_secs(timeout))
            .service(http_service);

        let sink = BatchServiceSink::new(service, acker)
            .batched_with_min(Buffer::new(false), &batch)
            .with_flat_map(|event| iter_ok(Some(encode_event(event))));

        Ok(Box::new(sink))
    }

    fn healthcheck(&self, creds: &Option<PubsubCreds>) -> crate::Result<super::Healthcheck> {
        let uri = self.uri("")?;
        let mut request = Request::get(uri).body(Body::empty()).unwrap();
        if let Some(creds) = creds.as_ref() {
            creds.apply(&mut request);
        }

        let https = HttpsConnector::new(4).expect("TLS initialization failed");
        let client = Client::builder().build(https);
        let creds = creds.clone();
        let healthcheck = client
            .request(request)
            .map_err(|err| err.into())
            .and_then(|response| match response.status() {
                hyper::StatusCode::OK => {
                    // If there are credentials configured, the
                    // generated token needs to be periodically
                    // regenerated.
                    // This is a bit of a hack, but I'm not sure where
                    // else to reliably spawn the regeneration task.
                    creds.map(|creds| creds.spawn_regenerate_token());
                    Ok(())
                }
                status => Err(super::HealthcheckError::UnexpectedStatus { status }.into()),
            });

        Ok(Box::new(healthcheck))
    }

    fn uri(&self, suffix: &str) -> crate::Result<Uri> {
        let uri = format!(
            "https://pubsub.googleapis.com/v1/projects/{}/topics/{}{}",
            self.project, self.topic, suffix
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

fn make_body(logs: Vec<u8>) -> Vec<u8> {
    let mut body = BytesMut::with_capacity(logs.len() + 16);
    body.put("{\"messages\":[");
    if logs.len() > 0 {
        body.put(&logs[..logs.len() - 1]);
    }
    body.put("]}");

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
    use crate::event::LogEvent;
    use std::iter::FromIterator;

    #[test]
    fn encode_valid1() {
        let log = LogEvent::from_iter([("message", "hello world")].into_iter().map(|&s| s));
        let body = make_body(encode_event(log.into()));
        let body = String::from_utf8_lossy(&body);
        assert_eq!(
            body,
            "{\"messages\":[{\"data\":\"eyJtZXNzYWdlIjoiaGVsbG8gd29ybGQifQ==\"}]}"
        );
    }

    #[test]
    fn encode_valid2() {
        let log1 = LogEvent::from_iter([("message", "hello world")].into_iter().map(|&s| s));
        let log2 = LogEvent::from_iter([("message", "killroy was here")].into_iter().map(|&s| s));
        let mut event = encode_event(log1.into());
        event.extend(encode_event(log2.into()));
        let body = make_body(event);
        let body = String::from_utf8_lossy(&body);
        assert_eq!(
            body,
            "{\"messages\":[{\"data\":\"eyJtZXNzYWdlIjoiaGVsbG8gd29ybGQifQ==\"},{\"data\":\"eyJtZXNzYWdlIjoia2lsbHJveSB3YXMgaGVyZSJ9\"}]}"
        );
    }
}

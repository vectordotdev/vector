use super::util::{self, SinkExt};
use futures::{Future, Sink};
use hyper::{Request, Uri};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::record::Record;

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct HttpSinkConfig {
    pub base_uri: String,
    #[serde(default)]
    pub path: String,
    #[serde(flatten)]
    pub basic_auth: Option<BasicAuth>,
    #[serde(default)]
    pub healthcheck_path: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct BasicAuth {
    user: String,
    password: String,
}

struct ValidatedConfig {
    uri: Uri,
    healthcheck_uri: Uri,
    basic_auth: Option<BasicAuth>,
}

impl HttpSinkConfig {
    fn validated(&self) -> Result<ValidatedConfig, String> {
        Ok(ValidatedConfig {
            uri: self.uri()?,
            healthcheck_uri: self.healthcheck_uri()?,
            basic_auth: self.basic_auth.clone(),
        })
    }

    fn uri(&self) -> Result<Uri, String> {
        self.build_uri(&self.path)
    }

    fn healthcheck_uri(&self) -> Result<Uri, String> {
        self.build_uri(&self.healthcheck_path)
    }

    fn build_uri(&self, path: &str) -> Result<Uri, String> {
        let base: Uri = self
            .base_uri
            .parse()
            .map_err(|e| format!("invalid base_uri: {}", e))?;
        let path: Uri = path.parse().map_err(|e| format!("invalid path: {}", e))?;
        Ok(Uri::builder()
            .scheme(base.scheme_str().unwrap_or("http"))
            .authority(
                base.authority_part()
                    .map(|a| a.as_str())
                    .unwrap_or("127.0.0.1"),
            )
            .path_and_query(path.path_and_query().map(|pq| pq.as_str()).unwrap_or(""))
            .build()
            .expect("bug building uri"))
    }
}

#[typetag::serde(name = "http")]
impl crate::topology::config::SinkConfig for HttpSinkConfig {
    fn build(&self) -> Result<(super::RouterSink, super::Healthcheck), String> {
        Ok((http(self.validated()?), healthcheck(self.validated()?)))
    }
}

fn http(config: ValidatedConfig) -> super::RouterSink {
    let sink = util::http::HttpSink::new()
        .with(move |body: Vec<u8>| {
            let request = Request::post(&config.uri)
                .header("Content-Type", "application/json")
                .header("Content-Encoding", "gzip")
                .body(body.into())
                .unwrap();

            Ok(request)
        })
        .size_buffered(2 * 1024 * 1024, true)
        .with(move |record: Record| {
            let mut body = json!({
                "msg": record.line,
                "ts": record.timestamp.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                "fields": record.custom,
            });
            if let Some(host) = record.host {
                body["host"] = json!(host);
            }
            let mut body = serde_json::to_vec(&body).unwrap();
            body.push(b'\n');
            Ok(body)
        });

    Box::new(sink)
}

fn healthcheck(config: ValidatedConfig) -> super::Healthcheck {
    use hyper::{Body, Client, Request};
    use hyper_tls::HttpsConnector;

    let request = Request::head(&config.healthcheck_uri)
        .body(Body::empty())
        .unwrap();

    let https = HttpsConnector::new(4).expect("TLS initialization failed");
    let client = Client::builder().build(https);

    let healthcheck = client
        .request(request)
        .map_err(|err| err.to_string())
        .and_then(|response| {
            use hyper::StatusCode;

            match response.status() {
                StatusCode::OK => Ok(()),
                other => Err(format!("Unexpected status: {}", other)),
            }
        });

    Box::new(healthcheck)
}

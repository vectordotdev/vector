use super::util;
use super::util::SinkExt;
use futures::{Future, Sink};
use hyper::{Request, Uri};
use serde_derive::{Deserialize, Serialize};
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

impl HttpSinkConfig {
    fn uri(&self) -> Uri {
        self.build_uri(&self.path)
    }

    fn healthcheck_uri(&self) -> Uri {
        self.build_uri(&self.healthcheck_path)
    }

    fn build_uri(&self, path: &str) -> Uri {
        let base: Uri = self.base_uri.parse().unwrap();
        let path: Uri = path.parse().unwrap();
        Uri::builder()
            .scheme(base.scheme_str().unwrap_or("http"))
            .authority(
                base.authority_part()
                    .map(|a| a.as_str())
                    .unwrap_or("127.0.0.1"),
            )
            .path_and_query(path.path_and_query().map(|pq| pq.as_str()).unwrap_or(""))
            .build()
            .unwrap()
    }
}

#[typetag::serde(name = "http")]
impl crate::topology::config::SinkConfig for HttpSinkConfig {
    fn build(&self) -> Result<(super::RouterSink, super::Healthcheck), String> {
        Ok((http(self.clone()), healthcheck(self.clone())))
    }
}

pub fn http(config: HttpSinkConfig) -> super::RouterSink {
    let sink = util::http::HttpSink::new()
        .with(move |body: Vec<u8>| {
            let request = Request::post(config.uri())
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

pub fn healthcheck(config: HttpSinkConfig) -> super::Healthcheck {
    use hyper::{Body, Client, Request};
    use hyper_tls::HttpsConnector;

    let request = Request::head(config.healthcheck_uri())
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

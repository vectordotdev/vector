use super::util::{self, SinkExt};
use futures::{future, Future, Sink};
use headers::HeaderMapExt;
use hyper::{Request, Uri};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::record::Record;

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct HttpSinkConfig {
    pub uri: String,
    pub healthcheck_uri: Option<String>,
    #[serde(flatten)]
    pub basic_auth: Option<BasicAuth>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct BasicAuth {
    user: String,
    password: String,
}

impl BasicAuth {
    fn apply(&self, header_map: &mut http::header::HeaderMap) {
        let auth = headers::Authorization::basic(&self.user, &self.password);
        header_map.typed_insert(auth)
    }
}

#[derive(Clone, Debug)]
struct ValidatedConfig {
    uri: Uri,
    healthcheck_uri: Option<Uri>,
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
        build_uri(&self.uri)
    }

    fn healthcheck_uri(&self) -> Result<Option<Uri>, String> {
        if let Some(uri) = &self.healthcheck_uri {
            build_uri(uri).map(Some)
        } else {
            Ok(None)
        }
    }
}

fn build_uri(raw: &str) -> Result<Uri, String> {
    let base: Uri = raw
        .parse()
        .map_err(|e| format!("invalid uri ({}): {:?}", e, raw))?;
    Ok(Uri::builder()
        .scheme(base.scheme_str().unwrap_or("http"))
        .authority(
            base.authority_part()
                .map(|a| a.as_str())
                .unwrap_or("127.0.0.1"),
        )
        .path_and_query(base.path_and_query().map(|pq| pq.as_str()).unwrap_or(""))
        .build()
        .expect("bug building uri"))
}

#[typetag::serde(name = "http")]
impl crate::topology::config::SinkConfig for HttpSinkConfig {
    fn build(&self) -> Result<(super::RouterSink, super::Healthcheck), String> {
        let config = self.validated()?;
        let sink = http(config.clone());

        if let Some(healthcheck_uri) = config.healthcheck_uri {
            Ok((sink, healthcheck(healthcheck_uri, config.basic_auth)))
        } else {
            Ok((sink, Box::new(future::ok(()))))
        }
    }
}

fn http(config: ValidatedConfig) -> super::RouterSink {
    let sink = util::http::HttpSink::new()
        .with(move |body: Vec<u8>| {
            let mut request = Request::post(&config.uri)
                .header("Content-Type", "application/x-ndjson")
                .header("Content-Encoding", "gzip")
                .body(body.into())
                .unwrap();

            if let Some(ref auth) = config.basic_auth {
                auth.apply(request.headers_mut());
            }

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

fn healthcheck(uri: Uri, auth: Option<BasicAuth>) -> super::Healthcheck {
    use hyper::{Body, Client, Request};
    use hyper_tls::HttpsConnector;

    let mut request = Request::head(&uri).body(Body::empty()).unwrap();

    if let Some(auth) = auth {
        auth.apply(request.headers_mut());
    }

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

use super::util::{self, Buffer, SinkExt};
use futures::{future, Future, Sink};
use headers::HeaderMapExt;
use hyper::Uri;
use indexmap::IndexMap;
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
    pub headers: Option<IndexMap<String, String>>,
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
    headers: Option<IndexMap<String, String>>,
}

impl HttpSinkConfig {
    fn validated(&self) -> Result<ValidatedConfig, String> {
        Ok(ValidatedConfig {
            uri: self.uri()?,
            healthcheck_uri: self.healthcheck_uri()?,
            basic_auth: self.basic_auth.clone(),
            headers: self.headers.clone(),
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
        .with(move |body: Buffer| {
            let mut request = util::http::Request::post(config.uri.clone(), body.into());
            request
                .header("Content-Type", "application/x-ndjson")
                .header("Content-Encoding", "gzip");

            if let Some(headers) = &config.headers {
                for (header, value) in headers.iter() {
                    request.header(header, value);
                }
            }

            if let Some(auth) = &config.basic_auth {
                auth.apply(&mut request.headers);
            }

            Ok(request)
        })
        .batched(Buffer::new(true), 2 * 1024 * 1024)
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

#[cfg(test)]
mod tests {
    use crate::{
        sinks::http::HttpSinkConfig,
        test_util::{next_addr, random_lines, shutdown_on_idle},
        topology::config::SinkConfig,
    };
    use bytes::Buf;
    use futures::{stream, sync::mpsc, Future, Sink, Stream};
    use headers::{Authorization, HeaderMapExt};
    use hyper::service::service_fn_ok;
    use hyper::{Body, Request, Response, Server};
    use std::io::{BufRead, BufReader};

    #[test]
    fn test_http_happy_path() {
        let num_lines = 1000;

        let in_addr = next_addr();

        let config = r#"
        uri = "http://$IN_ADDR/frames"
        user = "waldo"
        password = "hunter2"
    "#
        .replace("$IN_ADDR", &format!("{}", in_addr));

        let config: HttpSinkConfig = toml::from_str(&config).unwrap();

        let (sink, _healthcheck) = config.build().unwrap();

        let (tx, rx) = mpsc::unbounded();
        let service = move || {
            let tx = tx.clone();
            service_fn_ok(move |req: Request<Body>| {
                let (parts, body) = req.into_parts();

                let tx = tx.clone();
                tokio::spawn(
                    body.concat2()
                        .map_err(|e| panic!(e))
                        .and_then(|body| tx.send((parts, body)))
                        .map(|_| ())
                        .map_err(|e| panic!(e)),
                );

                Response::new(Body::empty())
            })
        };

        let (trigger, tripwire) = stream_cancel::Tripwire::new();
        let server = Server::bind(&in_addr)
            .serve(service)
            .with_graceful_shutdown(tripwire)
            .map_err(|e| panic!("server error: {}", e));

        let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();

        let pump = sink.send_all(stream::iter_ok::<_, ()>(
            input_lines
                .clone()
                .into_iter()
                .map(|line| crate::Record::from(line)),
        ));

        let mut rt = tokio::runtime::Runtime::new().unwrap();

        rt.spawn(server);

        let (mut sink, _) = rt.block_on(pump).unwrap();
        rt.block_on(futures::future::poll_fn(move || sink.close()))
            .unwrap();

        drop(trigger);

        let output_lines = rx
            .wait()
            .map(Result::unwrap)
            .map(|(parts, body)| {
                assert_eq!(hyper::Method::POST, parts.method);
                assert_eq!("/frames", parts.uri.path());
                assert_eq!(
                    Some(Authorization::basic("waldo", "hunter2")),
                    parts.headers.typed_get()
                );
                body
            })
            .map(hyper::Chunk::reader)
            .map(flate2::read::GzDecoder::new)
            .map(BufReader::new)
            .flat_map(BufRead::lines)
            .map(Result::unwrap)
            .map(|s| {
                let val: serde_json::Value = serde_json::from_str(&s).unwrap();
                val.get("msg").unwrap().as_str().unwrap().to_owned()
            })
            .collect::<Vec<_>>();

        shutdown_on_idle(rt);

        assert_eq!(num_lines, output_lines.len());
        assert_eq!(input_lines, output_lines);
    }

    #[test]
    fn passes_custom_headers() {
        let num_lines = 1000;

        let in_addr = next_addr();

        let config = r#"
        uri = "http://$IN_ADDR/frames"
        [headers]
        foo = "bar"
        baz = "quux"
    "#
        .replace("$IN_ADDR", &format!("{}", in_addr));

        let config: HttpSinkConfig = toml::from_str(&config).unwrap();

        let (sink, _healthcheck) = config.build().unwrap();

        let (tx, rx) = mpsc::unbounded();
        let service = move || {
            let tx = tx.clone();
            service_fn_ok(move |req: Request<Body>| {
                let (parts, body) = req.into_parts();

                let tx = tx.clone();
                tokio::spawn(
                    body.concat2()
                        .map_err(|e| panic!(e))
                        .and_then(|body| tx.send((parts, body)))
                        .map(|_| ())
                        .map_err(|e| panic!(e)),
                );

                Response::new(Body::empty())
            })
        };

        let (trigger, tripwire) = stream_cancel::Tripwire::new();
        let server = Server::bind(&in_addr)
            .serve(service)
            .with_graceful_shutdown(tripwire)
            .map_err(|e| panic!("server error: {}", e));

        let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();

        let pump = sink.send_all(stream::iter_ok::<_, ()>(
            input_lines
                .clone()
                .into_iter()
                .map(|line| crate::Record::from(line)),
        ));

        let mut rt = tokio::runtime::Runtime::new().unwrap();

        rt.spawn(server);

        let (mut sink, _) = rt.block_on(pump).unwrap();
        rt.block_on(futures::future::poll_fn(move || sink.close()))
            .unwrap();

        drop(trigger);

        let output_lines = rx
            .wait()
            .map(Result::unwrap)
            .map(|(parts, body)| {
                assert_eq!(hyper::Method::POST, parts.method);
                assert_eq!("/frames", parts.uri.path());
                assert_eq!(
                    Some("bar"),
                    parts.headers.get("foo").map(|v| v.to_str().unwrap())
                );
                assert_eq!(
                    Some("quux"),
                    parts.headers.get("baz").map(|v| v.to_str().unwrap())
                );
                body
            })
            .map(hyper::Chunk::reader)
            .map(flate2::read::GzDecoder::new)
            .map(BufReader::new)
            .flat_map(BufRead::lines)
            .map(Result::unwrap)
            .map(|s| {
                let val: serde_json::Value = serde_json::from_str(&s).unwrap();
                val.get("msg").unwrap().as_str().unwrap().to_owned()
            })
            .collect::<Vec<_>>();

        shutdown_on_idle(rt);

        assert_eq!(num_lines, output_lines.len());
        assert_eq!(input_lines, output_lines);
    }
}

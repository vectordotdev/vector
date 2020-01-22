use crate::{
    event::Event,
    topology::config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
};
use futures::{future, stream, sync::mpsc, Future, Sink, Stream};
use headers::HeaderMapExt;
use http::{
    header::{self, HeaderName, HeaderValue},
    Method, Uri,
};
use hyper::{server::Builder, service::service_fn, Body, Request, Response, Server, StatusCode};
use indexmap::IndexMap;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::{net::SocketAddr, sync::Arc};

#[derive(Deserialize, Serialize, Clone, Debug, Default, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HttpSourceConfig {
    pub addr: String,
    pub healthcheck_uri: Option<String>,
    // #[serde(flatten)]
    // pub basic_auth: Option<BasicAuth>,
    // pub headers: Option<IndexMap<String, String>>,
    // pub compression: Option<Compression>,
    // pub tls: Option<TlsOptions>,
    pub encoding: Encoding,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Text,
    Ndjson,
    Json,
}

inventory::submit! {
    SourceDescription::new_without_default::<HttpSourceConfig>("http")
}

#[typetag::serde(name = "http")]
impl SourceConfig for HttpSourceConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        out: mpsc::Sender<Event>,
    ) -> crate::Result<super::Source> {
        http_source(self, out)
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "http"
    }
}

type ResponseFut<T, E> = Box<dyn Future<Item = T, Error = E> + Send>;

fn http_source(
    config: &HttpSourceConfig,
    out: mpsc::Sender<Event>,
) -> crate::Result<super::Source> {
    let addr = config
        .addr
        .parse::<SocketAddr>()
        .expect("address parse failed");

    let fut = future::lazy(move || {
        println!("Start Http server {:?}", addr);
        let (tx, rx) = mpsc::channel(1024);
        let new_service = move || {
            let tx = tx.clone();
            service_fn(
                move |req: Request<Body>| -> ResponseFut<Response<Body>, hyper::Error> {
                    match (req.method(), req.uri().path()) {
                        (&Method::POST, "/") => {
                            println!("1 - service fn called {:?}", req);
                            let tx = tx.clone();
                            Box::new(
                                req.into_body()
                                    .fold::<_, _, Result<_, hyper::Error>>(
                                        vec![],
                                        |mut acc, chunk| {
                                            acc.extend_from_slice(&chunk);
                                            Ok(acc)
                                        },
                                    )
                                    .and_then(move |v| {
                                        let s = String::from_utf8(v)
                                            .expect("Conversion to UTF-8 failed");
                                        tx.send(s).map_err(|_| panic!("Send failed"))
                                    })
                                    .and_then(|_| {
                                        futures::future::ok(Response::new(Body::from("")))
                                    }),
                            )
                        }
                        _ => {
                            let mut response = Response::new(Body::empty());
                            *response.status_mut() = StatusCode::NOT_FOUND;
                            Box::new(future::ok(response))
                        }
                    }
                },
            )
        };
        let server = Server::bind(&addr)
            .serve(new_service)
            .map(|_| ())
            .map_err(|e| eprintln!("server error {:?}", e));

        tokio::spawn(server);
        rx.map(|msg| {
            println!("2 - Received event {:?}", msg);
            Event::from(msg)
        })
        .forward(out.sink_map_err(|e| eprintln!("{:?}", e)))
        .map(|_| ())
    });
    Ok(Box::new(fut))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event, runtime,
        sources::file,
        test_util::{block_on, collect_n, random_events_with_stream, shutdown_on_idle},
        topology::Config,
    };
    use futures::{Future, Stream};
    use hyper::Client;
    use pretty_assertions::assert_eq;
    use std::{thread, time::Duration};
    use stream_cancel::Tripwire;
    use tokio::util::FutureExt;

    fn default_config() -> HttpSourceConfig {
        HttpSourceConfig {
            addr: "127.0.0.1:6000".into(),
            ..Default::default()
        }
    }

    #[test]
    fn http_src_parse_config() {
        let config = toml::from_str::<HttpSourceConfig>(
            r#"
        "#,
        )
        .unwrap();
        assert_eq!(config, HttpSourceConfig::default());
        let config: HttpSourceConfig = toml::from_str(
            r#"
        addr = "localhost:6000"
        healthcheck_uri = "localhost:3000"
        encoding = "json"
            "#,
        )
        .unwrap();
        assert_eq!(
            config,
            HttpSourceConfig {
                addr: "localhost:6000".into(),
                healthcheck_uri: Some("localhost:3000".into()),
                encoding: Encoding::Json
            }
        );
    }

    #[test]
    fn http_src_create() {
        let config = default_config();
        assert!(http_source(&config, mpsc::channel(1).0).is_ok());
    }

    #[test]
    fn http_src_happy_path() {
        let config = default_config();

        let (tx, rx) = mpsc::channel(100);
        let mut rt = runtime::Runtime::new().expect("creating runtime failed");

        let source = http_source(&config, tx);
        thread::sleep(Duration::from_millis(1000));
        rt.spawn(source.expect("Building source failed"));

        thread::sleep(Duration::from_millis(1000));
        println!("Listening on 6000");

        for _ in 0..10 {
            rt.spawn(post_json(config.addr.parse().unwrap()));
        }

        let recv = rt.block_on(collect_n(rx, 10)).expect("block failed");
        println!("Got all events - {:?}", recv);
    }

    fn post_json(url: hyper::Uri) -> impl Future<Item = (), Error = ()> {
        let client = Client::new();
        let req = Request::builder()
            .method(Method::POST)
            .uri(url)
            .header(hyper::header::CONTENT_TYPE, "application/json")
            .body(Body::empty())
            .unwrap();
        client
            .request(req)
            .and_then(|res| {
                // asynchronously concatenate chunks of the body
                res.into_body().concat2()
            })
            .and_then(|body| {
                let users = String::from_utf8(body.to_vec()).unwrap();
                Ok(users)
            })
            .map(|_| ())
            .map_err(|e| eprintln!("error occured during send {}", e))
    }
}

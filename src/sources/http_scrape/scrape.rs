//! Generalized HTTP scrape source.
//! Scrapes an endpoint at an interval, decoding the HTTP responses into events.

use bytes::{Bytes, BytesMut};
use chrono::Utc;
use futures_util::FutureExt;
use http::{response::Parts, Uri};
use snafu::ResultExt;
use std::collections::HashMap;
use tokio_util::codec::Decoder as _;

use crate::{
    codecs::{Decoder, DecodingConfig},
    config::{SourceConfig, SourceContext, SourceDescription},
    http::Auth,
    serde::default_decoding,
    serde::default_framing_message_based,
    sources,
    tls::{TlsConfig, TlsSettings},
    Result,
};
use codecs::{
    decoding::{DeserializerConfig, FramingConfig},
    StreamDecodingError,
};
use vector_config::configurable_component;
use vector_core::{
    config::{log_schema, LogNamespace, Output},
    event::Event,
};

/// The name of this source
const NAME: &str = "http_scrape";

/// Configuration for the `http_scrape` source.
#[configurable_component(source)]
#[derive(Clone, Debug)]
pub struct HttpScrapeConfig {
    /// Endpoint to scrape events from.
    endpoint: String,

    /// The interval between scrapes, in seconds.
    #[serde(default = "super::default_scrape_interval_secs")]
    scrape_interval_secs: u64,

    /// Custom parameters for the scrape request query string.
    ///
    /// One or more values for the same parameter key can be provided. The parameters provided in this option are
    /// appended to any parameters manually provided in the `endpoint` option.
    query: Option<HashMap<String, Vec<String>>>,

    /// Decoder to use on the HTTP responses.
    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    decoding: DeserializerConfig,

    /// Framing to use in the decoding.
    #[configurable(derived)]
    #[serde(default = "default_framing_message_based")]
    framing: FramingConfig,

    /// Headers to apply to the HTTP requests.
    #[serde(default)]
    headers: Option<HashMap<String, String>>,

    /// TLS configuration.
    #[configurable(derived)]
    tls: Option<TlsConfig>,

    /// HTTP Authentication.
    #[configurable(derived)]
    auth: Option<Auth>,
}

impl Default for HttpScrapeConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:9898/logs".to_string(),
            query: None,
            scrape_interval_secs: super::default_scrape_interval_secs(),
            decoding: default_decoding(),
            framing: default_framing_message_based(),
            headers: None,
            tls: None,
            auth: None,
        }
    }
}

inventory::submit! {
    SourceDescription::new::<HttpScrapeConfig>(NAME)
}

impl_generate_config_from_default!(HttpScrapeConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "http_scrape")]
impl SourceConfig for HttpScrapeConfig {
    async fn build(&self, cx: SourceContext) -> Result<sources::Source> {
        // build the url
        let endpoints = vec![self.endpoint.clone()];
        let urls = endpoints
            .iter()
            .map(|s| s.parse::<Uri>().context(sources::UriParseSnafu))
            .map(|r| r.map(|uri| super::get_url(&uri, &self.query)))
            .collect::<std::result::Result<Vec<Uri>, sources::BuildError>>()?;

        let tls = TlsSettings::from_options(&self.tls)?;

        // build the decoder
        let decoder = DecodingConfig::new(
            self.framing.clone(),
            self.decoding.clone(),
            LogNamespace::Vector,
        )
        .build();

        let content_type = self.decoding.content_type(&self.framing).to_string();

        // the only specific context needed is the codec decoding
        let context = HttpScrapeContext { decoder };

        let inputs = super::GenericHttpScrapeInputs {
            urls,
            interval_secs: self.scrape_interval_secs,
            headers: self.headers.clone(),
            content_type,
            auth: self.auth.clone(),
            tls,
            proxy: cx.proxy.clone(),
            shutdown: cx.shutdown,
        };

        Ok(super::http_scrape(inputs, context, cx.out).boxed())
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<Output> {
        vec![Output::default(self.decoding.output_type())]
    }

    fn source_type(&self) -> &'static str {
        NAME
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

#[derive(Clone)]
struct HttpScrapeContext {
    decoder: Decoder,
}

impl HttpScrapeContext {
    /// Decode the events from the byte buffer
    fn decode_events(&mut self, buf: &mut BytesMut) -> Vec<Event> {
        let mut events = Vec::new();
        loop {
            match self.decoder.decode_eof(buf) {
                Ok(Some((next, _))) => {
                    events.extend(next.into_iter());
                }
                Ok(None) => break,
                Err(error) => {
                    // Error is logged by `crate::codecs::Decoder`, no further
                    // handling is needed here.
                    if !error.can_continue() {
                        break;
                    }
                    break;
                }
            }
        }
        events
    }

    /// Enriches events with source_type, timestamp
    fn enrich_events(&self, events: &mut Vec<Event>) {
        for event in events {
            match event {
                Event::Log(ref mut log) => {
                    log.try_insert(log_schema().source_type_key(), Bytes::from(NAME));
                    log.try_insert(log_schema().timestamp_key(), Utc::now());
                }
                Event::Metric(ref mut metric) => {
                    metric.insert_tag(log_schema().source_type_key().to_string(), NAME.to_string());
                }
                Event::Trace(ref mut trace) => {
                    trace.insert(log_schema().source_type_key(), Bytes::from(NAME));
                }
            }
        }
    }
}

impl super::HttpScraper for HttpScrapeContext {
    /// Decodes the HTTP response body into events per the decoder configured.
    fn on_response(
        &mut self,
        _url: &http::Uri,
        _header: &Parts,
        body: &Bytes,
    ) -> Option<Vec<Event>> {
        // get the body into a byte array
        let mut buf = BytesMut::new();
        let body = String::from_utf8_lossy(body);
        buf.extend_from_slice(body.as_bytes());

        // decode and enrich
        let mut events = self.decode_events(&mut buf);
        self.enrich_events(&mut events);

        Some(events)
    }
}

#[cfg(test)]
mod test {
    use futures::{poll, StreamExt};
    use std::task::Poll;
    use tokio::time::{sleep, Duration};
    use warp::Filter;

    use super::*;
    use crate::{
        test_util::{
            components::{run_and_assert_source_compliance, HTTP_PULL_SOURCE_TAGS},
            next_addr, test_generate_config,
        },
        SourceSender,
    };

    #[test]
    fn http_scrape_generate_config() {
        test_generate_config::<HttpScrapeConfig>();
    }

    // I haven't seen a better way to validate an error occurred, but it seems like there should be
    // a way, since if this is run live it generates an HTTP error.
    #[tokio::test]
    async fn invalid_endpoint() {
        let source = HttpScrapeConfig {
            endpoint: "http://nope".to_string(),
            scrape_interval_secs: 1,
            query: None,
            decoding: default_decoding(),
            framing: default_framing_message_based(),
            headers: None,
            auth: None,
            tls: None,
        };

        // Build the source and set ourselves up to both drive it to completion as well as collect all the events it sends out.
        let (tx, mut rx) = SourceSender::new_test();
        let context = SourceContext::new_test(tx, None);

        let source = source
            .build(context)
            .await
            .expect("source should not fail to build");

        sleep(Duration::from_secs(1)).await;

        drop(source);

        assert_eq!(poll!(rx.next()), Poll::Ready(None));
    }

    async fn run_test(config: HttpScrapeConfig) -> Vec<Event> {
        let events = run_and_assert_source_compliance(
            config,
            Duration::from_secs(1),
            &HTTP_PULL_SOURCE_TAGS,
        )
        .await;
        assert!(!events.is_empty());
        events
    }

    #[tokio::test]
    async fn bytes_decoding() {
        let in_addr = next_addr();

        let dummy_endpoint = warp::path!("endpoint")
            .and(warp::header::exact("Accept", "text/plain"))
            .map(|| r#"A plain text event"#);

        tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));

        run_test(HttpScrapeConfig {
            endpoint: format!("http://{}/endpoint", in_addr),
            scrape_interval_secs: 1,
            query: None,
            decoding: default_decoding(),
            framing: default_framing_message_based(),
            headers: None,
            auth: None,
            tls: None,
        })
        .await;
    }

    #[tokio::test]
    async fn json_decoding() {
        let in_addr = next_addr();

        let dummy_endpoint = warp::path!("endpoint")
            .and(warp::header::exact("Accept", "text/plain"))
            .map(|| r#"{"data" : "foo"}"#);

        tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));

        run_test(HttpScrapeConfig {
            endpoint: format!("http://{}/endpoint", in_addr),
            scrape_interval_secs: 1,
            query: None,
            decoding: DeserializerConfig::Json,
            framing: default_framing_message_based(),
            headers: None,
            auth: None,
            tls: None,
        })
        .await;
    }

    #[tokio::test]
    async fn request_query_applied() {
        let in_addr = next_addr();

        let dummy_endpoint = warp::path!("endpoint")
            .and(warp::query::raw())
            .map(|query| format!(r#"{{"data" : "{}"}}"#, query));

        tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));

        let events = run_test(HttpScrapeConfig {
            endpoint: format!("http://{}/endpoint?key1=val1", in_addr),
            scrape_interval_secs: 1,
            query: Some(HashMap::from([
                ("key1".to_string(), vec!["val2".to_string()]),
                (
                    "key2".to_string(),
                    vec!["val1".to_string(), "val2".to_string()],
                ),
            ])),
            decoding: DeserializerConfig::Json,
            framing: default_framing_message_based(),
            headers: None,
            auth: None,
            tls: None,
        })
        .await;

        let logs: Vec<_> = events.into_iter().map(|event| event.into_log()).collect();

        let expected = HashMap::from([
            (
                "key1".to_string(),
                vec!["val1".to_string(), "val2".to_string()],
            ),
            (
                "key2".to_string(),
                vec!["val1".to_string(), "val2".to_string()],
            ),
        ]);

        for log in logs {
            let query = log.get("data").expect("data must be available");
            let mut got: HashMap<String, Vec<String>> = HashMap::new();
            for (k, v) in url::form_urlencoded::parse(
                query.as_bytes().expect("byte conversion should succeed"),
            ) {
                got.entry(k.to_string())
                    .or_insert_with(Vec::new)
                    .push(v.to_string());
            }
            for v in got.values_mut() {
                v.sort();
            }
            assert_eq!(got, expected);
        }
    }
}

#[cfg(all(test, feature = "http-scrape-integration-tests"))]
mod integration_tests {
    use tokio::time::Duration;

    use super::*;
    use crate::test_util::components::{run_and_assert_source_compliance, HTTP_PULL_SOURCE_TAGS};

    async fn run_test(config: HttpScrapeConfig) -> Vec<Event> {
        let events = run_and_assert_source_compliance(
            config,
            Duration::from_secs(1),
            &HTTP_PULL_SOURCE_TAGS,
        )
        .await;
        assert!(!events.is_empty());
        events
    }

    #[tokio::test]
    async fn scraped_logs_bytes() {
        run_test(HttpScrapeConfig {
            endpoint: "http://dufs:5000/logs/bytes".to_string(),
            scrape_interval_secs: 1,
            query: None,
            decoding: DeserializerConfig::Bytes,
            framing: default_framing_message_based(),
            headers: None,
            auth: None,
            tls: None,
        })
        .await;
    }

    #[tokio::test]
    async fn scraped_logs_json() {
        let events = run_test(HttpScrapeConfig {
            endpoint: "http://dufs:5000/logs/json.json".to_string(),
            scrape_interval_secs: 1,
            query: None,
            decoding: DeserializerConfig::Json,
            framing: default_framing_message_based(),
            headers: None,
            auth: None,
            tls: None,
        })
        .await;
        let log = events[0].as_log();
        assert_eq!(log[log_schema().source_type_key()], NAME.into());
    }

    #[tokio::test]
    async fn scraped_metrics_native_json() {
        let events = run_test(HttpScrapeConfig {
            endpoint: "http://dufs:5000/metrics/native.json".to_string(),
            scrape_interval_secs: 1,
            query: None,
            decoding: DeserializerConfig::NativeJson,
            framing: default_framing_message_based(),
            headers: None,
            auth: None,
            tls: None,
        })
        .await;

        let metric = events[0].as_metric();
        assert_eq!(
            metric.tags().unwrap()[log_schema().source_type_key()],
            NAME.to_string()
        );
    }

    #[tokio::test]
    async fn scraped_trace_native_json() {
        let events = run_test(HttpScrapeConfig {
            endpoint: "http://dufs:5000/traces/native.json".to_string(),
            scrape_interval_secs: 1,
            query: None,
            decoding: DeserializerConfig::NativeJson,
            framing: default_framing_message_based(),
            headers: None,
            auth: None,
            tls: None,
        })
        .await;

        let trace = events[0].as_trace();
        assert_eq!(trace.as_map()[log_schema().source_type_key()], NAME.into());
    }

    #[tokio::test]
    async fn unauthorized() {
        // TODO how to assert failure

        // let config = HttpScrapeConfig {
        //     endpoint: format!("http://dufs-auth:5000/auth/json.json"),
        //     scrape_interval_secs: 1,
        //     query: None,
        //     decoding: DeserializerConfig::NativeJson,
        //     framing: default_framing_message_based(),
        //     headers: None,
        //     auth: None,
        //     tls: None,
        // };
    }

    #[tokio::test]
    async fn authorized() {
        run_test(HttpScrapeConfig {
            endpoint: "http://dufs-auth:5000/logs/json.json".to_string(),
            scrape_interval_secs: 1,
            query: None,
            decoding: DeserializerConfig::Json,
            framing: default_framing_message_based(),
            headers: None,
            auth: Some(Auth::Basic {
                user: "user".to_string(),
                password: "pass".to_string(),
            }),
            tls: None,
        })
        .await;
    }

    #[tokio::test]
    async fn headers() {
        // TODO - is this worthy of testing and how to verify ?
    }

    #[tokio::test]
    async fn tls() {
        // TODO fix this, as it is there is an error from dufs with "Sending fatal alert
        // BadRecordMac"

        // and in vector error is:
        //
        // 2022-08-01T19:11:23.382932Z ERROR vector::internal_events::http_client: HTTP error. error=error trying to connect: error:1416F086:SSL routines:tls_process_server_certificate:certificate verify failed:ssl/statem/statem_clnt.c:1919:: self signed certificate error_type="request_failed" stage="processing"
        // 2022-08-01T19:11:23.383435Z ERROR vector::internal_events::http_scrape: HTTP request processing error. url=https://dufs-https:5000/logs/json.json error=CallRequest { source: hyper::Error(Connect, Custom { kind: Other, error: ConnectError { error: Error { code: ErrorCode(1), cause: Some(Ssl(ErrorStack([Error { code: 337047686, library: "SSL routines", function: "tls_process_server_certificate", reason: "certificate verify failed", file: "ssl/statem/statem_clnt.c", line: 1919 }]))) }, verify_result: X509VerifyResult { code: 18, error: "self signed certificate" } } }) } error_type="request_failed" stage="receiving" internal_log_rate_secs=10

        // let cert_path = "tests/data/ca/certs/ca.cert.pem";
        // let key_path = "tests/data/ca/private/ca.key.pem";

        // run_test(HttpScrapeConfig {
        //     endpoint: "https://dufs-https:5000/logs/json.json".to_string(),
        //     scrape_interval_secs: 1,
        //     query: None,
        //     decoding: DeserializerConfig::Json,
        //     framing: default_framing_message_based(),
        //     headers: None,
        //     auth: None,
        //     tls: Some(TlsConfig {
        //         crt_file: Some(cert_path.into()),
        //         key_file: Some(key_path.into()),
        //         ..Default::default()
        //     }),
        // })
        // .await;
    }

    #[tokio::test]
    async fn shutdown() {
        // TODO - is this worthy of testing and how to verify
    }
}

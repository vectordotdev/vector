use crate::{
    config::{
        log_schema, DataType, GenerateConfig, Resource, SourceConfig, SourceContext,
        SourceDescription,
    },
    event::Event,
    internal_events::HttpDecompressError,
    sources,
    tls::{MaybeTlsSettings, TlsConfig},
    Pipeline,
};
use bytes::{Buf, Bytes};
use chrono::Utc;
use flate2::read::{DeflateDecoder, MultiGzDecoder};
use futures::{FutureExt, SinkExt, StreamExt, TryFutureExt};
use http::StatusCode;
use regex::Regex;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::{error::Error, fmt, io::Read, net::SocketAddr, sync::Arc};
use vector_core::event::{BatchNotifier, BatchStatus, LogEvent};
use warp::{
    filters::BoxedFilter, path, path::FullPath, reject::Rejection, reply::Response, Filter, Reply,
};

#[derive(Clone, Copy, Debug, Snafu)]
pub(crate) enum ApiError {
    BadRequest,
    InvalidDataFormat,
    ServerShutdown,
}

impl warp::reject::Reject for ApiError {}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct DatadogAgentConfig {
    address: SocketAddr,
    tls: Option<TlsConfig>,
    #[serde(default = "crate::serde::default_true")]
    store_api_key: bool,
}

inventory::submit! {
    SourceDescription::new::<DatadogAgentConfig>("datadog_agent")
}

impl GenerateConfig for DatadogAgentConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            address: "0.0.0.0:8080".parse().unwrap(),
            tls: None,
            store_api_key: true,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "datadog_agent")]
impl SourceConfig for DatadogAgentConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<sources::Source> {
        let source = DatadogAgentSource::new(self.store_api_key);

        let tls = MaybeTlsSettings::from_config(&self.tls, true)?;
        let listener = tls.bind(&self.address).await?;
        let service = source.event_service(cx.acknowledgements, cx.out.clone());

        let shutdown = cx.shutdown;
        Ok(Box::pin(async move {
            let span = crate::trace::current_span();
            let routes = service
                .with(warp::trace(move |_info| span.clone()))
                .recover(|r: Rejection| async move {
                    if let Some(e_msg) = r.find::<ErrorMessage>() {
                        let json = warp::reply::json(e_msg);
                        Ok(warp::reply::with_status(json, e_msg.status_code()))
                    } else {
                        // other internal error - will return 500 internal server error
                        Err(r)
                    }
                });
            warp::serve(routes)
                .serve_incoming_with_graceful_shutdown(
                    listener.accept_stream(),
                    shutdown.map(|_| ()),
                )
                .await;

            Ok(())
        }))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "datadog_agent"
    }

    fn resources(&self) -> Vec<Resource> {
        vec![Resource::tcp(self.address)]
    }
}

#[derive(Clone)]
struct DatadogAgentSource {
    store_api_key: bool,
    api_key_matcher: Regex,
    log_schema_timestamp_key: &'static str,
    log_schema_source_type_key: &'static str,
}

impl DatadogAgentSource {
    fn new(store_api_key: bool) -> Self {
        Self {
            store_api_key,
            api_key_matcher: Regex::new(
                r"^(/v1/input|/api/v2/logs)/(?P<api_key>[[:alnum:]]{32})/??",
            )
            .expect("static regex always compiles"),
            log_schema_source_type_key: log_schema().source_type_key(),
            log_schema_timestamp_key: log_schema().timestamp_key(),
        }
    }

    fn extract_api_key(&self, path: &str, header: Option<String>) -> Option<Arc<str>> {
        // Grab from URL first
        self.api_key_matcher
            .captures(path)
            .and_then(|cap| cap.name("api_key").map(|key| key.as_str()).map(Arc::from))
            // Try from header next
            .or_else(|| header.map(Arc::from))
    }

    async fn handle_request(
        events: Result<Vec<Event>, ErrorMessage>,
        acknowledgements: bool,
        mut out: Pipeline,
    ) -> Result<Response, Rejection> {
        match events {
            Ok(mut events) => {
                let receiver = acknowledgements.then(|| {
                    let (batch, receiver) = BatchNotifier::new_with_receiver();
                    for event in &mut events {
                        event.add_batch_notifier(Arc::clone(&batch));
                    }
                    receiver
                });

                let mut events = futures::stream::iter(events).map(Ok);
                out.send_all(&mut events)
                    .map_err(move |error: crate::pipeline::ClosedError| {
                        // can only fail if receiving end disconnected, so we are shutting down,
                        // probably not gracefully.
                        error!(message = "Failed to forward events, downstream is closed.");
                        error!(message = "Tried to send the following event.", %error);
                        warp::reject::custom(ApiError::ServerShutdown)
                    })
                    .await?;
                match receiver {
                    None => Ok(warp::reply().into_response()),
                    Some(receiver) => match receiver.await {
                        BatchStatus::Delivered => Ok(warp::reply().into_response()),
                        BatchStatus::Errored => Err(warp::reject::custom(ErrorMessage::new(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "Error delivering contents to sink".into(),
                        ))),
                        BatchStatus::Failed => Err(warp::reject::custom(ErrorMessage::new(
                            StatusCode::BAD_REQUEST,
                            "Contents failed to deliver to sink".into(),
                        ))),
                    },
                }
            }
            Err(err) => Err(warp::reject::custom(err)),
        }
    }

    fn event_service(self, acknowledgements: bool, out: Pipeline) -> BoxedFilter<(Response,)> {
        warp::post()
            .and(path!("v1" / "input" / ..).or(path!("api" / "v2" / "logs" / ..)))
            .and(warp::path::full())
            .and(warp::header::optional::<String>("content-encoding"))
            .and(warp::header::optional::<String>("dd-api-key"))
            .and(warp::body::bytes())
            .and_then(
                move |_,
                      path: FullPath,
                      encoding_header: Option<String>,
                      api_token: Option<String>,
                      body: Bytes| {
                    let token: Option<Arc<str>> = if self.store_api_key {
                        self.extract_api_key(path.as_str(), api_token)
                    } else {
                        None
                    };
                    let events = decode(&encoding_header, body)
                        .and_then(|body| self.decode_body(body, token));
                    Self::handle_request(events, acknowledgements, out.clone())
                },
            )
            .boxed()
    }

    fn decode_body(
        &self,
        body: Bytes,
        api_key: Option<Arc<str>>,
    ) -> Result<Vec<Event>, ErrorMessage> {
        if body.is_empty() {
            // The datadog agent may send an empty payload as a keep alive
            debug!(
                message = "Empty payload ignored.",
                internal_log_rate_secs = 30
            );
            return Ok(Vec::new());
        }

        let messages: Vec<LogMsg> = serde_json::from_slice(&body).map_err(|error| {
            ErrorMessage::new(
                StatusCode::BAD_REQUEST,
                format!("Error parsing JSON: {:?}", error),
            )
        })?;

        let now = Utc::now();
        Ok(messages
            .into_iter()
            .map(|msg| {
                let mut log = LogEvent::default();
                log.insert_flat(self.log_schema_timestamp_key, now);
                log.insert_flat(
                    self.log_schema_source_type_key,
                    Bytes::from("datadog_agent"),
                );
                log.insert_flat("message".to_string(), msg.message);
                log.insert_flat("status".to_string(), msg.status);
                log.insert_flat("timestamp".to_string(), msg.timestamp);
                log.insert_flat("hostname".to_string(), msg.hostname);
                log.insert_flat("service".to_string(), msg.service);
                log.insert_flat("ddsource".to_string(), msg.ddsource);
                log.insert_flat("ddtags".to_string(), msg.ddtags);
                if let Some(k) = &api_key {
                    log.metadata_mut().set_datadog_api_key(Some(Arc::clone(k)));
                }
                log
            })
            .map(|log| log.into())
            .collect())
    }
}

fn decode(header: &Option<String>, mut body: Bytes) -> Result<Bytes, ErrorMessage> {
    if let Some(encodings) = header {
        for encoding in encodings.rsplit(',').map(str::trim) {
            body = match encoding {
                "identity" => body,
                "gzip" | "x-gzip" => {
                    let mut decoded = Vec::new();
                    MultiGzDecoder::new(body.reader())
                        .read_to_end(&mut decoded)
                        .map_err(|error| handle_decode_error(encoding, error))?;
                    decoded.into()
                }
                "deflate" | "x-deflate" => {
                    let mut decoded = Vec::new();
                    DeflateDecoder::new(body.reader())
                        .read_to_end(&mut decoded)
                        .map_err(|error| handle_decode_error(encoding, error))?;
                    decoded.into()
                }
                encoding => {
                    return Err(ErrorMessage::new(
                        StatusCode::UNSUPPORTED_MEDIA_TYPE,
                        format!("Unsupported encoding {}", encoding),
                    ))
                }
            }
        }
    }

    Ok(body)
}

fn handle_decode_error(encoding: &str, error: impl std::error::Error) -> ErrorMessage {
    emit!(HttpDecompressError {
        encoding,
        error: &error
    });
    ErrorMessage::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        format!("Failed decompressing payload with {} decoder.", encoding),
    )
}

#[derive(Serialize, Debug)]
pub struct ErrorMessage {
    code: u16,
    message: String,
}

impl ErrorMessage {
    pub fn new(code: StatusCode, message: String) -> Self {
        ErrorMessage {
            code: code.as_u16(),
            message,
        }
    }

    pub fn status_code(&self) -> StatusCode {
        StatusCode::from_u16(self.code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

impl Error for ErrorMessage {}

impl fmt::Display for ErrorMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl warp::reject::Reject for ErrorMessage {}

// https://github.com/DataDog/datadog-agent/blob/a33248c2bc125920a9577af1e16f12298875a4ad/pkg/logs/processor/json.go#L23-L49
#[derive(Deserialize, Clone, Serialize, Debug)]
#[serde(deny_unknown_fields)]
struct LogMsg {
    pub message: Bytes,
    pub status: Bytes,
    pub timestamp: i64,
    pub hostname: Bytes,
    pub service: Bytes,
    pub ddsource: Bytes,
    pub ddtags: Bytes,
}

#[cfg(test)]
mod tests {
    use super::{DatadogAgentConfig, LogMsg};
    use crate::{
        config::{log_schema, SourceConfig, SourceContext},
        event::{Event, EventStatus},
        sources::datadog::agent::DatadogAgentSource,
        test_util::{next_addr, spawn_collect_n, trace_init, wait_for_tcp},
        Pipeline,
    };
    use bytes::Bytes;
    use futures::Stream;
    use http::HeaderMap;
    use pretty_assertions::assert_eq;
    use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};
    use std::net::SocketAddr;

    impl Arbitrary for LogMsg {
        fn arbitrary(g: &mut Gen) -> Self {
            LogMsg {
                message: Bytes::from(String::arbitrary(g)),
                status: Bytes::from(String::arbitrary(g)),
                timestamp: i64::arbitrary(g),
                hostname: Bytes::from(String::arbitrary(g)),
                service: Bytes::from(String::arbitrary(g)),
                ddsource: Bytes::from(String::arbitrary(g)),
                ddtags: Bytes::from(String::arbitrary(g)),
            }
        }
    }

    // We want to know that for any json payload that is a `Vec<LogMsg>` we can
    // correctly decode it into a `Vec<LogEvent>`. For convenience we assume
    // that order is preserved in the decoding step though this is not
    // necessarily part of the contract of that function.
    #[test]
    fn test_decode_body() {
        fn inner(msgs: Vec<LogMsg>) -> TestResult {
            let body = Bytes::from(serde_json::to_string(&msgs).unwrap());
            let api_key = None;

            let source = DatadogAgentSource::new(true);
            let events = source.decode_body(body, api_key).unwrap();
            assert_eq!(events.len(), msgs.len());
            for (msg, event) in msgs.into_iter().zip(events.into_iter()) {
                let log = event.as_log();
                assert_eq!(log["message"], msg.message.into());
                assert_eq!(log["status"], msg.status.into());
                assert_eq!(log["timestamp"], msg.timestamp.into());
                assert_eq!(log["hostname"], msg.hostname.into());
                assert_eq!(log["service"], msg.service.into());
                assert_eq!(log["ddsource"], msg.ddsource.into());
                assert_eq!(log["ddtags"], msg.ddtags.into());
            }

            TestResult::passed()
        }

        QuickCheck::new().quickcheck(inner as fn(Vec<LogMsg>) -> TestResult);
    }

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DatadogAgentConfig>();
    }

    async fn source(
        status: EventStatus,
        acknowledgements: bool,
        store_api_key: bool,
    ) -> (impl Stream<Item = Event>, SocketAddr) {
        let (sender, recv) = Pipeline::new_test_finalize(status);
        let address = next_addr();
        let mut context = SourceContext::new_test(sender);
        context.acknowledgements = acknowledgements;
        tokio::spawn(async move {
            DatadogAgentConfig {
                address,
                tls: None,
                store_api_key,
            }
            .build(context)
            .await
            .unwrap()
            .await
            .unwrap();
        });
        wait_for_tcp(address).await;
        (recv, address)
    }

    async fn send_with_path(
        address: SocketAddr,
        body: &str,
        headers: HeaderMap,
        path: &str,
    ) -> u16 {
        reqwest::Client::new()
            .post(&format!("http://{}{}", address, path))
            .headers(headers)
            .body(body.to_owned())
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    }

    #[tokio::test]
    async fn full_payload_v1() {
        trace_init();
        let (rx, addr) = source(EventStatus::Delivered, true, true).await;

        let mut events = spawn_collect_n(
            async move {
                assert_eq!(
                    200,
                    send_with_path(
                        addr,
                        &serde_json::to_string(&[LogMsg {
                            message: Bytes::from("foo"),
                            timestamp: 123,
                            hostname: Bytes::from("festeburg"),
                            status: Bytes::from("notice"),
                            service: Bytes::from("vector"),
                            ddsource: Bytes::from("curl"),
                            ddtags: Bytes::from("one,two,three"),
                        }])
                        .unwrap(),
                        HeaderMap::new(),
                        "/v1/input/"
                    )
                    .await
                );
            },
            rx,
            1,
        )
        .await;

        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["message"], "foo".into());
            assert_eq!(log["timestamp"], 123.into());
            assert_eq!(log["hostname"], "festeburg".into());
            assert_eq!(log["status"], "notice".into());
            assert_eq!(log["service"], "vector".into());
            assert_eq!(log["ddsource"], "curl".into());
            assert_eq!(log["ddtags"], "one,two,three".into());
            assert!(event.metadata().datadog_api_key().is_none());
            assert_eq!(log[log_schema().source_type_key()], "datadog_agent".into());
        }
    }

    #[tokio::test]
    async fn full_payload_v2() {
        trace_init();
        let (rx, addr) = source(EventStatus::Delivered, true, true).await;

        let mut events = spawn_collect_n(
            async move {
                assert_eq!(
                    200,
                    send_with_path(
                        addr,
                        &serde_json::to_string(&[LogMsg {
                            message: Bytes::from("foo"),
                            timestamp: 123,
                            hostname: Bytes::from("festeburg"),
                            status: Bytes::from("notice"),
                            service: Bytes::from("vector"),
                            ddsource: Bytes::from("curl"),
                            ddtags: Bytes::from("one,two,three"),
                        }])
                        .unwrap(),
                        HeaderMap::new(),
                        "/api/v2/logs"
                    )
                    .await
                );
            },
            rx,
            1,
        )
        .await;

        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["message"], "foo".into());
            assert_eq!(log["timestamp"], 123.into());
            assert_eq!(log["hostname"], "festeburg".into());
            assert_eq!(log["status"], "notice".into());
            assert_eq!(log["service"], "vector".into());
            assert_eq!(log["ddsource"], "curl".into());
            assert_eq!(log["ddtags"], "one,two,three".into());
            assert!(event.metadata().datadog_api_key().is_none());
            assert_eq!(log[log_schema().source_type_key()], "datadog_agent".into());
        }
    }

    #[tokio::test]
    async fn no_api_key() {
        trace_init();
        let (rx, addr) = source(EventStatus::Delivered, true, true).await;

        let mut events = spawn_collect_n(
            async move {
                assert_eq!(
                    200,
                    send_with_path(
                        addr,
                        &serde_json::to_string(&[LogMsg {
                            message: Bytes::from("foo"),
                            timestamp: 123,
                            hostname: Bytes::from("festeburg"),
                            status: Bytes::from("notice"),
                            service: Bytes::from("vector"),
                            ddsource: Bytes::from("curl"),
                            ddtags: Bytes::from("one,two,three"),
                        }])
                        .unwrap(),
                        HeaderMap::new(),
                        "/v1/input/"
                    )
                    .await
                );
            },
            rx,
            1,
        )
        .await;

        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["message"], "foo".into());
            assert_eq!(log["timestamp"], 123.into());
            assert_eq!(log["hostname"], "festeburg".into());
            assert_eq!(log["status"], "notice".into());
            assert_eq!(log["service"], "vector".into());
            assert_eq!(log["ddsource"], "curl".into());
            assert_eq!(log["ddtags"], "one,two,three".into());
            assert!(event.metadata().datadog_api_key().is_none());
            assert_eq!(log[log_schema().source_type_key()], "datadog_agent".into());
        }
    }

    #[tokio::test]
    async fn api_key_in_url() {
        trace_init();
        let (rx, addr) = source(EventStatus::Delivered, true, true).await;

        let mut events = spawn_collect_n(
            async move {
                assert_eq!(
                    200,
                    send_with_path(
                        addr,
                        &serde_json::to_string(&[LogMsg {
                            message: Bytes::from("bar"),
                            timestamp: 456,
                            hostname: Bytes::from("festeburg"),
                            status: Bytes::from("notice"),
                            service: Bytes::from("vector"),
                            ddsource: Bytes::from("curl"),
                            ddtags: Bytes::from("one,two,three"),
                        }])
                        .unwrap(),
                        HeaderMap::new(),
                        "/v1/input/12345678abcdefgh12345678abcdefgh"
                    )
                    .await
                );
            },
            rx,
            1,
        )
        .await;

        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["message"], "bar".into());
            assert_eq!(log["timestamp"], 456.into());
            assert_eq!(log["hostname"], "festeburg".into());
            assert_eq!(log["status"], "notice".into());
            assert_eq!(log["service"], "vector".into());
            assert_eq!(log["ddsource"], "curl".into());
            assert_eq!(log["ddtags"], "one,two,three".into());
            assert_eq!(log[log_schema().source_type_key()], "datadog_agent".into());
            assert_eq!(
                &event.metadata().datadog_api_key().as_ref().unwrap()[..],
                "12345678abcdefgh12345678abcdefgh"
            );
        }
    }

    #[tokio::test]
    async fn api_key_in_header() {
        trace_init();
        let (rx, addr) = source(EventStatus::Delivered, true, true).await;

        let mut headers = HeaderMap::new();
        headers.insert(
            "dd-api-key",
            "12345678abcdefgh12345678abcdefgh".parse().unwrap(),
        );

        let mut events = spawn_collect_n(
            async move {
                assert_eq!(
                    200,
                    send_with_path(
                        addr,
                        &serde_json::to_string(&[LogMsg {
                            message: Bytes::from("baz"),
                            timestamp: 789,
                            hostname: Bytes::from("festeburg"),
                            status: Bytes::from("notice"),
                            service: Bytes::from("vector"),
                            ddsource: Bytes::from("curl"),
                            ddtags: Bytes::from("one,two,three"),
                        }])
                        .unwrap(),
                        headers,
                        "/v1/input/"
                    )
                    .await
                );
            },
            rx,
            1,
        )
        .await;

        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["message"], "baz".into());
            assert_eq!(log["timestamp"], 789.into());
            assert_eq!(log["hostname"], "festeburg".into());
            assert_eq!(log["status"], "notice".into());
            assert_eq!(log["service"], "vector".into());
            assert_eq!(log["ddsource"], "curl".into());
            assert_eq!(log["ddtags"], "one,two,three".into());
            assert_eq!(log[log_schema().source_type_key()], "datadog_agent".into());
            assert_eq!(
                &event.metadata().datadog_api_key().as_ref().unwrap()[..],
                "12345678abcdefgh12345678abcdefgh"
            );
        }
    }

    #[tokio::test]
    async fn delivery_failure() {
        trace_init();
        let (rx, addr) = source(EventStatus::Failed, true, true).await;

        spawn_collect_n(
            async move {
                assert_eq!(
                    400,
                    send_with_path(
                        addr,
                        &serde_json::to_string(&[LogMsg {
                            message: Bytes::from("foo"),
                            timestamp: 123,
                            hostname: Bytes::from("festeburg"),
                            status: Bytes::from("notice"),
                            service: Bytes::from("vector"),
                            ddsource: Bytes::from("curl"),
                            ddtags: Bytes::from("one,two,three"),
                        }])
                        .unwrap(),
                        HeaderMap::new(),
                        "/v1/input/"
                    )
                    .await
                );
            },
            rx,
            1,
        )
        .await;
    }

    #[tokio::test]
    async fn ignores_disabled_acknowledgements() {
        trace_init();
        let (rx, addr) = source(EventStatus::Failed, false, true).await;

        let events = spawn_collect_n(
            async move {
                assert_eq!(
                    200,
                    send_with_path(
                        addr,
                        &serde_json::to_string(&[LogMsg {
                            message: Bytes::from("foo"),
                            timestamp: 123,
                            hostname: Bytes::from("festeburg"),
                            status: Bytes::from("notice"),
                            service: Bytes::from("vector"),
                            ddsource: Bytes::from("curl"),
                            ddtags: Bytes::from("one,two,three"),
                        }])
                        .unwrap(),
                        HeaderMap::new(),
                        "/v1/input/"
                    )
                    .await
                );
            },
            rx,
            1,
        )
        .await;

        assert_eq!(events.len(), 1);
    }

    #[tokio::test]
    async fn ignores_api_key() {
        trace_init();
        let (rx, addr) = source(EventStatus::Delivered, true, false).await;

        let mut headers = HeaderMap::new();
        headers.insert(
            "dd-api-key",
            "12345678abcdefgh12345678abcdefgh".parse().unwrap(),
        );

        let mut events = spawn_collect_n(
            async move {
                assert_eq!(
                    200,
                    send_with_path(
                        addr,
                        &serde_json::to_string(&[LogMsg {
                            message: Bytes::from("baz"),
                            timestamp: 789,
                            hostname: Bytes::from("festeburg"),
                            status: Bytes::from("notice"),
                            service: Bytes::from("vector"),
                            ddsource: Bytes::from("curl"),
                            ddtags: Bytes::from("one,two,three"),
                        }])
                        .unwrap(),
                        headers,
                        "/v1/input/12345678abcdefgh12345678abcdefgh"
                    )
                    .await
                );
            },
            rx,
            1,
        )
        .await;

        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["message"], "baz".into());
            assert_eq!(log["timestamp"], 789.into());
            assert_eq!(log["hostname"], "festeburg".into());
            assert_eq!(log["status"], "notice".into());
            assert_eq!(log["service"], "vector".into());
            assert_eq!(log["ddsource"], "curl".into());
            assert_eq!(log["ddtags"], "one,two,three".into());
            assert_eq!(log[log_schema().source_type_key()], "datadog_agent".into());
            assert!(event.metadata().datadog_api_key().is_none());
        }
    }
}

use super::sketch_parser::decode_ddsketch;
use crate::{
    codecs::{
        self,
        decoding::{DecodingConfig, DeserializerConfig, FramingConfig},
    },
    common::datadog::{DatadogMetricType, DatadogSeriesMetric},
    config::{
        log_schema, AcknowledgementsConfig, DataType, GenerateConfig, Resource, SourceConfig,
        SourceContext, SourceDescription,
    },
    event::{
        metric::{Metric, MetricKind, MetricValue},
        Event,
    },
    internal_events::{EventsReceived, HttpBytesReceived, HttpDecompressError},
    serde::{bool_or_struct, default_decoding, default_framing_message_based},
    sources::{
        self,
        util::{ErrorMessage, StreamDecodingError},
    },
    tls::{MaybeTlsSettings, TlsConfig},
    Pipeline,
};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use chrono::{TimeZone, Utc};
use flate2::read::{MultiGzDecoder, ZlibDecoder};
use futures::{future, FutureExt, SinkExt, StreamExt, TryFutureExt};
use http::StatusCode;
use regex::Regex;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::collections::BTreeMap;
use std::{io::Read, net::SocketAddr, sync::Arc};
use tokio_util::codec::Decoder;
use vector_core::event::{BatchNotifier, BatchStatus};
use vector_core::ByteSizeOf;
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
    #[serde(default = "default_framing_message_based")]
    framing: Box<dyn FramingConfig>,
    #[serde(default = "default_decoding")]
    decoding: Box<dyn DeserializerConfig>,
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: AcknowledgementsConfig,
}

inventory::submit! {
    SourceDescription::new::<DatadogAgentConfig>("datadog_agent")
}

#[derive(Deserialize)]
pub struct ApiKeyQueryParams {
    #[serde(rename = "dd-api-key")]
    dd_api_key: Option<String>,
}

impl GenerateConfig for DatadogAgentConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            address: "0.0.0.0:8080".parse().unwrap(),
            tls: None,
            store_api_key: true,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            acknowledgements: AcknowledgementsConfig::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "datadog_agent")]
impl SourceConfig for DatadogAgentConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<sources::Source> {
        let decoder = DecodingConfig::new(self.framing.clone(), self.decoding.clone()).build()?;
        let tls = MaybeTlsSettings::from_config(&self.tls, true)?;
        let source = DatadogAgentSource::new(self.store_api_key, decoder, tls.http_protocol_name());
        let listener = tls.bind(&self.address).await?;
        let log_service = source
            .clone()
            .event_service(self.acknowledgements.enabled, cx.out.clone());
        let series_v1_service = source
            .clone()
            .series_v1_service(self.acknowledgements.enabled, cx.out.clone());
        let sketches_service = source
            .clone()
            .sketches_service(self.acknowledgements.enabled, cx.out.clone());
        let series_v2_service = source.series_v2_service();

        let shutdown = cx.shutdown;
        Ok(Box::pin(async move {
            let span = crate::trace::current_span();
            let routes = log_service
                .or(series_v1_service)
                .unify()
                .or(series_v2_service)
                .unify()
                .or(sketches_service)
                .unify()
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
        DataType::Any
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
    decoder: codecs::Decoder,
    protocol: &'static str,
}

#[derive(Deserialize, Serialize)]
struct DatadogSeriesRequest {
    series: Vec<DatadogSeriesMetric>,
}

impl DatadogAgentSource {
    fn new(store_api_key: bool, decoder: codecs::Decoder, protocol: &'static str) -> Self {
        Self {
            store_api_key,
            api_key_matcher: Regex::new(r"^/v1/input/(?P<api_key>[[:alnum:]]{32})/??")
                .expect("static regex always compiles"),
            log_schema_source_type_key: log_schema().source_type_key(),
            log_schema_timestamp_key: log_schema().timestamp_key(),
            decoder,
            protocol,
        }
    }

    fn extract_api_key(
        &self,
        path: &str,
        header: Option<String>,
        query_params: Option<String>,
    ) -> Option<Arc<str>> {
        if !self.store_api_key {
            return None;
        }
        // Grab from URL first
        self.api_key_matcher
            .captures(path)
            .and_then(|cap| cap.name("api_key").map(|key| key.as_str()).map(Arc::from))
            // Try from query params
            .or_else(|| query_params.map(Arc::from))
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
                let receiver = BatchNotifier::maybe_apply_to_events(acknowledgements, &mut events);

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
                        BatchStatus::Rejected => Err(warp::reject::custom(ErrorMessage::new(
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
            .and(warp::query::<ApiKeyQueryParams>())
            .and(warp::body::bytes())
            .and_then(
                move |_,
                      path: FullPath,
                      encoding_header: Option<String>,
                      api_token: Option<String>,
                      query_params: ApiKeyQueryParams,
                      body: Bytes| {
                    emit!(&HttpBytesReceived {
                        byte_size: body.len(),
                        http_path: path.as_str(),
                        protocol: self.protocol,
                    });
                    let events = decode(&encoding_header, body).and_then(|body| {
                        self.decode_log_body(
                            body,
                            self.extract_api_key(path.as_str(), api_token, query_params.dd_api_key),
                        )
                    });
                    Self::handle_request(events, acknowledgements, out.clone())
                },
            )
            .boxed()
    }

    fn series_v1_service(self, acknowledgements: bool, out: Pipeline) -> BoxedFilter<(Response,)> {
        warp::post()
            .and(path!("api" / "v1" / "series" / ..))
            .and(warp::path::full())
            .and(warp::header::optional::<String>("content-encoding"))
            .and(warp::header::optional::<String>("dd-api-key"))
            .and(warp::query::<ApiKeyQueryParams>())
            .and(warp::body::bytes())
            .and_then(
                move |path: FullPath,
                      encoding_header: Option<String>,
                      api_token: Option<String>,
                      query_params: ApiKeyQueryParams,
                      body: Bytes| {
                    emit!(&HttpBytesReceived {
                        byte_size: body.len(),
                        http_path: path.as_str(),
                        protocol: self.protocol,
                    });
                    let events = decode(&encoding_header, body).and_then(|body| {
                        self.decode_datadog_series(
                            body,
                            self.extract_api_key(path.as_str(), api_token, query_params.dd_api_key),
                        )
                    });
                    Self::handle_request(events, acknowledgements, out.clone())
                },
            )
            .boxed()
    }

    fn series_v2_service(self) -> BoxedFilter<(Response,)> {
        warp::post()
            // This should not happen anytime soon as the v2 series endpoint does not exist yet
            // but the route exists in the agent codebase
            .and(path!("api" / "v2" / "series" / ..))
            .and_then(|| {
                error!(message = "/api/v2/series route is not supported.");
                let response: Result<Response, Rejection> =
                    Err(warp::reject::custom(ErrorMessage::new(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "Vector does not support the /api/v2/series route".to_string(),
                    )));
                future::ready(response)
            })
            .boxed()
    }

    fn sketches_service(self, acknowledgements: bool, out: Pipeline) -> BoxedFilter<(Response,)> {
        warp::post()
            .and(path!("api" / "beta" / "sketches" / ..))
            .and(warp::path::full())
            .and(warp::header::optional::<String>("content-encoding"))
            .and(warp::header::optional::<String>("dd-api-key"))
            .and(warp::query::<ApiKeyQueryParams>())
            .and(warp::body::bytes())
            .and_then(
                move |path: FullPath,
                      encoding_header: Option<String>,
                      api_token: Option<String>,
                      query_params: ApiKeyQueryParams,
                      body: Bytes| {
                    emit!(&HttpBytesReceived {
                        byte_size: body.len(),
                        http_path: path.as_str(),
                        protocol: self.protocol,
                    });
                    let events = decode(&encoding_header, body).and_then(|body| {
                        self.decode_datadog_sketches(
                            body,
                            self.extract_api_key(path.as_str(), api_token, query_params.dd_api_key),
                        )
                    });
                    Self::handle_request(events, acknowledgements, out.clone())
                },
            )
            .boxed()
    }

    fn decode_datadog_sketches(
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

        let metrics = decode_ddsketch(body, &api_key).map_err(|error| {
            ErrorMessage::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("Error decoding Datadog sketch: {:?}", error),
            )
        })?;

        emit!(&EventsReceived {
            byte_size: metrics.size_of(),
            count: metrics.len(),
        });

        Ok(metrics)
    }

    fn decode_datadog_series(
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

        let metrics: DatadogSeriesRequest = serde_json::from_slice(&body).map_err(|error| {
            ErrorMessage::new(
                StatusCode::BAD_REQUEST,
                format!("Error parsing JSON: {:?}", error),
            )
        })?;

        let decoded_metrics: Vec<Event> = metrics
            .series
            .into_iter()
            .flat_map(|m| into_vector_metric(m, api_key.clone()))
            .collect();

        emit!(&EventsReceived {
            byte_size: decoded_metrics.size_of(),
            count: decoded_metrics.len(),
        });

        Ok(decoded_metrics)
    }

    fn decode_log_body(
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
        let mut decoded = Vec::new();

        for message in messages {
            let mut decoder = self.decoder.clone();
            let mut buffer = BytesMut::new();
            buffer.put(message.message);
            loop {
                match decoder.decode_eof(&mut buffer) {
                    Ok(Some((events, _byte_size))) => {
                        for mut event in events {
                            if let Event::Log(ref mut log) = event {
                                log.try_insert_flat("status", message.status.clone());
                                log.try_insert_flat("timestamp", message.timestamp);
                                log.try_insert_flat("hostname", message.hostname.clone());
                                log.try_insert_flat("service", message.service.clone());
                                log.try_insert_flat("ddsource", message.ddsource.clone());
                                log.try_insert_flat("ddtags", message.ddtags.clone());
                                log.try_insert_flat(
                                    self.log_schema_source_type_key,
                                    Bytes::from("datadog_agent"),
                                );
                                log.try_insert_flat(self.log_schema_timestamp_key, now);
                                if let Some(k) = &api_key {
                                    log.metadata_mut().set_datadog_api_key(Some(Arc::clone(k)));
                                }
                            }

                            decoded.push(event);
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        // Error is logged by `crate::codecs::Decoder`, no further
                        // handling is needed here.
                        if !error.can_continue() {
                            break;
                        }
                    }
                }
            }
        }
        emit!(&EventsReceived {
            byte_size: decoded.size_of(),
            count: decoded.len(),
        });

        Ok(decoded)
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
                    ZlibDecoder::new(body.reader())
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

fn into_vector_metric(dd_metric: DatadogSeriesMetric, api_key: Option<Arc<str>>) -> Vec<Event> {
    let mut tags: BTreeMap<String, String> = dd_metric
        .tags
        .unwrap_or_default()
        .iter()
        .map(|tag| {
            let kv = tag.split_once(":").unwrap_or((tag, ""));
            (kv.0.trim().into(), kv.1.trim().into())
        })
        .collect();

    dd_metric
        .host
        .and_then(|host| tags.insert(log_schema().host_key().to_owned(), host));
    dd_metric
        .source_type_name
        .and_then(|source| tags.insert("source_type_name".into(), source));
    dd_metric
        .device
        .and_then(|dev| tags.insert("device".into(), dev));

    match dd_metric.r#type {
        DatadogMetricType::Count => dd_metric
            .points
            .iter()
            .map(|dd_point| {
                Metric::new(
                    dd_metric.metric.clone(),
                    MetricKind::Incremental,
                    MetricValue::Counter { value: dd_point.1 },
                )
                .with_timestamp(Some(Utc.timestamp(dd_point.0, 0)))
                .with_tags(Some(tags.clone()))
            })
            .collect::<Vec<_>>(),
        DatadogMetricType::Gauge => dd_metric
            .points
            .iter()
            .map(|dd_point| {
                Metric::new(
                    dd_metric.metric.clone(),
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: dd_point.1 },
                )
                .with_timestamp(Some(Utc.timestamp(dd_point.0, 0)))
                .with_tags(Some(tags.clone()))
            })
            .collect::<Vec<_>>(),
        // Agent sends rate only for dogstatsd counter https://github.com/DataDog/datadog-agent/blob/f4a13c6dca5e2da4bb722f861a8ac4c2f715531d/pkg/metrics/counter.go#L8-L10
        // for consistency purpose (w.r.t. (dog)statsd source) they are turned back into counters
        DatadogMetricType::Rate => dd_metric
            .points
            .iter()
            .map(|dd_point| {
                let i = dd_metric.interval.filter(|v| *v != 0).unwrap_or(1) as f64;
                Metric::new(
                    dd_metric.metric.clone(),
                    MetricKind::Incremental,
                    MetricValue::Counter {
                        value: dd_point.1 * i,
                    },
                )
                .with_timestamp(Some(Utc.timestamp(dd_point.0, 0)))
                .with_tags(Some(tags.clone()))
            })
            .collect::<Vec<_>>(),
    }
    .into_iter()
    .map(|mut metric| {
        if let Some(k) = &api_key {
            metric
                .metadata_mut()
                .set_datadog_api_key(Some(Arc::clone(k)));
        }
        metric.into()
    })
    .collect()
}

fn handle_decode_error(encoding: &str, error: impl std::error::Error) -> ErrorMessage {
    emit!(&HttpDecompressError {
        encoding,
        error: &error
    });
    ErrorMessage::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        format!("Failed decompressing payload with {} decoder.", encoding),
    )
}

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
    use super::{DatadogAgentConfig, DatadogAgentSource, DatadogSeriesRequest, LogMsg};
    use crate::{
        codecs::{self, BytesDecoder, BytesDeserializer},
        common::datadog::{DatadogMetricType, DatadogPoint, DatadogSeriesMetric},
        config::{log_schema, SourceConfig, SourceContext},
        event::{
            metric::{MetricKind, MetricSketch, MetricValue},
            Event, EventStatus,
        },
        serde::{default_decoding, default_framing_message_based},
        test_util::{next_addr, spawn_collect_n, trace_init, wait_for_tcp},
        Pipeline,
    };
    use bytes::Bytes;
    use chrono::{TimeZone, Utc};
    use futures::Stream;
    use http::HeaderMap;
    use pretty_assertions::assert_eq;
    use prost::Message;
    use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};
    use std::net::SocketAddr;
    use std::str;

    mod dd_proto {
        include!(concat!(env!("OUT_DIR"), "/datadog.agentpayload.rs"));
    }

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
    fn test_decode_log_body() {
        fn inner(msgs: Vec<LogMsg>) -> TestResult {
            let body = Bytes::from(serde_json::to_string(&msgs).unwrap());
            let api_key = None;

            let decoder = codecs::Decoder::new(
                Box::new(BytesDecoder::new()),
                Box::new(BytesDeserializer::new()),
            );
            let source = DatadogAgentSource::new(true, decoder, "http");
            let events = source.decode_log_body(body, api_key).unwrap();
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
        let context = SourceContext::new_test(sender);
        tokio::spawn(async move {
            DatadogAgentConfig {
                address,
                tls: None,
                store_api_key,
                framing: default_framing_message_based(),
                decoding: default_decoding(),
                acknowledgements: acknowledgements.into(),
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
    async fn api_key_in_query_params() {
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
                        "/api/v2/logs?dd-api-key=12345678abcdefgh12345678abcdefgh"
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
        let (rx, addr) = source(EventStatus::Rejected, true, true).await;

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
        let (rx, addr) = source(EventStatus::Rejected, false, true).await;

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

    #[tokio::test]
    async fn decode_series_endpoints() {
        trace_init();
        let (rx, addr) = source(EventStatus::Delivered, true, true).await;

        let mut headers = HeaderMap::new();
        headers.insert(
            "dd-api-key",
            "12345678abcdefgh12345678abcdefgh".parse().unwrap(),
        );

        let dd_metric_request = DatadogSeriesRequest {
            series: vec![
                DatadogSeriesMetric {
                    metric: "dd_gauge".to_string(),
                    r#type: DatadogMetricType::Gauge,
                    interval: None,
                    points: vec![
                        DatadogPoint(1542182950, 3.14),
                        DatadogPoint(1542182951, 3.1415),
                    ],
                    tags: Some(vec!["foo:bar".to_string()]),
                    host: Some("random_host".to_string()),
                    source_type_name: None,
                    device: None,
                },
                DatadogSeriesMetric {
                    metric: "dd_rate".to_string(),
                    r#type: DatadogMetricType::Rate,
                    interval: Some(10),
                    points: vec![DatadogPoint(1542182950, 3.14)],
                    tags: Some(vec!["foo:bar:baz".to_string()]),
                    host: Some("another_random_host".to_string()),
                    source_type_name: None,
                    device: None,
                },
                DatadogSeriesMetric {
                    metric: "dd_count".to_string(),
                    r#type: DatadogMetricType::Count,
                    interval: None,
                    points: vec![DatadogPoint(1542182955, 16777216_f64)],
                    tags: Some(vec!["foobar".to_string()]),
                    host: Some("a_host".to_string()),
                    source_type_name: None,
                    device: None,
                },
            ],
        };
        let events = spawn_collect_n(
            async move {
                assert_eq!(
                    200,
                    send_with_path(
                        addr,
                        &serde_json::to_string(&dd_metric_request).unwrap(),
                        headers,
                        "/api/v1/series"
                    )
                    .await
                );
            },
            rx,
            4,
        )
        .await;

        {
            let mut metric = events[0].as_metric();
            assert_eq!(metric.name(), "dd_gauge");
            assert_eq!(
                metric.timestamp(),
                Some(Utc.ymd(2018, 11, 14).and_hms(8, 9, 10))
            );
            assert_eq!(metric.kind(), MetricKind::Absolute);
            assert_eq!(*metric.value(), MetricValue::Gauge { value: 3.14 });
            assert_eq!(metric.tags().unwrap()["host"], "random_host".to_string());
            assert_eq!(metric.tags().unwrap()["foo"], "bar".to_string());

            assert_eq!(
                &events[0].metadata().datadog_api_key().as_ref().unwrap()[..],
                "12345678abcdefgh12345678abcdefgh"
            );

            metric = events[1].as_metric();
            assert_eq!(metric.name(), "dd_gauge");
            assert_eq!(
                metric.timestamp(),
                Some(Utc.ymd(2018, 11, 14).and_hms(8, 9, 11))
            );
            assert_eq!(metric.kind(), MetricKind::Absolute);
            assert_eq!(*metric.value(), MetricValue::Gauge { value: 3.1415 });
            assert_eq!(metric.tags().unwrap()["host"], "random_host".to_string());
            assert_eq!(metric.tags().unwrap()["foo"], "bar".to_string());

            assert_eq!(
                &events[1].metadata().datadog_api_key().as_ref().unwrap()[..],
                "12345678abcdefgh12345678abcdefgh"
            );

            metric = events[2].as_metric();
            assert_eq!(metric.name(), "dd_rate");
            assert_eq!(
                metric.timestamp(),
                Some(Utc.ymd(2018, 11, 14).and_hms(8, 9, 10))
            );
            assert_eq!(metric.kind(), MetricKind::Incremental);
            assert_eq!(
                *metric.value(),
                MetricValue::Counter {
                    value: 3.14 * (10_f64)
                }
            );
            assert_eq!(
                metric.tags().unwrap()["host"],
                "another_random_host".to_string()
            );
            assert_eq!(metric.tags().unwrap()["foo"], "bar:baz".to_string());

            assert_eq!(
                &events[2].metadata().datadog_api_key().as_ref().unwrap()[..],
                "12345678abcdefgh12345678abcdefgh"
            );

            metric = events[3].as_metric();
            assert_eq!(metric.name(), "dd_count");
            assert_eq!(
                metric.timestamp(),
                Some(Utc.ymd(2018, 11, 14).and_hms(8, 9, 15))
            );
            assert_eq!(metric.kind(), MetricKind::Incremental);
            assert_eq!(
                *metric.value(),
                MetricValue::Counter {
                    value: 16777216_f64
                }
            );
            assert_eq!(metric.tags().unwrap()["host"], "a_host".to_string());
            assert_eq!(metric.tags().unwrap()["foobar"], "".to_string());

            assert_eq!(
                &events[3].metadata().datadog_api_key().as_ref().unwrap()[..],
                "12345678abcdefgh12345678abcdefgh"
            );
        }
    }

    #[tokio::test]
    async fn decode_sketches() {
        trace_init();
        let (rx, addr) = source(EventStatus::Delivered, true, true).await;

        let mut headers = HeaderMap::new();
        headers.insert(
            "dd-api-key",
            "12345678abcdefgh12345678abcdefgh".parse().unwrap(),
        );

        let mut buf = Vec::new();
        let sketch = dd_proto::sketch_payload::Sketch {
            metric: "dd_sketch".to_string(),
            tags: vec!["foo:bar".to_string(), "foobar".to_string()],
            host: "a_host".to_string(),
            distributions: Vec::new(),
            dogsketches: vec![dd_proto::sketch_payload::sketch::Dogsketch {
                ts: 1542182950,
                cnt: 2,
                min: 16.0,
                max: 31.0,
                avg: 23.5,
                sum: 74.0,
                k: vec![1517, 1559],
                n: vec![1, 1],
            }],
        };

        let sketch_payload = dd_proto::SketchPayload {
            metadata: None,
            sketches: vec![sketch],
        };

        sketch_payload.encode(&mut buf).unwrap();

        let events = spawn_collect_n(
            async move {
                assert_eq!(
                    200,
                    send_with_path(
                        addr,
                        unsafe { str::from_utf8_unchecked(&buf) },
                        headers,
                        "/api/beta/sketches"
                    )
                    .await
                );
            },
            rx,
            1,
        )
        .await;

        {
            let metric = events[0].as_metric();
            assert_eq!(metric.name(), "dd_sketch");
            assert_eq!(
                metric.timestamp(),
                Some(Utc.ymd(2018, 11, 14).and_hms(8, 9, 10))
            );
            assert_eq!(metric.kind(), MetricKind::Incremental);
            assert_eq!(metric.tags().unwrap()["host"], "a_host".to_string());
            assert_eq!(metric.tags().unwrap()["foo"], "bar".to_string());
            assert_eq!(metric.tags().unwrap()["foobar"], "".to_string());

            let s = &*metric.value();
            assert!(matches!(s, MetricValue::Sketch { .. }));
            if let MetricValue::Sketch {
                sketch: MetricSketch::AgentDDSketch(ddsketch),
            } = s
            {
                assert_eq!(ddsketch.bins().len(), 2);
                assert_eq!(ddsketch.count(), 2);
                assert_eq!(ddsketch.min(), Some(16.0));
                assert_eq!(ddsketch.max(), Some(31.0));
                assert_eq!(ddsketch.sum(), Some(74.0));
                assert_eq!(ddsketch.avg(), Some(23.5));
            }

            assert_eq!(
                &events[0].metadata().datadog_api_key().as_ref().unwrap()[..],
                "12345678abcdefgh12345678abcdefgh"
            );
        }
    }
}

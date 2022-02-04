#[cfg(all(test, feature = "datadog-agent-integration-tests"))]
mod integration_tests;
#[cfg(test)]
mod tests;

use std::{collections::BTreeMap, io::Read, net::SocketAddr, sync::Arc};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use chrono::{TimeZone, Utc};
use flate2::read::{MultiGzDecoder, ZlibDecoder};
use futures::{future, FutureExt};
use http::StatusCode;
use regex::Regex;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use tokio_util::codec::Decoder;
use vector_core::{
    event::{BatchNotifier, BatchStatus},
    internal_event::EventsReceived,
    ByteSizeOf,
};
use warp::{
    filters::BoxedFilter, path, path::FullPath, reject::Rejection, reply::Response, Filter, Reply,
};

use super::sketch_parser::decode_ddsketch;
use crate::{
    codecs::{
        self,
        decoding::{DecodingConfig, DeserializerConfig, FramingConfig},
    },
    common::datadog::{DatadogMetricType, DatadogSeriesMetric},
    config::{
        log_schema, AcknowledgementsConfig, DataType, GenerateConfig, Output, Resource,
        SourceConfig, SourceContext, SourceDescription,
    },
    event::{
        metric::{Metric, MetricKind, MetricValue},
        Event,
    },
    internal_events::{HttpBytesReceived, HttpDecompressError, StreamClosedError},
    serde::{bool_or_struct, default_decoding, default_framing_message_based},
    sources::{
        self,
        util::{ErrorMessage, StreamDecodingError},
    },
    tls::{MaybeTlsSettings, TlsConfig},
    SourceSender,
};

const LOGS: &str = "logs";
const METRICS: &str = "metrics";

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
    #[serde(default = "crate::serde::default_false")]
    multiple_outputs: bool,
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
            acknowledgements: Default::default(),
            multiple_outputs: false,
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
        let acknowledgements = cx.globals.acknowledgements.merge(&self.acknowledgements);
        let log_service = source.clone().event_service(
            acknowledgements.enabled(),
            cx.out.clone(),
            self.multiple_outputs,
        );
        let series_v1_service = source.clone().series_v1_service(
            acknowledgements.enabled(),
            cx.out.clone(),
            self.multiple_outputs,
        );
        let sketches_service = source.clone().sketches_service(
            acknowledgements.enabled(),
            cx.out.clone(),
            self.multiple_outputs,
        );
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

    fn outputs(&self) -> Vec<Output> {
        if self.multiple_outputs {
            vec![
                Output::from((METRICS, DataType::Metric)),
                Output::from((LOGS, DataType::Log)),
            ]
        } else {
            vec![Output::default(DataType::Any)]
        }
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
        mut out: SourceSender,
        output: Option<&str>,
    ) -> Result<Response, Rejection> {
        match events {
            Ok(mut events) => {
                let receiver = BatchNotifier::maybe_apply_to_events(acknowledgements, &mut events);
                let count = events.len();

                let mut events = futures::stream::iter(events);
                if let Some(name) = output {
                    out.send_all_named(name, &mut events).await
                } else {
                    out.send_all(&mut events).await
                }
                .map_err(move |error: crate::source_sender::ClosedError| {
                    emit!(&StreamClosedError { error, count });
                    warp::reject::custom(ApiError::ServerShutdown)
                })?;
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

    fn event_service(
        self,
        acknowledgements: bool,
        out: SourceSender,
        multiple_outputs: bool,
    ) -> BoxedFilter<(Response,)> {
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
                    if multiple_outputs {
                        Self::handle_request(events, acknowledgements, out.clone(), Some(LOGS))
                    } else {
                        Self::handle_request(events, acknowledgements, out.clone(), None)
                    }
                },
            )
            .boxed()
    }

    fn series_v1_service(
        self,
        acknowledgements: bool,
        out: SourceSender,
        multiple_outputs: bool,
    ) -> BoxedFilter<(Response,)> {
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
                    if multiple_outputs {
                        Self::handle_request(events, acknowledgements, out.clone(), Some(METRICS))
                    } else {
                        Self::handle_request(events, acknowledgements, out.clone(), None)
                    }
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

    fn sketches_service(
        self,
        acknowledgements: bool,
        out: SourceSender,
        multiple_outputs: bool,
    ) -> BoxedFilter<(Response,)> {
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
                    if multiple_outputs {
                        Self::handle_request(events, acknowledgements, out.clone(), Some(METRICS))
                    } else {
                        Self::handle_request(events, acknowledgements, out.clone(), None)
                    }
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

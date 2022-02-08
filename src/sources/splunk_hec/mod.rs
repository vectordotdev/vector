use std::{
    collections::HashMap,
    io::Read,
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
};

use bytes::{Buf, Bytes};
use chrono::{DateTime, TimeZone, Utc};
use flate2::read::MultiGzDecoder;
use futures::{stream, FutureExt};
use http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{de::Read as JsonRead, Deserializer, Value as JsonValue};
use snafu::Snafu;
use vector_core::{event::BatchNotifier, ByteSizeOf};
use warp::{filters::BoxedFilter, path, reject::Rejection, reply::Response, Filter, Reply};

use self::{
    acknowledgements::{
        HecAckStatusRequest, HecAckStatusResponse, HecAcknowledgementsConfig,
        IndexerAcknowledgement,
    },
    splunk_response::{HecResponse, HecResponseMetadata, HecStatusCode},
};
use crate::{
    config::{
        log_schema, DataType, Output, Resource, SourceConfig, SourceContext, SourceDescription,
    },
    event::{Event, LogEvent, Value},
    internal_events::{
        EventsReceived, HttpBytesReceived, SplunkHecRequestBodyInvalidError, SplunkHecRequestError,
        SplunkHecRequestReceived,
    },
    serde::bool_or_struct,
    source_sender::StreamSendError,
    tls::{MaybeTlsSettings, TlsConfig},
    SourceSender,
};

mod acknowledgements;

// Event fields unique to splunk_hec source
pub const CHANNEL: &str = "splunk_channel";
pub const INDEX: &str = "splunk_index";
pub const SOURCE: &str = "splunk_source";
pub const SOURCETYPE: &str = "splunk_sourcetype";

/// Accepts HTTP requests.
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct SplunkConfig {
    /// Local address on which to listen
    #[serde(default = "default_socket_address")]
    address: SocketAddr,
    /// Splunk HEC token. Deprecated - use `valid_tokens` instead
    token: Option<String>,
    /// A list of tokens to accept. Omit this to accept any token
    valid_tokens: Option<Vec<String>>,
    tls: Option<TlsConfig>,
    /// Splunk HEC indexer acknowledgement settings
    #[serde(deserialize_with = "bool_or_struct")]
    acknowledgements: HecAcknowledgementsConfig,
    /// Splunk HEC token passthrough
    store_hec_token: bool,
}

inventory::submit! {
    SourceDescription::new::<SplunkConfig>("splunk_hec")
}

impl_generate_config_from_default!(SplunkConfig);

impl SplunkConfig {
    #[cfg(test)]
    pub fn on(address: SocketAddr) -> Self {
        SplunkConfig {
            address,
            ..Self::default()
        }
    }
}

impl Default for SplunkConfig {
    fn default() -> Self {
        SplunkConfig {
            address: default_socket_address(),
            token: None,
            valid_tokens: None,
            tls: None,
            acknowledgements: Default::default(),
            store_hec_token: false,
        }
    }
}

fn default_socket_address() -> SocketAddr {
    SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 8088)
}

#[async_trait::async_trait]
#[typetag::serde(name = "splunk_hec")]
impl SourceConfig for SplunkConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let tls = MaybeTlsSettings::from_config(&self.tls, true)?;
        let shutdown = cx.shutdown.clone();
        let out = cx.out.clone();
        let source = SplunkSource::new(self, tls.http_protocol_name(), cx);

        let event_service = source.event_service(out.clone());
        let raw_service = source.raw_service(out);
        let health_service = source.health_service();
        let ack_service = source.ack_service();
        let options = SplunkSource::options();

        let services = path!("services" / "collector" / ..)
            .and(
                warp::path::full()
                    .map(|path: warp::filters::path::FullPath| {
                        emit!(&SplunkHecRequestReceived {
                            path: path.as_str()
                        });
                    })
                    .untuple_one(),
            )
            .and(
                event_service
                    .or(raw_service)
                    .unify()
                    .or(health_service)
                    .unify()
                    .or(ack_service)
                    .unify()
                    .or(options)
                    .unify(),
            )
            .or_else(finish_err);

        let listener = tls.bind(&self.address).await?;

        Ok(Box::pin(async move {
            let span = crate::trace::current_span();
            warp::serve(services.with(warp::trace(move |_info| span.clone())))
                .serve_incoming_with_graceful_shutdown(
                    listener.accept_stream(),
                    shutdown.map(|_| ()),
                )
                .await;

            Ok(())
        }))
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(DataType::Log)]
    }

    fn source_type(&self) -> &'static str {
        "splunk_hec"
    }

    fn resources(&self) -> Vec<Resource> {
        vec![Resource::tcp(self.address)]
    }
}

/// Shared data for responding to requests.
struct SplunkSource {
    valid_credentials: Vec<String>,
    protocol: &'static str,
    idx_ack: Option<Arc<IndexerAcknowledgement>>,
    store_hec_token: bool,
}

impl SplunkSource {
    fn new(config: &SplunkConfig, protocol: &'static str, cx: SourceContext) -> Self {
        let acknowledgements = cx
            .globals
            .acknowledgements
            .merge(&config.acknowledgements.inner);
        let shutdown = cx.shutdown.shared();
        let valid_tokens = config
            .valid_tokens
            .iter()
            .flatten()
            .chain(config.token.iter());

        let idx_ack = acknowledgements.enabled().then(|| {
            Arc::new(IndexerAcknowledgement::new(
                config.acknowledgements.clone(),
                shutdown,
            ))
        });

        SplunkSource {
            valid_credentials: valid_tokens
                .map(|token| format!("Splunk {}", token))
                .collect(),
            protocol,
            idx_ack,
            store_hec_token: config.store_hec_token,
        }
    }

    fn event_service(&self, out: SourceSender) -> BoxedFilter<(Response,)> {
        let splunk_channel_query_param = warp::query::<HashMap<String, String>>()
            .map(|qs: HashMap<String, String>| qs.get("channel").map(|v| v.to_owned()));
        let splunk_channel_header = warp::header::optional::<String>("x-splunk-request-channel");

        let splunk_channel = splunk_channel_header
            .and(splunk_channel_query_param)
            .map(|header: Option<String>, query_param| header.or(query_param));

        let protocol = self.protocol;
        let idx_ack = self.idx_ack.clone();
        let store_hec_token = self.store_hec_token;

        warp::post()
            .and(path!("event").or(path!("event" / "1.0")))
            .and(self.authorization())
            .and(splunk_channel)
            .and(warp::addr::remote())
            .and(warp::header::optional::<String>("X-Forwarded-For"))
            .and(self.gzip())
            .and(warp::body::bytes())
            .and(warp::path::full())
            .and_then(
                move |_,
                      token: Option<String>,
                      channel: Option<String>,
                      remote: Option<SocketAddr>,
                      xff: Option<String>,
                      gzip: bool,
                      body: Bytes,
                      path: warp::path::FullPath| {
                    let mut out = out.clone();
                    let idx_ack = idx_ack.clone();
                    emit!(&HttpBytesReceived {
                        byte_size: body.len(),
                        http_path: path.as_str(),
                        protocol,
                    });

                    async move {
                        if idx_ack.is_some() && channel.is_none() {
                            return Err(Rejection::from(ApiError::MissingChannel));
                        }

                        let mut data = Vec::new();
                        let body = if gzip {
                            MultiGzDecoder::new(body.reader())
                                .read_to_end(&mut data)
                                .map_err(|_| Rejection::from(ApiError::BadRequest))?;
                            String::from_utf8_lossy(data.as_slice())
                        } else {
                            String::from_utf8_lossy(body.as_ref())
                        };

                        let (batch, receiver) =
                            BatchNotifier::maybe_new_with_receiver(idx_ack.is_some());
                        let maybe_ack_id = match (idx_ack, receiver, channel.clone()) {
                            (Some(idx_ack), Some(receiver), Some(channel_id)) => {
                                match idx_ack.get_ack_id_from_channel(channel_id, receiver).await {
                                    Ok(ack_id) => Some(ack_id),
                                    Err(rej) => return Err(rej),
                                }
                            }
                            _ => None,
                        };
                        let mut events = stream::iter(EventIterator::new(
                            Deserializer::from_str(&body).into_iter::<JsonValue>(),
                            channel,
                            remote,
                            xff,
                            batch,
                            token.filter(|_| store_hec_token).map(Into::into),
                        ));

                        match out.send_result_stream(&mut events).await {
                            Ok(()) => Ok(maybe_ack_id),
                            Err(StreamSendError::Stream(error)) => Err(error),
                            Err(StreamSendError::Closed(_)) => {
                                Err(Rejection::from(ApiError::ServerShutdown))
                            }
                        }
                    }
                },
            )
            .map(finish_ok)
            .boxed()
    }

    fn raw_service(&self, out: SourceSender) -> BoxedFilter<(Response,)> {
        let protocol = self.protocol;
        let idx_ack = self.idx_ack.clone();
        let store_hec_token = self.store_hec_token;

        warp::post()
            .and(path!("raw" / "1.0").or(path!("raw")))
            .and(self.authorization())
            .and(SplunkSource::required_channel())
            .and(warp::addr::remote())
            .and(warp::header::optional::<String>("X-Forwarded-For"))
            .and(self.gzip())
            .and(warp::body::bytes())
            .and(warp::path::full())
            .and_then(
                move |_,
                      token: Option<String>,
                      channel_id: String,
                      remote: Option<SocketAddr>,
                      xff: Option<String>,
                      gzip: bool,
                      body: Bytes,
                      path: warp::path::FullPath| {
                    let mut out = out.clone();
                    let idx_ack = idx_ack.clone();
                    emit!(&HttpBytesReceived {
                        byte_size: body.len(),
                        http_path: path.as_str(),
                        protocol,
                    });

                    async move {
                        let (batch, receiver) =
                            BatchNotifier::maybe_new_with_receiver(idx_ack.is_some());
                        let maybe_ack_id = match (idx_ack, receiver) {
                            (Some(idx_ack), Some(receiver)) => Some(
                                idx_ack
                                    .get_ack_id_from_channel(channel_id.clone(), receiver)
                                    .await?,
                            ),
                            _ => None,
                        };
                        let mut event = raw_event(body, gzip, channel_id, remote, xff, batch)?;
                        event.metadata_mut().set_splunk_hec_token(
                            token.filter(|_| store_hec_token).map(Into::into),
                        );

                        let res = out.send(event).await;
                        res.map(|_| maybe_ack_id)
                            .map_err(|_| Rejection::from(ApiError::ServerShutdown))
                    }
                },
            )
            .map(finish_ok)
            .boxed()
    }

    fn health_service(&self) -> BoxedFilter<(Response,)> {
        let valid_credentials = self.valid_credentials.clone();
        let authorize =
            warp::header::optional("Authorization").and_then(move |token: Option<String>| {
                let valid_credentials = valid_credentials.clone();
                async move {
                    if valid_credentials.is_empty() {
                        return Ok(());
                    }
                    match token {
                        Some(token) if valid_credentials.contains(&token) => Ok(()),
                        _ => Err(Rejection::from(ApiError::BadRequest)),
                    }
                }
            });

        warp::get()
            .and(path!("health" / "1.0").or(path!("health")))
            .and(authorize)
            .map(move |_, _| warp::reply().into_response())
            .boxed()
    }

    fn ack_service(&self) -> BoxedFilter<(Response,)> {
        let idx_ack = self.idx_ack.clone();
        warp::post()
            .and(path!("ack"))
            .and(self.authorization())
            .and(SplunkSource::required_channel())
            .and(warp::body::json())
            .and_then(move |_, channel_id: String, body: HecAckStatusRequest| {
                let idx_ack = idx_ack.clone();
                async move {
                    if let Some(idx_ack) = idx_ack {
                        let ack_statuses = idx_ack
                            .get_acks_status_from_channel(channel_id, &body.acks)
                            .await?;
                        Ok(
                            warp::reply::json(&HecAckStatusResponse { acks: ack_statuses })
                                .into_response(),
                        )
                    } else {
                        Err(Rejection::from(ApiError::AckIsDisabled))
                    }
                }
            })
            .boxed()
    }

    fn options() -> BoxedFilter<(Response,)> {
        let post = warp::options()
            .and(
                path!("event")
                    .or(path!("event" / "1.0"))
                    .or(path!("raw" / "1.0"))
                    .or(path!("raw")),
            )
            .map(|_| warp::reply::with_header(warp::reply(), "Allow", "POST").into_response());

        let get = warp::options()
            .and(path!("health").or(path!("health" / "1.0")))
            .map(|_| warp::reply::with_header(warp::reply(), "Allow", "GET").into_response());

        post.or(get).unify().boxed()
    }

    /// Authorize request
    fn authorization(&self) -> BoxedFilter<(Option<String>,)> {
        let valid_credentials = self.valid_credentials.clone();
        warp::header::optional("Authorization")
            .and_then(move |token: Option<String>| {
                let valid_credentials = valid_credentials.clone();
                async move {
                    match (token, valid_credentials.is_empty()) {
                        // Remove the "Splunk " prefix if present as it is not
                        // part of the token itself
                        (token, true) => {
                            Ok(token
                                .map(|t| t.strip_prefix("Splunk ").map(Into::into).unwrap_or(t)))
                        }
                        (Some(token), false) if valid_credentials.contains(&token) => Ok(Some(
                            token
                                .strip_prefix("Splunk ")
                                .map(Into::into)
                                .unwrap_or(token),
                        )),
                        (Some(_), false) => Err(Rejection::from(ApiError::InvalidAuthorization)),
                        (None, false) => Err(Rejection::from(ApiError::MissingAuthorization)),
                    }
                }
            })
            .boxed()
    }

    /// Is body encoded with gzip
    fn gzip(&self) -> BoxedFilter<(bool,)> {
        warp::header::optional::<String>("Content-Encoding")
            .and_then(|encoding: Option<String>| async move {
                match encoding {
                    Some(s) if s.as_bytes() == b"gzip" => Ok(true),
                    Some(_) => Err(Rejection::from(ApiError::UnsupportedEncoding)),
                    None => Ok(false),
                }
            })
            .boxed()
    }

    fn required_channel() -> BoxedFilter<(String,)> {
        let splunk_channel_query_param = warp::query::<HashMap<String, String>>()
            .map(|qs: HashMap<String, String>| qs.get("channel").map(|v| v.to_owned()));
        let splunk_channel_header = warp::header::optional::<String>("x-splunk-request-channel");

        splunk_channel_header
            .and(splunk_channel_query_param)
            .and_then(|header: Option<String>, query_param| async move {
                header
                    .or(query_param)
                    .ok_or_else(|| Rejection::from(ApiError::MissingChannel))
            })
            .boxed()
    }
}
/// Constructs one or more events from json-s coming from reader.
/// If errors, it's done with input.
struct EventIterator<'de, R: JsonRead<'de>> {
    /// Remaining request with JSON events
    deserializer: serde_json::StreamDeserializer<'de, R, JsonValue>,
    /// Count of sent events
    events: usize,
    /// Optional channel from headers
    channel: Option<Value>,
    /// Default time
    time: Time,
    /// Remaining extracted default values
    extractors: [DefaultExtractor; 4],
    /// Event finalization
    batch: Option<Arc<BatchNotifier>>,
    /// Splunk HEC Token for passthrough
    token: Option<Arc<str>>,
}

impl<'de, R: JsonRead<'de>> EventIterator<'de, R> {
    fn new(
        deserializer: serde_json::StreamDeserializer<'de, R, JsonValue>,
        channel: Option<String>,
        remote: Option<SocketAddr>,
        remote_addr: Option<String>,
        batch: Option<Arc<BatchNotifier>>,
        token: Option<Arc<str>>,
    ) -> Self {
        EventIterator {
            deserializer,
            events: 0,
            channel: channel.map(Value::from),
            time: Time::Now(Utc::now()),
            extractors: [
                // Extract the host field with the given priority:
                // 1. The host field is present in the event payload
                // 2. The x-forwarded-for header is present in the incoming request
                // 3. Use the `remote`: SocketAddr value provided by warp
                DefaultExtractor::new_with(
                    "host",
                    log_schema().host_key(),
                    remote_addr
                        .or_else(|| remote.map(|addr| addr.to_string()))
                        .map(Value::from),
                ),
                DefaultExtractor::new("index", INDEX),
                DefaultExtractor::new("source", SOURCE),
                DefaultExtractor::new("sourcetype", SOURCETYPE),
            ],
            batch,
            token,
        }
    }

    fn build_event(&mut self, mut json: JsonValue) -> Result<Event, Rejection> {
        // Construct Event from parsed json event
        let mut event = Event::new_empty_log();
        let log = event.as_mut_log();

        // Add source type
        log.insert(log_schema().source_type_key(), Bytes::from("splunk_hec"));

        // Process event field
        match json.get_mut("event") {
            Some(event) => match event.take() {
                JsonValue::String(string) => {
                    if string.is_empty() {
                        return Err(ApiError::EmptyEventField { event: self.events }.into());
                    }
                    log.insert(log_schema().message_key(), string);
                }
                JsonValue::Object(mut object) => {
                    if object.is_empty() {
                        return Err(ApiError::EmptyEventField { event: self.events }.into());
                    }

                    // Add 'line' value as 'event::schema().message_key'
                    if let Some(line) = object.remove("line") {
                        match line {
                            // This don't quite fit the meaning of a event::schema().message_key
                            JsonValue::Array(_) | JsonValue::Object(_) => {
                                log.insert("line", line);
                            }
                            _ => {
                                log.insert(log_schema().message_key(), line);
                            }
                        }
                    }

                    for (key, value) in object {
                        log.insert(key, value);
                    }
                }
                _ => return Err(ApiError::InvalidDataFormat { event: self.events }.into()),
            },
            None => return Err(ApiError::MissingEventField { event: self.events }.into()),
        }

        // Process channel field
        if let Some(JsonValue::String(guid)) = json.get_mut("channel").map(JsonValue::take) {
            log.insert(CHANNEL, guid);
        } else if let Some(guid) = self.channel.as_ref() {
            log.insert(CHANNEL, guid.clone());
        }

        // Process fields field
        if let Some(JsonValue::Object(object)) = json.get_mut("fields").map(JsonValue::take) {
            for (key, value) in object {
                log.insert(key, value);
            }
        }

        // Process time field
        let parsed_time = match json.get_mut("time").map(JsonValue::take) {
            Some(JsonValue::Number(time)) => Some(Some(time)),
            Some(JsonValue::String(time)) => Some(time.parse::<serde_json::Number>().ok()),
            _ => None,
        };
        match parsed_time {
            None => (),
            Some(Some(t)) => {
                if let Some(t) = t.as_u64() {
                    let time = parse_timestamp(t as i64)
                        .ok_or(ApiError::InvalidDataFormat { event: self.events })?;

                    self.time = Time::Provided(time);
                } else if let Some(t) = t.as_f64() {
                    self.time = Time::Provided(Utc.timestamp(
                        t.floor() as i64,
                        (t.fract() * 1000.0 * 1000.0 * 1000.0) as u32,
                    ));
                } else {
                    return Err(ApiError::InvalidDataFormat { event: self.events }.into());
                }
            }
            Some(None) => return Err(ApiError::InvalidDataFormat { event: self.events }.into()),
        }

        // Add time field
        match self.time.clone() {
            Time::Provided(time) => log.insert(log_schema().timestamp_key(), time),
            Time::Now(time) => log.insert(log_schema().timestamp_key(), time),
        };

        // Extract default extracted fields
        for de in self.extractors.iter_mut() {
            de.extract(log, &mut json);
        }

        // Add passthrough token if present
        if let Some(token) = &self.token {
            log.metadata_mut()
                .set_splunk_hec_token(Some(Arc::clone(token)));
        }

        if let Some(batch) = self.batch.clone() {
            event.add_batch_notifier(batch);
        }

        emit!(&EventsReceived {
            count: 1,
            byte_size: event.size_of(),
        });
        self.events += 1;

        Ok(event)
    }
}

impl<'de, R: JsonRead<'de>> Iterator for EventIterator<'de, R> {
    type Item = Result<Event, Rejection>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.deserializer.next() {
            Some(Ok(json)) => Some(self.build_event(json)),
            None => {
                if self.events == 0 {
                    Some(Err(ApiError::NoData.into()))
                } else {
                    None
                }
            }
            Some(Err(error)) => {
                emit!(&SplunkHecRequestBodyInvalidError {
                    error: error.into()
                });
                Some(Err(
                    ApiError::InvalidDataFormat { event: self.events }.into()
                ))
            }
        }
    }
}

/// Parse a `i64` unix timestamp that can either be in seconds, milliseconds or
/// nanoseconds.
///
/// This attempts to parse timestamps based on what cutoff range they fall into.
/// For seconds to be parsed the timestamp must be less than the unix epoch of
/// the year `2400`. For this to parse milliseconds the time must be smaller
/// than the year `10,000` in unix epoch milliseconds. If the value is larger
/// than both we attempt to parse it as nanoseconds.
///
/// Returns `None` if `t` is negative.
fn parse_timestamp(t: i64) -> Option<DateTime<Utc>> {
    // Utc.ymd(2400, 1, 1).and_hms(0,0,0).timestamp();
    const SEC_CUTOFF: i64 = 13569465600;
    // Utc.ymd(10_000, 1, 1).and_hms(0,0,0).timestamp_millis();
    const MILLISEC_CUTOFF: i64 = 253402300800000;

    // Timestamps can't be negative!
    if t < 0 {
        return None;
    }

    let ts = if t < SEC_CUTOFF {
        Utc.timestamp(t, 0)
    } else if t < MILLISEC_CUTOFF {
        Utc.timestamp_millis(t)
    } else {
        Utc.timestamp_nanos(t)
    };

    Some(ts)
}

/// Maintains last known extracted value of field and uses it in the absence of field.
struct DefaultExtractor {
    field: &'static str,
    to_field: &'static str,
    value: Option<Value>,
}

impl DefaultExtractor {
    const fn new(field: &'static str, to_field: &'static str) -> Self {
        DefaultExtractor {
            field,
            to_field,
            value: None,
        }
    }

    fn new_with(
        field: &'static str,
        to_field: &'static str,
        value: impl Into<Option<Value>>,
    ) -> Self {
        DefaultExtractor {
            field,
            to_field,
            value: value.into(),
        }
    }

    fn extract(&mut self, log: &mut LogEvent, value: &mut JsonValue) {
        // Process json_field
        if let Some(JsonValue::String(new_value)) = value.get_mut(self.field).map(JsonValue::take) {
            self.value = Some(new_value.into());
        }

        // Add data field
        if let Some(index) = self.value.as_ref() {
            log.insert(self.to_field, index.clone());
        }
    }
}

/// For tracking origin of the timestamp
#[derive(Clone, Debug)]
enum Time {
    /// Backup
    Now(DateTime<Utc>),
    /// Provided in the request
    Provided(DateTime<Utc>),
}

/// Creates event from raw request
fn raw_event(
    bytes: Bytes,
    gzip: bool,
    channel: String,
    remote: Option<SocketAddr>,
    xff: Option<String>,
    batch: Option<Arc<BatchNotifier>>,
) -> Result<Event, Rejection> {
    // Process gzip
    let message: Value = if gzip {
        let mut data = Vec::new();
        match MultiGzDecoder::new(bytes.reader()).read_to_end(&mut data) {
            Ok(0) => return Err(ApiError::NoData.into()),
            Ok(_) => Value::from(Bytes::from(data)),
            Err(error) => {
                emit!(&SplunkHecRequestBodyInvalidError { error });
                return Err(ApiError::InvalidDataFormat { event: 0 }.into());
            }
        }
    } else {
        bytes.into()
    };

    // Construct event
    let mut event = Event::new_empty_log();
    let log = event.as_mut_log();

    // Add message
    log.insert(log_schema().message_key(), message);

    // Add channel
    log.insert(CHANNEL, channel);

    // host-field priority for raw endpoint:
    // - x-forwarded-for is set to `host` field first, if present. If not present:
    // - set remote addr to host field
    if let Some(remote_address) = xff {
        log.insert(log_schema().host_key(), remote_address);
    } else if let Some(remote) = remote {
        log.insert(log_schema().host_key(), remote.to_string());
    }

    // Add timestamp
    log.insert(log_schema().timestamp_key(), Utc::now());

    // Add source type
    event
        .as_mut_log()
        .try_insert(log_schema().source_type_key(), Bytes::from("splunk_hec"));
    if let Some(batch) = batch {
        event.add_batch_notifier(batch);
    }

    emit!(&EventsReceived {
        count: 1,
        byte_size: event.size_of(),
    });

    Ok(event)
}

#[derive(Clone, Copy, Debug, Snafu)]
pub(crate) enum ApiError {
    MissingAuthorization,
    InvalidAuthorization,
    UnsupportedEncoding,
    MissingChannel,
    NoData,
    InvalidDataFormat { event: usize },
    ServerShutdown,
    EmptyEventField { event: usize },
    MissingEventField { event: usize },
    BadRequest,
    ServiceUnavailable,
    AckIsDisabled,
}

impl warp::reject::Reject for ApiError {}

/// Cached bodies for common responses
mod splunk_response {
    use serde::Serialize;

    // https://docs.splunk.com/Documentation/Splunk/8.2.3/Data/TroubleshootHTTPEventCollector#Possible_error_codes
    pub enum HecStatusCode {
        Success = 0,
        TokenIsRequired = 2,
        InvalidAuthorization = 3,
        NoData = 5,
        InvalidDataFormat = 6,
        ServerIsBusy = 9,
        DataChannelIsMissing = 10,
        EventFieldIsRequired = 12,
        EventFieldCannotBeBlank = 13,
        AckIsDisabled = 14,
    }

    #[derive(Serialize)]
    pub enum HecResponseMetadata {
        #[serde(rename = "ackId")]
        AckId(u64),
        #[serde(rename = "invalid-event-number")]
        InvalidEventNumber(usize),
    }

    #[derive(Serialize)]
    pub struct HecResponse {
        text: &'static str,
        code: u8,
        #[serde(skip_serializing_if = "Option::is_none", flatten)]
        pub metadata: Option<HecResponseMetadata>,
    }

    impl HecResponse {
        pub const fn new(code: HecStatusCode) -> Self {
            let text = match code {
                HecStatusCode::Success => "Success",
                HecStatusCode::TokenIsRequired => "Token is required",
                HecStatusCode::InvalidAuthorization => "Invalid authorization",
                HecStatusCode::NoData => "No data",
                HecStatusCode::InvalidDataFormat => "Invalid data format",
                HecStatusCode::DataChannelIsMissing => "Data channel is missing",
                HecStatusCode::EventFieldIsRequired => "Event field is required",
                HecStatusCode::EventFieldCannotBeBlank => "Event field cannot be blank",
                HecStatusCode::ServerIsBusy => "Server is busy",
                HecStatusCode::AckIsDisabled => "Ack is disabled",
            };

            Self {
                text,
                code: code as u8,
                metadata: None,
            }
        }

        pub const fn with_metadata(mut self, metadata: HecResponseMetadata) -> Self {
            self.metadata = Some(metadata);
            self
        }
    }

    pub const INVALID_AUTHORIZATION: HecResponse =
        HecResponse::new(HecStatusCode::InvalidAuthorization);
    pub const TOKEN_IS_REQUIRED: HecResponse = HecResponse::new(HecStatusCode::TokenIsRequired);
    pub const NO_DATA: HecResponse = HecResponse::new(HecStatusCode::NoData);
    pub const SUCCESS: HecResponse = HecResponse::new(HecStatusCode::Success);
    pub const SERVER_IS_BUSY: HecResponse = HecResponse::new(HecStatusCode::ServerIsBusy);
    pub const NO_CHANNEL: HecResponse = HecResponse::new(HecStatusCode::DataChannelIsMissing);
    pub const ACK_IS_DISABLED: HecResponse = HecResponse::new(HecStatusCode::AckIsDisabled);
}

fn finish_ok(maybe_ack_id: Option<u64>) -> Response {
    let body = if let Some(ack_id) = maybe_ack_id {
        HecResponse::new(HecStatusCode::Success).with_metadata(HecResponseMetadata::AckId(ack_id))
    } else {
        splunk_response::SUCCESS
    };
    response_json(StatusCode::OK, &body)
}

async fn finish_err(rejection: Rejection) -> Result<(Response,), Rejection> {
    if let Some(&error) = rejection.find::<ApiError>() {
        emit!(&SplunkHecRequestError { error });
        Ok((match error {
            ApiError::MissingAuthorization => {
                response_json(StatusCode::UNAUTHORIZED, splunk_response::TOKEN_IS_REQUIRED)
            }
            ApiError::InvalidAuthorization => response_json(
                StatusCode::UNAUTHORIZED,
                splunk_response::INVALID_AUTHORIZATION,
            ),
            ApiError::UnsupportedEncoding => empty_response(StatusCode::UNSUPPORTED_MEDIA_TYPE),
            ApiError::MissingChannel => {
                response_json(StatusCode::BAD_REQUEST, splunk_response::NO_CHANNEL)
            }
            ApiError::NoData => response_json(StatusCode::BAD_REQUEST, splunk_response::NO_DATA),
            ApiError::ServerShutdown => empty_response(StatusCode::SERVICE_UNAVAILABLE),
            ApiError::InvalidDataFormat { event } => response_json(
                StatusCode::BAD_REQUEST,
                HecResponse::new(HecStatusCode::InvalidDataFormat)
                    .with_metadata(HecResponseMetadata::InvalidEventNumber(event)),
            ),
            ApiError::EmptyEventField { event } => response_json(
                StatusCode::BAD_REQUEST,
                HecResponse::new(HecStatusCode::EventFieldCannotBeBlank)
                    .with_metadata(HecResponseMetadata::InvalidEventNumber(event)),
            ),
            ApiError::MissingEventField { event } => response_json(
                StatusCode::BAD_REQUEST,
                HecResponse::new(HecStatusCode::EventFieldIsRequired)
                    .with_metadata(HecResponseMetadata::InvalidEventNumber(event)),
            ),
            ApiError::BadRequest => empty_response(StatusCode::BAD_REQUEST),
            ApiError::ServiceUnavailable => response_json(
                StatusCode::SERVICE_UNAVAILABLE,
                splunk_response::SERVER_IS_BUSY,
            ),
            ApiError::AckIsDisabled => {
                response_json(StatusCode::BAD_REQUEST, splunk_response::ACK_IS_DISABLED)
            }
        },))
    } else {
        Err(rejection)
    }
}

/// Response without body
fn empty_response(code: StatusCode) -> Response {
    let mut res = Response::default();
    *res.status_mut() = code;
    res
}

/// Response with body
fn response_json(code: StatusCode, body: impl Serialize) -> Response {
    warp::reply::with_status(warp::reply::json(&body), code).into_response()
}

#[cfg(feature = "sinks-splunk_hec")]
#[cfg(test)]
mod tests {
    use std::{net::SocketAddr, num::NonZeroU64};

    use chrono::{TimeZone, Utc};
    use futures_util::Stream;
    use reqwest::{RequestBuilder, Response};
    use serde::Deserialize;
    use vector_core::event::EventStatus;

    use super::{acknowledgements::HecAcknowledgementsConfig, parse_timestamp, SplunkConfig};
    use crate::{
        config::{log_schema, SinkConfig, SinkContext, SourceConfig, SourceContext},
        event::Event,
        sinks::{
            splunk_hec::logs::{config::HecLogsSinkConfig, encoder::HecLogsEncoder},
            util::{encoding::EncodingConfig, BatchConfig, Compression, TowerRequestConfig},
            Healthcheck, VectorSink,
        },
        sources::splunk_hec::acknowledgements::{HecAckStatusRequest, HecAckStatusResponse},
        test_util::{
            collect_n,
            components::{self, HTTP_PUSH_SOURCE_TAGS, SOURCE_TESTS},
            next_addr, wait_for_tcp,
        },
        SourceSender,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<SplunkConfig>();
    }

    /// Splunk token
    const TOKEN: &str = "token";
    const VALID_TOKENS: &[&str; 2] = &[TOKEN, "secondary-token"];

    async fn source(
        acknowledgements: Option<HecAcknowledgementsConfig>,
    ) -> (impl Stream<Item = Event> + Unpin, SocketAddr) {
        source_with(Some(TOKEN.to_owned()), None, acknowledgements, false).await
    }

    async fn source_with(
        token: Option<String>,
        valid_tokens: Option<&[&str]>,
        acknowledgements: Option<HecAcknowledgementsConfig>,
        store_hec_token: bool,
    ) -> (impl Stream<Item = Event> + Unpin, SocketAddr) {
        components::init_test();
        let (sender, recv) = SourceSender::new_test_finalize(EventStatus::Delivered);
        let address = next_addr();
        let valid_tokens =
            valid_tokens.map(|tokens| tokens.iter().map(|&token| String::from(token)).collect());
        let cx = SourceContext::new_test(sender);
        tokio::spawn(async move {
            SplunkConfig {
                address,
                token,
                valid_tokens,
                tls: None,
                acknowledgements: acknowledgements.unwrap_or_default(),
                store_hec_token,
            }
            .build(cx)
            .await
            .unwrap()
            .await
            .unwrap()
        });
        wait_for_tcp(address).await;
        (recv, address)
    }

    async fn sink(
        address: SocketAddr,
        encoding: impl Into<EncodingConfig<HecLogsEncoder>>,
        compression: Compression,
    ) -> (VectorSink, Healthcheck) {
        HecLogsSinkConfig {
            default_token: TOKEN.to_owned(),
            endpoint: format!("http://{}", address),
            host_key: "host".to_owned(),
            indexed_fields: vec![],
            index: None,
            sourcetype: None,
            source: None,
            encoding: encoding.into(),
            compression,
            batch: BatchConfig::default(),
            request: TowerRequestConfig::default(),
            tls: None,
            acknowledgements: Default::default(),
            timestamp_nanos_key: None,
        }
        .build(SinkContext::new_test())
        .await
        .unwrap()
    }

    async fn start(
        encoding: impl Into<EncodingConfig<HecLogsEncoder>>,
        compression: Compression,
        acknowledgements: Option<HecAcknowledgementsConfig>,
    ) -> (VectorSink, impl Stream<Item = Event> + Unpin) {
        let (source, address) = source(acknowledgements).await;
        let (sink, health) = sink(address, encoding, compression).await;
        assert!(health.await.is_ok());
        (sink, source)
    }

    async fn channel_n(
        messages: Vec<impl Into<Event> + Send + 'static>,
        sink: VectorSink,
        source: impl Stream<Item = Event> + Unpin,
    ) -> Vec<Event> {
        let n = messages.len();

        tokio::spawn(async move {
            sink.run_events(messages.into_iter().map(Into::into))
                .await
                .unwrap();
        });

        let events = collect_n(source, n).await;
        SOURCE_TESTS.assert(&HTTP_PUSH_SOURCE_TAGS);
        assert_eq!(n, events.len());

        events
    }

    #[derive(Clone, Copy, Debug)]
    enum Channel<'a> {
        Header(&'a str),
        QueryParam(&'a str),
    }

    #[derive(Default)]
    struct SendWithOpts<'a> {
        channel: Option<Channel<'a>>,
        forwarded_for: Option<String>,
    }

    async fn post(address: SocketAddr, api: &str, message: &str) -> u16 {
        let channel = Channel::Header("channel");
        let options = SendWithOpts {
            channel: Some(channel),
            forwarded_for: None,
        };
        send_with(address, api, message, TOKEN, &options).await
    }

    fn build_request(
        address: SocketAddr,
        api: &str,
        message: &str,
        token: &str,
        opts: &SendWithOpts<'_>,
    ) -> RequestBuilder {
        let mut b = reqwest::Client::new()
            .post(&format!("http://{}/{}", address, api))
            .header("Authorization", format!("Splunk {}", token));

        b = match opts.channel {
            Some(c) => match c {
                Channel::Header(v) => b.header("x-splunk-request-channel", v),
                Channel::QueryParam(v) => b.query(&[("channel", v)]),
            },
            None => b,
        };

        b = match &opts.forwarded_for {
            Some(f) => b.header("X-Forwarded-For", f),
            None => b,
        };

        b.body(message.to_owned())
    }

    async fn send_with<'a>(
        address: SocketAddr,
        api: &str,
        message: &str,
        token: &str,
        opts: &SendWithOpts<'_>,
    ) -> u16 {
        let b = build_request(address, api, message, token, opts);
        b.send().await.unwrap().status().as_u16()
    }

    async fn send_with_response<'a>(
        address: SocketAddr,
        api: &str,
        message: &str,
        token: &str,
        opts: &SendWithOpts<'_>,
    ) -> Response {
        let b = build_request(address, api, message, token, opts);
        b.send().await.unwrap()
    }

    #[tokio::test]
    async fn no_compression_text_event() {
        let message = "gzip_text_event";
        let (sink, source) = start(HecLogsEncoder::Text, Compression::None, None).await;

        let event = channel_n(vec![message], sink, source).await.remove(0);

        assert_eq!(event.as_log()[log_schema().message_key()], message.into());
        assert!(event.as_log().get(log_schema().timestamp_key()).is_some());
        assert_eq!(
            event.as_log()[log_schema().source_type_key()],
            "splunk_hec".into()
        );
        assert!(event.metadata().splunk_hec_token().is_none());
    }

    #[tokio::test]
    async fn one_simple_text_event() {
        let message = "one_simple_text_event";
        let (sink, source) = start(HecLogsEncoder::Text, Compression::gzip_default(), None).await;

        let event = channel_n(vec![message], sink, source).await.remove(0);

        assert_eq!(event.as_log()[log_schema().message_key()], message.into());
        assert!(event.as_log().get(log_schema().timestamp_key()).is_some());
        assert_eq!(
            event.as_log()[log_schema().source_type_key()],
            "splunk_hec".into()
        );
        assert!(event.metadata().splunk_hec_token().is_none());
    }

    #[tokio::test]
    async fn multiple_simple_text_event() {
        let n = 200;
        let (sink, source) = start(HecLogsEncoder::Text, Compression::None, None).await;

        let messages = (0..n)
            .map(|i| format!("multiple_simple_text_event_{}", i))
            .collect::<Vec<_>>();
        let events = channel_n(messages.clone(), sink, source).await;

        for (msg, event) in messages.into_iter().zip(events.into_iter()) {
            assert_eq!(event.as_log()[log_schema().message_key()], msg.into());
            assert!(event.as_log().get(log_schema().timestamp_key()).is_some());
            assert_eq!(
                event.as_log()[log_schema().source_type_key()],
                "splunk_hec".into()
            );
            assert!(event.metadata().splunk_hec_token().is_none());
        }
    }

    #[tokio::test]
    async fn one_simple_json_event() {
        let message = "one_simple_json_event";
        let (sink, source) = start(HecLogsEncoder::Json, Compression::gzip_default(), None).await;

        let event = channel_n(vec![message], sink, source).await.remove(0);

        assert_eq!(event.as_log()[log_schema().message_key()], message.into());
        assert!(event.as_log().get(log_schema().timestamp_key()).is_some());
        assert_eq!(
            event.as_log()[log_schema().source_type_key()],
            "splunk_hec".into()
        );
        assert!(event.metadata().splunk_hec_token().is_none());
    }

    #[tokio::test]
    async fn multiple_simple_json_event() {
        let n = 200;
        let (sink, source) = start(HecLogsEncoder::Json, Compression::gzip_default(), None).await;

        let messages = (0..n)
            .map(|i| format!("multiple_simple_json_event{}", i))
            .collect::<Vec<_>>();
        let events = channel_n(messages.clone(), sink, source).await;

        for (msg, event) in messages.into_iter().zip(events.into_iter()) {
            assert_eq!(event.as_log()[log_schema().message_key()], msg.into());
            assert!(event.as_log().get(log_schema().timestamp_key()).is_some());
            assert_eq!(
                event.as_log()[log_schema().source_type_key()],
                "splunk_hec".into()
            );
            assert!(event.metadata().splunk_hec_token().is_none());
        }
    }

    #[tokio::test]
    async fn json_event() {
        let (sink, source) = start(HecLogsEncoder::Json, Compression::gzip_default(), None).await;

        let mut event = Event::new_empty_log();
        event.as_mut_log().insert("greeting", "hello");
        event.as_mut_log().insert("name", "bob");
        sink.run_events(vec![event]).await.unwrap();

        let event = collect_n(source, 1).await.remove(0);
        assert_eq!(event.as_log()["greeting"], "hello".into());
        assert_eq!(event.as_log()["name"], "bob".into());
        assert!(event.as_log().get(log_schema().timestamp_key()).is_some());
        assert_eq!(
            event.as_log()[log_schema().source_type_key()],
            "splunk_hec".into()
        );
        assert!(event.metadata().splunk_hec_token().is_none());
    }

    #[tokio::test]
    async fn line_to_message() {
        let (sink, source) = start(HecLogsEncoder::Json, Compression::gzip_default(), None).await;

        let mut event = Event::new_empty_log();
        event.as_mut_log().insert("line", "hello");
        sink.run_events(vec![event]).await.unwrap();

        let event = collect_n(source, 1).await.remove(0);
        assert_eq!(event.as_log()[log_schema().message_key()], "hello".into());
        assert!(event.metadata().splunk_hec_token().is_none());
    }

    #[tokio::test]
    async fn raw() {
        let message = "raw";
        let (source, address) = source(None).await;

        assert_eq!(200, post(address, "services/collector/raw", message).await);

        let event = collect_n(source, 1).await.remove(0);
        SOURCE_TESTS.assert(&HTTP_PUSH_SOURCE_TAGS);
        assert_eq!(event.as_log()[log_schema().message_key()], message.into());
        assert_eq!(event.as_log()[&super::CHANNEL], "channel".into());
        assert!(event.as_log().get(log_schema().timestamp_key()).is_some());
        assert_eq!(
            event.as_log()[log_schema().source_type_key()],
            "splunk_hec".into()
        );
        assert!(event.metadata().splunk_hec_token().is_none());
    }

    #[tokio::test]
    async fn channel_header() {
        let message = "raw";
        let (source, address) = source(None).await;

        let opts = SendWithOpts {
            channel: Some(Channel::Header("guid")),
            forwarded_for: None,
        };

        assert_eq!(
            200,
            send_with(address, "services/collector/raw", message, TOKEN, &opts).await
        );

        let event = collect_n(source, 1).await.remove(0);
        SOURCE_TESTS.assert(&HTTP_PUSH_SOURCE_TAGS);
        assert_eq!(event.as_log()[&super::CHANNEL], "guid".into());
    }

    #[tokio::test]
    async fn xff_header_raw() {
        let message = "raw";
        let (source, address) = source(None).await;

        let opts = SendWithOpts {
            channel: Some(Channel::Header("guid")),
            forwarded_for: Some(String::from("10.0.0.1")),
        };

        assert_eq!(
            200,
            send_with(address, "services/collector/raw", message, TOKEN, &opts).await
        );

        let event = collect_n(source, 1).await.remove(0);
        SOURCE_TESTS.assert(&HTTP_PUSH_SOURCE_TAGS);
        assert_eq!(event.as_log()[log_schema().host_key()], "10.0.0.1".into());
    }

    // Test helps to illustrate that a payload's `host` value should override an x-forwarded-for header
    #[tokio::test]
    async fn xff_header_event_with_host_field() {
        let message = r#"{"event":"first", "host": "10.1.0.2"}"#;
        let (source, address) = source(None).await;

        let opts = SendWithOpts {
            channel: Some(Channel::Header("guid")),
            forwarded_for: Some(String::from("10.0.0.1")),
        };

        assert_eq!(
            200,
            send_with(address, "services/collector/event", message, TOKEN, &opts).await
        );

        let event = collect_n(source, 1).await.remove(0);
        SOURCE_TESTS.assert(&HTTP_PUSH_SOURCE_TAGS);
        assert_eq!(event.as_log()[log_schema().host_key()], "10.1.0.2".into());
    }

    // Test helps to illustrate that a payload's `host` value should override an x-forwarded-for header
    #[tokio::test]
    async fn xff_header_event_without_host_field() {
        let message = r#"{"event":"first", "color": "blue"}"#;
        let (source, address) = source(None).await;

        let opts = SendWithOpts {
            channel: Some(Channel::Header("guid")),
            forwarded_for: Some(String::from("10.0.0.1")),
        };

        assert_eq!(
            200,
            send_with(address, "services/collector/event", message, TOKEN, &opts).await
        );

        let event = collect_n(source, 1).await.remove(0);
        SOURCE_TESTS.assert(&HTTP_PUSH_SOURCE_TAGS);
        assert_eq!(event.as_log()[log_schema().host_key()], "10.0.0.1".into());
    }

    #[tokio::test]
    async fn channel_query_param() {
        let message = "raw";
        let (source, address) = source(None).await;

        let opts = SendWithOpts {
            channel: Some(Channel::QueryParam("guid")),
            forwarded_for: None,
        };

        assert_eq!(
            200,
            send_with(address, "services/collector/raw", message, TOKEN, &opts).await
        );

        let event = collect_n(source, 1).await.remove(0);
        SOURCE_TESTS.assert(&HTTP_PUSH_SOURCE_TAGS);
        assert_eq!(event.as_log()[&super::CHANNEL], "guid".into());
    }

    #[tokio::test]
    async fn no_data() {
        let (_source, address) = source(None).await;

        assert_eq!(400, post(address, "services/collector/event", "").await);
    }

    #[tokio::test]
    async fn invalid_token() {
        let (_source, address) = source(None).await;
        let opts = SendWithOpts {
            channel: Some(Channel::Header("channel")),
            forwarded_for: None,
        };

        assert_eq!(
            401,
            send_with(address, "services/collector/event", "", "nope", &opts).await
        );
    }

    #[tokio::test]
    async fn secondary_token() {
        let message = r#"{"event":"first", "color": "blue"}"#;
        let (_source, address) = source_with(None, Some(VALID_TOKENS), None, false).await;
        let options = SendWithOpts {
            channel: None,
            forwarded_for: None,
        };

        assert_eq!(
            200,
            send_with(
                address,
                "services/collector/event",
                message,
                VALID_TOKENS.get(1).unwrap(),
                &options
            )
            .await
        );
        SOURCE_TESTS.assert(&HTTP_PUSH_SOURCE_TAGS);
    }

    #[tokio::test]
    async fn event_service_token_passthrough_enabled() {
        let message = "passthrough_token_enabled";
        let (source, address) = source_with(None, Some(VALID_TOKENS), None, true).await;
        let (sink, health) = sink(address, HecLogsEncoder::Text, Compression::gzip_default()).await;
        assert!(health.await.is_ok());

        let event = channel_n(vec![message], sink, source).await.remove(0);

        SOURCE_TESTS.assert(&HTTP_PUSH_SOURCE_TAGS);
        assert_eq!(event.as_log()[log_schema().message_key()], message.into());
        assert_eq!(
            &event.metadata().splunk_hec_token().as_ref().unwrap()[..],
            TOKEN
        );
    }

    #[tokio::test]
    async fn raw_service_token_passthrough_enabled() {
        let message = "raw";
        let (source, address) = source_with(None, Some(VALID_TOKENS), None, true).await;

        assert_eq!(200, post(address, "services/collector/raw", message).await);

        let event = collect_n(source, 1).await.remove(0);
        SOURCE_TESTS.assert(&HTTP_PUSH_SOURCE_TAGS);
        assert_eq!(event.as_log()[log_schema().message_key()], message.into());
        assert_eq!(event.as_log()[&super::CHANNEL], "channel".into());
        assert!(event.as_log().get(log_schema().timestamp_key()).is_some());
        assert_eq!(
            event.as_log()[log_schema().source_type_key()],
            "splunk_hec".into()
        );
        assert_eq!(
            &event.metadata().splunk_hec_token().as_ref().unwrap()[..],
            TOKEN
        );
    }

    #[tokio::test]
    async fn no_authorization() {
        let message = "no_authorization";
        let (source, address) = source_with(None, None, None, false).await;
        let (sink, health) = sink(address, HecLogsEncoder::Text, Compression::gzip_default()).await;
        assert!(health.await.is_ok());

        let event = channel_n(vec![message], sink, source).await.remove(0);

        SOURCE_TESTS.assert(&HTTP_PUSH_SOURCE_TAGS);
        assert_eq!(event.as_log()[log_schema().message_key()], message.into());
        assert!(event.metadata().splunk_hec_token().is_none())
    }

    #[tokio::test]
    async fn no_authorization_token_passthrough_enabled() {
        let message = "no_authorization";
        let (source, address) = source_with(None, None, None, true).await;
        let (sink, health) = sink(address, HecLogsEncoder::Text, Compression::gzip_default()).await;
        assert!(health.await.is_ok());

        let event = channel_n(vec![message], sink, source).await.remove(0);

        SOURCE_TESTS.assert(&HTTP_PUSH_SOURCE_TAGS);
        assert_eq!(event.as_log()[log_schema().message_key()], message.into());
        assert_eq!(
            &event.metadata().splunk_hec_token().as_ref().unwrap()[..],
            TOKEN
        );
    }

    #[tokio::test]
    async fn partial() {
        let message = r#"{"event":"first"}{"event":"second""#;
        let (source, address) = source(None).await;

        assert_eq!(
            400,
            post(address, "services/collector/event", message).await
        );

        let event = collect_n(source, 1).await.remove(0);
        SOURCE_TESTS.assert(&HTTP_PUSH_SOURCE_TAGS);
        assert_eq!(event.as_log()[log_schema().message_key()], "first".into());
        assert!(event.as_log().get(log_schema().timestamp_key()).is_some());
        assert_eq!(
            event.as_log()[log_schema().source_type_key()],
            "splunk_hec".into()
        );
    }

    #[tokio::test]
    async fn handles_newlines() {
        let message = r#"
{"event":"first"}
        "#;
        let (source, address) = source(None).await;

        assert_eq!(
            200,
            post(address, "services/collector/event", message).await
        );

        let event = collect_n(source, 1).await.remove(0);
        SOURCE_TESTS.assert(&HTTP_PUSH_SOURCE_TAGS);
        assert_eq!(event.as_log()[log_schema().message_key()], "first".into());
        assert!(event.as_log().get(log_schema().timestamp_key()).is_some());
        assert_eq!(
            event.as_log()[log_schema().source_type_key()],
            "splunk_hec".into()
        );
    }

    #[tokio::test]
    async fn handles_spaces() {
        let message = r#" {"event":"first"} "#;
        let (source, address) = source(None).await;

        assert_eq!(
            200,
            post(address, "services/collector/event", message).await
        );

        let event = collect_n(source, 1).await.remove(0);
        SOURCE_TESTS.assert(&HTTP_PUSH_SOURCE_TAGS);
        assert_eq!(event.as_log()[log_schema().message_key()], "first".into());
        assert!(event.as_log().get(log_schema().timestamp_key()).is_some());
        assert_eq!(
            event.as_log()[log_schema().source_type_key()],
            "splunk_hec".into()
        );
    }

    #[tokio::test]
    async fn handles_non_utf8() {
        let message = b" {\"event\": { \"non\": \"A non UTF8 character \xE4\", \"number\": 2, \"bool\": true } } ";
        let (source, address) = source(None).await;

        let b = reqwest::Client::new()
            .post(&format!(
                "http://{}/{}",
                address, "services/collector/event"
            ))
            .header("Authorization", format!("Splunk {}", TOKEN))
            .body::<&[u8]>(message);

        assert_eq!(200, b.send().await.unwrap().status().as_u16());

        let event = collect_n(source, 1).await.remove(0);
        SOURCE_TESTS.assert(&HTTP_PUSH_SOURCE_TAGS);
        assert_eq!(event.as_log()["non"], "A non UTF8 character �".into());
        assert_eq!(event.as_log()["number"], 2.into());
        assert_eq!(event.as_log()["bool"], true.into());
        assert!(event.as_log().get(log_schema().timestamp_key()).is_some());
        assert_eq!(
            event.as_log()[log_schema().source_type_key()],
            "splunk_hec".into()
        );
    }

    #[tokio::test]
    async fn default() {
        let message = r#"{"event":"first","source":"main"}{"event":"second"}{"event":"third","source":"secondary"}"#;
        let (source, address) = source(None).await;

        assert_eq!(
            200,
            post(address, "services/collector/event", message).await
        );

        let events = collect_n(source, 3).await;

        SOURCE_TESTS.assert(&HTTP_PUSH_SOURCE_TAGS);
        assert_eq!(
            events[0].as_log()[log_schema().message_key()],
            "first".into()
        );
        assert_eq!(events[0].as_log()[&super::SOURCE], "main".into());

        assert_eq!(
            events[1].as_log()[log_schema().message_key()],
            "second".into()
        );
        assert_eq!(events[1].as_log()[&super::SOURCE], "main".into());

        assert_eq!(
            events[2].as_log()[log_schema().message_key()],
            "third".into()
        );
        assert_eq!(events[2].as_log()[&super::SOURCE], "secondary".into());
    }

    #[test]
    fn parse_timestamps() {
        let cases = vec![
            Utc::now(),
            Utc.ymd(1971, 11, 7).and_hms(1, 1, 1),
            Utc.ymd(2011, 8, 5).and_hms(1, 1, 1),
            Utc.ymd(2189, 11, 4).and_hms(2, 2, 2),
        ];

        for case in cases {
            let sec = case.timestamp();
            let millis = case.timestamp_millis();
            let nano = case.timestamp_nanos();

            assert_eq!(
                parse_timestamp(sec as i64).unwrap().timestamp(),
                case.timestamp()
            );
            assert_eq!(
                parse_timestamp(millis as i64).unwrap().timestamp_millis(),
                case.timestamp_millis()
            );
            assert_eq!(
                parse_timestamp(nano as i64).unwrap().timestamp_nanos(),
                case.timestamp_nanos()
            );
        }

        assert!(parse_timestamp(-1).is_none());
    }

    /// This test will fail once `warp` crate fixes support for
    /// custom connection listener, at that point this test can be
    /// modified to pass.
    /// https://github.com/timberio/vector/issues/7097
    /// https://github.com/seanmonstar/warp/issues/830
    /// https://github.com/seanmonstar/warp/pull/713
    #[tokio::test]
    async fn host_test() {
        let message = "for the host";
        let (sink, source) = start(HecLogsEncoder::Text, Compression::gzip_default(), None).await;

        let event = channel_n(vec![message], sink, source).await.remove(0);

        SOURCE_TESTS.assert(&HTTP_PUSH_SOURCE_TAGS);
        assert_eq!(event.as_log()[log_schema().message_key()], message.into());
        assert!(event.as_log().get(log_schema().host_key()).is_none());
    }

    #[derive(Deserialize)]
    struct HecAckEventResponse {
        text: String,
        code: u8,
        #[serde(rename = "ackId")]
        ack_id: u64,
    }

    #[tokio::test]
    async fn ack_json_event() {
        let ack_config = HecAcknowledgementsConfig {
            inner: true.into(),
            ..Default::default()
        };
        let (source, address) = source(Some(ack_config)).await;
        let event_message = r#"{"event":"first", "color": "blue"}{"event":"second"}"#;
        let opts = SendWithOpts {
            channel: Some(Channel::Header("guid")),
            forwarded_for: None,
        };
        let event_res = send_with_response(
            address,
            "services/collector/event",
            event_message,
            TOKEN,
            &opts,
        )
        .await
        .json::<HecAckEventResponse>()
        .await
        .unwrap();
        assert_eq!("Success", event_res.text.as_str());
        assert_eq!(0, event_res.code);
        let _ = collect_n(source, 1).await;

        let ack_message = serde_json::to_string(&HecAckStatusRequest {
            acks: vec![event_res.ack_id],
        })
        .unwrap();
        let ack_res = send_with_response(
            address,
            "services/collector/ack",
            ack_message.as_str(),
            TOKEN,
            &opts,
        )
        .await
        .json::<HecAckStatusResponse>()
        .await
        .unwrap();
        assert!(ack_res.acks.get(&event_res.ack_id).unwrap());
    }

    #[tokio::test]
    async fn ack_raw_event() {
        let ack_config = HecAcknowledgementsConfig {
            inner: true.into(),
            ..Default::default()
        };
        let (source, address) = source(Some(ack_config)).await;
        let event_message = "raw event message";
        let opts = SendWithOpts {
            channel: Some(Channel::Header("guid")),
            forwarded_for: None,
        };
        let event_res = send_with_response(
            address,
            "services/collector/raw",
            event_message,
            TOKEN,
            &opts,
        )
        .await
        .json::<HecAckEventResponse>()
        .await
        .unwrap();
        assert_eq!("Success", event_res.text.as_str());
        assert_eq!(0, event_res.code);
        let _ = collect_n(source, 1).await;

        let ack_message = serde_json::to_string(&HecAckStatusRequest {
            acks: vec![event_res.ack_id],
        })
        .unwrap();
        let ack_res = send_with_response(
            address,
            "services/collector/ack",
            ack_message.as_str(),
            TOKEN,
            &opts,
        )
        .await
        .json::<HecAckStatusResponse>()
        .await
        .unwrap();
        assert!(ack_res.acks.get(&event_res.ack_id).unwrap());
    }

    #[tokio::test]
    async fn ack_repeat_ack_query() {
        let ack_config = HecAcknowledgementsConfig {
            inner: true.into(),
            ..Default::default()
        };
        let (source, address) = source(Some(ack_config)).await;
        let event_message = "raw event message";
        let opts = SendWithOpts {
            channel: Some(Channel::Header("guid")),
            forwarded_for: None,
        };
        let event_res = send_with_response(
            address,
            "services/collector/raw",
            event_message,
            TOKEN,
            &opts,
        )
        .await
        .json::<HecAckEventResponse>()
        .await
        .unwrap();
        let _ = collect_n(source, 1).await;

        let ack_message = serde_json::to_string(&HecAckStatusRequest {
            acks: vec![event_res.ack_id],
        })
        .unwrap();
        let ack_res = send_with_response(
            address,
            "services/collector/ack",
            ack_message.as_str(),
            TOKEN,
            &opts,
        )
        .await
        .json::<HecAckStatusResponse>()
        .await
        .unwrap();
        assert!(ack_res.acks.get(&event_res.ack_id).unwrap());

        let ack_res = send_with_response(
            address,
            "services/collector/ack",
            ack_message.as_str(),
            TOKEN,
            &opts,
        )
        .await
        .json::<HecAckStatusResponse>()
        .await
        .unwrap();
        assert!(!ack_res.acks.get(&event_res.ack_id).unwrap());
    }

    #[tokio::test]
    async fn ack_exceed_max_number_of_ack_channels() {
        let ack_config = HecAcknowledgementsConfig {
            inner: true.into(),
            max_number_of_ack_channels: NonZeroU64::new(1).unwrap(),
            ..Default::default()
        };

        let (_source, address) = source(Some(ack_config)).await;
        let mut opts = SendWithOpts {
            channel: Some(Channel::Header("guid")),
            forwarded_for: None,
        };
        assert_eq!(
            200,
            send_with(address, "services/collector/raw", "message", TOKEN, &opts).await
        );

        opts.channel = Some(Channel::Header("other-guid"));
        assert_eq!(
            503,
            send_with(address, "services/collector/raw", "message", TOKEN, &opts).await
        );
        assert_eq!(
            503,
            send_with(
                address,
                "services/collector/event",
                r#"{"event":"first"}"#,
                TOKEN,
                &opts
            )
            .await
        );
    }

    #[tokio::test]
    async fn ack_exceed_max_pending_acks_per_channel() {
        let ack_config = HecAcknowledgementsConfig {
            inner: true.into(),
            max_pending_acks_per_channel: NonZeroU64::new(1).unwrap(),
            ..Default::default()
        };

        let (source, address) = source(Some(ack_config)).await;
        let opts = SendWithOpts {
            channel: Some(Channel::Header("guid")),
            forwarded_for: None,
        };
        for _ in 0..5 {
            send_with(
                address,
                "services/collector/event",
                r#"{"event":"first"}"#,
                TOKEN,
                &opts,
            )
            .await;
        }
        for _ in 0..5 {
            send_with(address, "services/collector/raw", "message", TOKEN, &opts).await;
        }
        let event_res = send_with_response(
            address,
            "services/collector/event",
            r#"{"event":"this will be acked"}"#,
            TOKEN,
            &opts,
        )
        .await
        .json::<HecAckEventResponse>()
        .await
        .unwrap();
        let _ = collect_n(source, 11).await;

        let ack_message_dropped = serde_json::to_string(&HecAckStatusRequest {
            acks: (0..10).collect::<Vec<u64>>(),
        })
        .unwrap();
        let ack_res = send_with_response(
            address,
            "services/collector/ack",
            ack_message_dropped.as_str(),
            TOKEN,
            &opts,
        )
        .await
        .json::<HecAckStatusResponse>()
        .await
        .unwrap();
        assert!(ack_res.acks.values().all(|ack_status| !*ack_status));

        let ack_message_acked = serde_json::to_string(&HecAckStatusRequest {
            acks: vec![event_res.ack_id],
        })
        .unwrap();
        let ack_res = send_with_response(
            address,
            "services/collector/ack",
            ack_message_acked.as_str(),
            TOKEN,
            &opts,
        )
        .await
        .json::<HecAckStatusResponse>()
        .await
        .unwrap();
        assert!(ack_res.acks.get(&event_res.ack_id).unwrap());
    }

    #[tokio::test]
    async fn event_service_acknowledgements_enabled_channel_required() {
        let message = r#"{"event":"first", "color": "blue"}"#;
        let ack_config = HecAcknowledgementsConfig {
            inner: true.into(),
            ..Default::default()
        };
        let (_, address) = source(Some(ack_config)).await;

        let opts = SendWithOpts {
            channel: None,
            forwarded_for: None,
        };

        assert_eq!(
            400,
            send_with(address, "services/collector/event", message, TOKEN, &opts).await
        );
    }

    #[tokio::test]
    async fn ack_service_acknowledgements_disabled() {
        let message = r#" {"acks":[0]} "#;
        let (_, address) = source(None).await;

        let opts = SendWithOpts {
            channel: Some(Channel::Header("guid")),
            forwarded_for: None,
        };

        assert_eq!(
            400,
            send_with(address, "services/collector/ack", message, TOKEN, &opts).await
        );
    }
}

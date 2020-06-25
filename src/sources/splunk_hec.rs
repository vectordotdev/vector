use crate::{
    event::{self, Event, LogEvent, Value},
    shutdown::ShutdownSignal,
    tls::{MaybeTlsSettings, TlsConfig},
    topology::config::{DataType, GlobalOptions, SourceConfig},
};
use bytes05::Bytes;
use chrono::{DateTime, TimeZone, Utc};
use flate2::read::GzDecoder;
use futures::{
    compat::{AsyncRead01CompatExt, Future01CompatExt, Stream01CompatExt},
    FutureExt, TryFutureExt, TryStreamExt,
};
use futures01::{sync::mpsc, Async, Future, Sink, Stream};
use http::StatusCode;
use lazy_static::lazy_static;
use serde::{de, Deserialize, Serialize};
use serde_json::{de::IoRead, json, Deserializer, Value as JsonValue};
use snafu::Snafu;
use std::{
    io::{Cursor, Read},
    net::{Ipv4Addr, SocketAddr},
};
use string_cache::DefaultAtom as Atom;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use warp::{filters::BoxedFilter, path, reject::Rejection, reply::Response, Filter, Reply};

// Event fields unique to splunk_hec source
lazy_static! {
    pub static ref CHANNEL: Atom = Atom::from("splunk_channel");
    pub static ref INDEX: Atom = Atom::from("splunk_index");
    pub static ref SOURCE: Atom = Atom::from("splunk_source");
    pub static ref SOURCETYPE: Atom = Atom::from("splunk_sourcetype");
}

/// Accepts HTTP requests.
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct SplunkConfig {
    /// Local address on which to listen
    #[serde(default = "default_socket_address")]
    address: SocketAddr,
    /// Splunk HEC token
    token: Option<String>,
    tls: Option<TlsConfig>,
}

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
            tls: None,
        }
    }
}

fn default_socket_address() -> SocketAddr {
    SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 8088)
}

#[typetag::serde(name = "splunk_hec")]
impl SourceConfig for SplunkConfig {
    fn build(
        &self,
        _: &str,
        _: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: mpsc::Sender<Event>,
    ) -> crate::Result<super::Source> {
        let source = SplunkSource::new(self);

        let event_service = source.event_service(out.clone());
        let raw_service = source.raw_service(out.clone());
        let health_service = source.health_service(out.clone());
        let options = SplunkSource::options();

        let services = path!("services" / "collector" / ..)
            .and(
                event_service
                    .or(raw_service)
                    .unify()
                    .or(health_service)
                    .unify()
                    .or(options)
                    .unify(),
            )
            .or_else(finish_err);

        let tls = MaybeTlsSettings::from_config(&self.tls, true)?;
        let incoming = tls.bind(&self.address)?.incoming();

        let fut = async move {
            let _ = warp::serve(services)
                .serve_incoming_with_graceful_shutdown(
                    incoming.compat().map_ok(|s| s.compat().compat()),
                    shutdown.clone().compat().map(|_| ()),
                )
                .await;
            // We need to drop the last copy of ShutdownSignalToken only after server has shut down.
            drop(shutdown);
            Ok(())
        };
        Ok(Box::new(fut.boxed().compat()))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "splunk_hec"
    }
}

/// Shared data for responding to requests.
struct SplunkSource {
    credentials: Option<Bytes>,
}

impl SplunkSource {
    fn new(config: &SplunkConfig) -> Self {
        SplunkSource {
            credentials: config
                .token
                .as_ref()
                .map(|token| format!("Splunk {}", token).into()),
        }
    }

    fn event_service(&self, out: mpsc::Sender<Event>) -> BoxedFilter<(Response,)> {
        warp::post()
            .and(path!("event").or(path!("event" / "1.0")))
            .and(self.authorization())
            .and(warp::header::optional::<String>("x-splunk-request-channel"))
            .and(warp::header::optional::<String>("host"))
            .and(self.gzip())
            .and(warp::body::bytes())
            .and_then(
                move |_,
                      _,
                      channel: Option<String>,
                      host: Option<String>,
                      gzip: bool,
                      body: Bytes| {
                    let out = out.clone();
                    async move {
                        // Construct event parser
                        if gzip {
                            EventStream::new(GzDecoder::new(Cursor::new(body)), channel, host)
                                .forward(out.clone().sink_map_err(|_| ApiError::ServerShutdown))
                                .map(|_| ())
                                .compat()
                                .await
                        } else {
                            EventStream::new(Cursor::new(body), channel, host)
                                .forward(out.clone().sink_map_err(|_| ApiError::ServerShutdown))
                                .map(|_| ())
                                .compat()
                                .await
                        }
                    }
                },
            )
            .map(finish_ok)
            .boxed()
    }

    fn raw_service(&self, out: mpsc::Sender<Event>) -> BoxedFilter<(Response,)> {
        warp::post()
            .and(path!("raw" / "1.0").or(path!("raw")))
            .and(self.authorization())
            .and(
                warp::header::optional::<String>("x-splunk-request-channel").and_then(
                    |channel: Option<String>| async {
                        if let Some(channel) = channel {
                            Ok(channel)
                        } else {
                            Err(Rejection::from(ApiError::MissingChannel))
                        }
                    },
                ),
            )
            .and(warp::header::optional::<String>("host"))
            .and(self.gzip())
            .and(warp::body::bytes())
            .and_then(
                move |_, _, channel: String, host: Option<String>, gzip: bool, body: Bytes| {
                    let out = out.clone();
                    async move {
                        // Construct event parser
                        futures01::stream::once(raw_event(body, gzip, channel, host))
                            .forward(out.clone().sink_map_err(|_| ApiError::ServerShutdown))
                            .map(|_| ())
                            .compat()
                            .await
                    }
                },
            )
            .map(finish_ok)
            .boxed()
    }

    fn health_service(&self, out: mpsc::Sender<Event>) -> BoxedFilter<(Response,)> {
        let credentials = self.credentials.clone();
        let authorize =
            warp::header::optional("Authorization").and_then(move |token: Option<String>| {
                let credentials = credentials.clone();
                async move {
                    match (token, credentials) {
                        (_, None) => Ok(()),
                        (Some(token), Some(password)) if token.as_bytes() == password.as_ref() => {
                            Ok(())
                        }
                        _ => Err(Rejection::from(ApiError::BadRequest)),
                    }
                }
            });

        warp::get()
            .and(path!("health" / "1.0").or(path!("health")))
            .and(authorize)
            .and_then(move |_, _| {
                let out = out.clone();
                async move {
                    match out.clone().poll_ready() {
                        Ok(Async::Ready(())) => Ok(warp::reply().into_response()),
                        // Since channel of mpsc::Sender increase by one with each sender, technically
                        // channel will never be full, and this will never be returned.
                        // This behavior dosn't fulfill one of purposes of healthcheck.
                        Ok(Async::NotReady) => Ok(warp::reply::with_status(
                            warp::reply(),
                            StatusCode::SERVICE_UNAVAILABLE,
                        )
                        .into_response()),
                        Err(_) => Err(Rejection::from(ApiError::ServerShutdown)),
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
    fn authorization(&self) -> BoxedFilter<((),)> {
        let credentials = self.credentials.clone();
        warp::header::optional("Authorization")
            .and_then(move |token: Option<String>| {
                let credentials = credentials.clone();
                async move {
                    match (token, credentials) {
                        (_, None) => Ok(()),
                        (Some(token), Some(password)) if token.as_bytes() == password.as_ref() => {
                            Ok(())
                        }
                        (Some(_), Some(_)) => Err(Rejection::from(ApiError::InvalidAuthorization)),
                        (None, Some(_)) => Err(Rejection::from(ApiError::MissingAuthorization)),
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
}

/// Constructs one ore more events from json-s coming from reader.
/// If errors, it's done with input.
struct EventStream<R: Read> {
    /// Remaining request with JSON events
    data: R,
    /// Count of sended events
    events: usize,
    /// Optinal channel from headers
    channel: Option<Value>,
    /// Default time
    time: Time,
    /// Remaining extracted default values
    extractors: [DefaultExtractor; 4],
}

impl<R: Read> EventStream<R> {
    fn new(data: R, channel: Option<String>, host: Option<String>) -> Self {
        EventStream {
            data,
            events: 0,
            channel: channel.map(|value| value.as_bytes().into()),
            time: Time::Now(Utc::now()),
            extractors: [
                DefaultExtractor::new_with(
                    "host",
                    &event::log_schema().host_key(),
                    host.map(|value| value.as_bytes().into()),
                ),
                DefaultExtractor::new("index", &INDEX),
                DefaultExtractor::new("source", &SOURCE),
                DefaultExtractor::new("sourcetype", &SOURCETYPE),
            ],
        }
    }

    /// As serde_json::from_reader, but doesn't require that all data has to be consumed,
    /// nor that it has to exist.
    fn from_reader_take<T>(&mut self) -> Result<Option<T>, serde_json::Error>
    where
        T: de::DeserializeOwned,
    {
        use serde_json::de::Read;
        let mut reader = IoRead::new(&mut self.data);
        match reader.peek()? {
            None => Ok(None),
            Some(_) => {
                Deserialize::deserialize(&mut Deserializer::new(reader)).map(|data| Some(data))
            }
        }
    }
}

impl<R: Read> Stream for EventStream<R> {
    type Item = Event;
    type Error = Rejection;
    fn poll(&mut self) -> Result<Async<Option<Event>>, Rejection> {
        // Parse JSON object
        let mut json = match self.from_reader_take::<JsonValue>() {
            Ok(Some(json)) => json,
            Ok(None) => {
                return if self.events == 0 {
                    Err(ApiError::NoData.into())
                } else {
                    Ok(Async::Ready(None))
                };
            }
            Err(error) => {
                error!(message = "Malformed request body",%error);
                return Err(ApiError::InvalidDataFormat { event: self.events }.into());
            }
        };

        // Concstruct Event from parsed json event
        let mut event = Event::new_empty_log();
        let log = event.as_mut_log();

        // Add source type
        log.insert(event::log_schema().source_type_key(), "splunk_hec");

        // Process event field
        match json.get_mut("event") {
            Some(event) => match event.take() {
                JsonValue::String(string) => {
                    if string.is_empty() {
                        return Err(ApiError::EmptyEventField { event: self.events }.into());
                    }
                    log.insert(event::log_schema().message_key().clone(), string);
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
                                log.insert(event::log_schema().message_key(), line);
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
            log.insert(CHANNEL.clone(), guid);
        } else if let Some(guid) = self.channel.as_ref() {
            log.insert(CHANNEL.clone(), guid.clone());
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
                        .ok_or_else(|| ApiError::InvalidDataFormat { event: self.events })?;

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
            Time::Provided(time) => log.insert(event::log_schema().timestamp_key().clone(), time),
            Time::Now(time) => log.insert(event::log_schema().timestamp_key().clone(), time),
        };

        // Extract default extracted fields
        for de in self.extractors.iter_mut() {
            de.extract(log, &mut json);
        }

        self.events += 1;

        Ok(Async::Ready(Some(event)))
    }
}

/// Parse a `i64` unix timestamp that can either be in seconds, milliseconds or
/// nanoseconds.
///
/// This attempts to parse timestamps based on what cutoff range they fall into.
/// For seconds to be parsed the timestamp must be less than the unix epoch of
/// the year `2400`. For this to parse milliseconds the time must be smaller
/// than the year `10,000` in unix epcoch milliseconds. If the value is larger
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
    to_field: &'static Atom,
    value: Option<Value>,
}

impl DefaultExtractor {
    fn new(field: &'static str, to_field: &'static Atom) -> Self {
        DefaultExtractor {
            field,
            to_field,
            value: None,
        }
    }

    fn new_with(
        field: &'static str,
        to_field: &'static Atom,
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
            log.insert(self.to_field.clone(), index.clone());
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
    host: Option<String>,
) -> Result<Event, Rejection> {
    // Process gzip
    let message: Value = if gzip {
        let mut data = Vec::new();
        match GzDecoder::new(Cursor::new(bytes)).read_to_end(&mut data) {
            Ok(0) => return Err(ApiError::NoData.into()),
            Ok(_) => data.into(),
            Err(error) => {
                error!(message = "Malformed request body",%error);
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
    log.insert(event::log_schema().message_key().clone(), message);

    // Add channel
    log.insert(CHANNEL.clone(), channel.as_bytes());

    // Add host
    if let Some(host) = host {
        log.insert(event::log_schema().host_key().clone(), host.as_bytes());
    }

    // Add timestamp
    log.insert(event::log_schema().timestamp_key().clone(), Utc::now());

    // Add source type
    event
        .as_mut_log()
        .try_insert(event::log_schema().source_type_key(), "splunk_hec");

    Ok(event)
}

#[derive(Debug, Snafu)]
enum ApiError {
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
}

impl From<ApiError> for Rejection {
    fn from(error: ApiError) -> Self {
        warp::reject::custom(error)
    }
}

impl warp::reject::Reject for ApiError {}

/// Cached bodies for common responses
mod splunk_response {
    use bytes::Bytes;
    use lazy_static::lazy_static;
    use serde_json::{json, Value};

    fn json_to_bytes(value: Value) -> Bytes {
        serde_json::to_string(&value).unwrap().into()
    }

    lazy_static! {
        pub static ref INVALID_AUTHORIZATION: Bytes =
            json_to_bytes(json!({"text":"Invalid authorization","code":3}));
        pub static ref MISSING_CREDENTIALS: Bytes =
            json_to_bytes(json!({"text":"Token is required","code":2}));
        pub static ref NO_DATA: Bytes = json_to_bytes(json!({"text":"No data","code":5}));
        pub static ref SUCCESS: Bytes = json_to_bytes(json!({"text":"Success","code":0}));
        pub static ref SERVER_ERROR: Bytes =
            json_to_bytes(json!({"text":"Internal server error","code":8}));
        pub static ref SERVER_SHUTDOWN: Bytes =
            json_to_bytes(json!({"text":"Server is shuting down","code":9}));
        pub static ref UNSUPPORTED_MEDIA_TYPE: Bytes =
            json_to_bytes(json!({"text":"unsupported content encoding"}));
        pub static ref NO_CHANNEL: Bytes =
            json_to_bytes(json!({"text":"Data channel is missing","code":10}));
    }
}

fn finish_ok(_: ()) -> Response {
    response_json(StatusCode::OK, splunk_response::SUCCESS.as_ref())
}

async fn finish_err(rejection: Rejection) -> Result<(Response,), Rejection> {
    if let Some(error) = rejection.find::<ApiError>() {
        Ok((match error {
            ApiError::MissingAuthorization => response_json(
                StatusCode::UNAUTHORIZED,
                splunk_response::MISSING_CREDENTIALS.as_ref(),
            ),
            ApiError::InvalidAuthorization => response_json(
                StatusCode::UNAUTHORIZED,
                splunk_response::INVALID_AUTHORIZATION.as_ref(),
            ),
            ApiError::UnsupportedEncoding => response_json(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                splunk_response::UNSUPPORTED_MEDIA_TYPE.as_ref(),
            ),
            ApiError::MissingChannel => response_json(
                StatusCode::BAD_REQUEST,
                splunk_response::NO_CHANNEL.as_ref(),
            ),
            ApiError::NoData => {
                response_json(StatusCode::BAD_REQUEST, splunk_response::NO_DATA.as_ref())
            }
            ApiError::ServerShutdown => response_json(
                StatusCode::SERVICE_UNAVAILABLE,
                splunk_response::SERVER_SHUTDOWN.as_ref(),
            ),
            ApiError::InvalidDataFormat { event } => event_error("Invalid data format", 6, *event),
            ApiError::EmptyEventField { event } => {
                event_error("Event field cannot be blank", 13, *event)
            }
            ApiError::MissingEventField { event } => {
                event_error("Event field is required", 12, *event)
            }
            ApiError::BadRequest => empty_response(StatusCode::BAD_REQUEST),
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

/// Error happened during parsing of events
fn event_error(text: &str, code: u16, event: usize) -> Response {
    let body = json!({
        "text":text,
        "code":code,
        "invalid-event-number":event
    });
    match serde_json::to_string(&body) {
        Ok(string) => response_json(StatusCode::BAD_REQUEST, string),
        Err(error) => {
            error!("Error encoding json body: {}", error);
            response_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                splunk_response::SERVER_ERROR.clone(),
            )
        }
    }
}

#[cfg(feature = "sinks-splunk_hec")]
#[cfg(test)]
mod tests {
    use super::{parse_timestamp, SplunkConfig};
    use crate::runtime::{Runtime, TaskExecutor};
    use crate::test_util::{self, collect_n, runtime};
    use crate::{
        event::{self, Event},
        shutdown::ShutdownSignal,
        sinks::{
            splunk_hec::{Encoding, HecSinkConfig},
            util::{encoding::EncodingConfigWithDefault, Compression},
            Healthcheck, RouterSink,
        },
        topology::config::{GlobalOptions, SinkConfig, SinkContext, SourceConfig},
    };
    use chrono::{TimeZone, Utc};
    use futures01::{stream, sync::mpsc, Sink};
    use http01::Method;
    use std::net::SocketAddr;

    /// Splunk token
    const TOKEN: &'static str = "token";

    const CHANNEL_CAPACITY: usize = 1000;

    fn source(rt: &mut Runtime) -> (mpsc::Receiver<Event>, SocketAddr) {
        source_with(rt, Some(TOKEN.to_owned()))
    }

    fn source_with(rt: &mut Runtime, token: Option<String>) -> (mpsc::Receiver<Event>, SocketAddr) {
        test_util::trace_init();
        let (sender, recv) = mpsc::channel(CHANNEL_CAPACITY);
        let address = test_util::next_addr();
        rt.spawn(
            SplunkConfig {
                address,
                token,
                tls: None,
            }
            .build(
                "default",
                &GlobalOptions::default(),
                ShutdownSignal::noop(),
                sender,
            )
            .unwrap(),
        );
        (recv, address)
    }

    fn sink(
        address: SocketAddr,
        encoding: impl Into<EncodingConfigWithDefault<Encoding>>,
        compression: Compression,
        exec: TaskExecutor,
    ) -> (RouterSink, Healthcheck) {
        HecSinkConfig {
            host: format!("http://{}", address),
            token: TOKEN.to_owned(),
            encoding: encoding.into(),
            compression,
            ..HecSinkConfig::default()
        }
        .build(SinkContext::new_test(exec))
        .unwrap()
    }

    fn start(
        encoding: impl Into<EncodingConfigWithDefault<Encoding>>,
        compression: Compression,
    ) -> (Runtime, RouterSink, mpsc::Receiver<Event>) {
        let mut rt = runtime();
        let (source, address) = source(&mut rt);
        let (sink, health) = sink(address, encoding, compression, rt.executor());
        assert!(rt.block_on(health).is_ok());
        (rt, sink, source)
    }

    fn channel_n(
        messages: Vec<impl Into<Event> + Send + 'static>,
        sink: RouterSink,
        source: mpsc::Receiver<Event>,
        rt: &mut Runtime,
    ) -> Vec<Event> {
        let n = messages.len();
        assert!(
            n <= CHANNEL_CAPACITY,
            "To much messages for the sink channel"
        );
        let pump = sink.send_all(stream::iter_ok(messages.into_iter().map(Into::into)));
        let _ = rt.block_on(pump).unwrap();
        let events = rt.block_on(collect_n(source, n)).unwrap();

        assert_eq!(n, events.len());

        events
    }

    fn post(address: SocketAddr, api: &str, message: &str) -> u16 {
        send_with(address, api, Method::POST, message, TOKEN)
    }

    fn send_with(
        address: SocketAddr,
        api: &str,
        method: Method,
        message: &str,
        token: &str,
    ) -> u16 {
        reqwest::Client::new()
            .request(method, &format!("http://{}/{}", address, api))
            .header("Authorization", format!("Splunk {}", token))
            .header("x-splunk-request-channel", "guid")
            .body(message.to_owned())
            .send()
            .unwrap()
            .status()
            .as_u16()
    }

    #[test]
    fn no_compression_text_event() {
        let message = "gzip_text_event";
        let (mut rt, sink, source) = start(Encoding::Text, Compression::None);

        let event = channel_n(vec![message], sink, source, &mut rt).remove(0);

        assert_eq!(
            event.as_log()[&event::log_schema().message_key()],
            message.into()
        );
        assert!(event
            .as_log()
            .get(&event::log_schema().timestamp_key())
            .is_some());
        assert_eq!(
            event.as_log()[event::log_schema().source_type_key()],
            "splunk_hec".into()
        );
    }

    #[test]
    fn one_simple_text_event() {
        let message = "one_simple_text_event";
        let (mut rt, sink, source) = start(Encoding::Text, Compression::Gzip);

        let event = channel_n(vec![message], sink, source, &mut rt).remove(0);

        assert_eq!(
            event.as_log()[&event::log_schema().message_key()],
            message.into()
        );
        assert!(event
            .as_log()
            .get(&event::log_schema().timestamp_key())
            .is_some());
        assert_eq!(
            event.as_log()[event::log_schema().source_type_key()],
            "splunk_hec".into()
        );
    }

    #[test]
    fn multiple_simple_text_event() {
        let n = 200;
        let (mut rt, sink, source) = start(Encoding::Text, Compression::None);

        let messages = (0..n)
            .into_iter()
            .map(|i| format!("multiple_simple_text_event_{}", i))
            .collect::<Vec<_>>();
        let events = channel_n(messages.clone(), sink, source, &mut rt);

        for (msg, event) in messages.into_iter().zip(events.into_iter()) {
            assert_eq!(
                event.as_log()[&event::log_schema().message_key()],
                msg.into()
            );
            assert!(event
                .as_log()
                .get(&event::log_schema().timestamp_key())
                .is_some());
            assert_eq!(
                event.as_log()[event::log_schema().source_type_key()],
                "splunk_hec".into()
            );
        }
    }

    #[test]
    fn one_simple_json_event() {
        let message = "one_simple_json_event";
        let (mut rt, sink, source) = start(Encoding::Json, Compression::Gzip);

        let event = channel_n(vec![message], sink, source, &mut rt).remove(0);

        assert_eq!(
            event.as_log()[&event::log_schema().message_key()],
            message.into()
        );
        assert!(event
            .as_log()
            .get(&event::log_schema().timestamp_key())
            .is_some());
        assert_eq!(
            event.as_log()[event::log_schema().source_type_key()],
            "splunk_hec".into()
        );
    }

    #[test]
    fn multiple_simple_json_event() {
        let n = 200;
        let (mut rt, sink, source) = start(Encoding::Json, Compression::Gzip);

        let messages = (0..n)
            .into_iter()
            .map(|i| format!("multiple_simple_json_event{}", i))
            .collect::<Vec<_>>();
        let events = channel_n(messages.clone(), sink, source, &mut rt);

        for (msg, event) in messages.into_iter().zip(events.into_iter()) {
            assert_eq!(
                event.as_log()[&event::log_schema().message_key()],
                msg.into()
            );
            assert!(event
                .as_log()
                .get(&event::log_schema().timestamp_key())
                .is_some());
            assert_eq!(
                event.as_log()[event::log_schema().source_type_key()],
                "splunk_hec".into()
            );
        }
    }

    #[test]
    fn json_event() {
        let (mut rt, sink, source) = start(Encoding::Json, Compression::Gzip);

        let mut event = Event::new_empty_log();
        event.as_mut_log().insert("greeting", "hello");
        event.as_mut_log().insert("name", "bob");

        let pump = sink.send(event);
        let _ = rt.block_on(pump).unwrap();
        let event = rt.block_on(collect_n(source, 1)).unwrap().remove(0);

        assert_eq!(event.as_log()[&"greeting".into()], "hello".into());
        assert_eq!(event.as_log()[&"name".into()], "bob".into());
        assert!(event
            .as_log()
            .get(&event::log_schema().timestamp_key())
            .is_some());
        assert_eq!(
            event.as_log()[event::log_schema().source_type_key()],
            "splunk_hec".into()
        );
    }

    #[test]
    fn line_to_message() {
        let (mut rt, sink, source) = start(Encoding::Json, Compression::Gzip);

        let mut event = Event::new_empty_log();
        event.as_mut_log().insert("line", "hello");

        let pump = sink.send(event);
        let _ = rt.block_on(pump).unwrap();
        let event = rt.block_on(collect_n(source, 1)).unwrap().remove(0);

        assert_eq!(
            event.as_log()[&event::log_schema().message_key()],
            "hello".into()
        );
    }

    #[test]
    fn raw() {
        let message = "raw";
        let mut rt = runtime();
        let (source, address) = source(&mut rt);

        assert_eq!(200, post(address, "services/collector/raw", message));

        let event = rt.block_on(collect_n(source, 1)).unwrap().remove(0);
        assert_eq!(
            event.as_log()[&event::log_schema().message_key()],
            message.into()
        );
        assert_eq!(event.as_log()[&super::CHANNEL], "guid".into());
        assert!(event
            .as_log()
            .get(&event::log_schema().timestamp_key())
            .is_some());
        assert_eq!(
            event.as_log()[event::log_schema().source_type_key()],
            "splunk_hec".into()
        );
    }

    #[test]
    fn no_data() {
        let mut rt = runtime();
        let (_source, address) = source(&mut rt);

        assert_eq!(400, post(address, "services/collector/event", ""));
    }

    #[test]
    fn invalid_token() {
        let mut rt = runtime();
        let (_source, address) = source(&mut rt);

        assert_eq!(
            401,
            send_with(
                address,
                "services/collector/event",
                Method::POST,
                "",
                "nope"
            )
        );
    }

    #[test]
    fn no_autorization() {
        let message = "no_autorization";
        let mut rt = runtime();
        let (source, address) = source_with(&mut rt, None);
        let (sink, health) = sink(address, Encoding::Text, Compression::Gzip, rt.executor());
        assert!(rt.block_on(health).is_ok());

        let event = channel_n(vec![message], sink, source, &mut rt).remove(0);

        assert_eq!(
            event.as_log()[&event::log_schema().message_key()],
            message.into()
        );
    }

    #[test]
    fn partial() {
        let message = r#"{"event":"first"}{"event":"second""#;
        let mut rt = runtime();
        let (source, address) = source(&mut rt);

        assert_eq!(400, post(address, "services/collector/event", message));

        let event = rt.block_on(collect_n(source, 1)).unwrap().remove(0);
        assert_eq!(
            event.as_log()[&event::log_schema().message_key()],
            "first".into()
        );
        assert!(event
            .as_log()
            .get(&event::log_schema().timestamp_key())
            .is_some());
        assert_eq!(
            event.as_log()[event::log_schema().source_type_key()],
            "splunk_hec".into()
        );
    }

    #[test]
    fn default() {
        let message = r#"{"event":"first","source":"main"}{"event":"second"}{"event":"third","source":"secondary"}"#;
        let mut rt = runtime();
        let (source, address) = source(&mut rt);

        assert_eq!(200, post(address, "services/collector/event", message));

        let events = rt.block_on(collect_n(source, 3)).unwrap();

        assert_eq!(
            events[0].as_log()[&event::log_schema().message_key()],
            "first".into()
        );
        assert_eq!(events[0].as_log()[&super::SOURCE], "main".into());

        assert_eq!(
            events[1].as_log()[&event::log_schema().message_key()],
            "second".into()
        );
        assert_eq!(events[1].as_log()[&super::SOURCE], "main".into());

        assert_eq!(
            events[2].as_log()[&event::log_schema().message_key()],
            "third".into()
        );
        assert_eq!(events[2].as_log()[&super::SOURCE], "secondary".into());
    }

    #[test]
    fn parse_timestamps() {
        let cases = vec![
            Utc::now(),
            Utc.ymd(1971, 11, 7).and_hms(1, 1, 1),
            Utc.ymd(2011, 08, 5).and_hms(1, 1, 1),
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
}

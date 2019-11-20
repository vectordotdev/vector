use crate::{
    event::{self, Event, LogEvent, ValueKind},
    topology::config::{DataType, GlobalOptions, SourceConfig},
};
use bytes::{buf::IntoBuf, Bytes};
use chrono::{TimeZone, Utc};
use flate2::read::GzDecoder;
use futures::{future, sync::mpsc, Async, Future, Sink, Stream};
use hyper::service::Service;
use hyper::{header::HeaderValue, Body, Method, Request, Response, Server, StatusCode};
use lazy_static::lazy_static;
use serde::{de, Deserialize, Serialize};
use serde_json::{de::IoRead, json, Deserializer, Value};
use std::sync::{Arc, Mutex};
use std::{
    io::Read,
    net::{Ipv4Addr, SocketAddr},
};
use stream_cancel::{Trigger, Tripwire};
use string_cache::DefaultAtom as Atom;

// Event fields unique to splunk_hec source
lazy_static! {
    pub static ref CHANNEL: Atom = Atom::from("channel");
    pub static ref INDEX: Atom = Atom::from("index");
    pub static ref SOURCE: Atom = Atom::from("source");
    pub static ref SOURCETYPE: Atom = Atom::from("sourcetype");
}

/// Cashed bodies for common responses
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

/// Accepts HTTP requests.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct SplunkConfig {
    /// Local address on which to listen
    #[serde(default = "default_socket_address")]
    address: SocketAddr,
    /// Splunk HEC token
    token: String,
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
        out: mpsc::Sender<Event>,
    ) -> crate::Result<super::Source> {
        let (trigger, tripwire) = Tripwire::new();

        let source = Arc::new(SplunkSource::new(self, out, trigger));

        let service = move || future::ok::<_, String>(Connection::new(source.clone()));

        // Build server
        let server = Server::bind(&self.address)
            .serve(service)
            .with_graceful_shutdown(tripwire)
            .map_err(|error| error!(message="Splunk HEC source stopped, because of error", %error));

        Ok(Box::new(server))
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
    /// Source output
    out: mpsc::Sender<Event>,
    /// Trigger for ending http server
    trigger: Arc<Mutex<Option<Trigger>>>,

    credentials: Bytes,
}

impl SplunkSource {
    fn new(config: &SplunkConfig, out: mpsc::Sender<Event>, trigger: Trigger) -> Self {
        SplunkSource {
            credentials: format!("Splunk {}", config.token).into(),
            out,
            trigger: Arc::new(Mutex::new(Some(trigger))),
        }
    }

    /// Sink shutdowns this source once source output is closed
    fn sink_with_shutdown(
        &self,
    ) -> impl Sink<SinkItem = Event, SinkError = Response<Body>> + 'static {
        let trigger = self.trigger.clone();
        self.out.clone().sink_map_err(move |_| {
            // Sink has been closed so server should stop listening
            trigger
                .try_lock()
                .map(|mut lock| {
                    // Stopping
                    lock.take();
                })
                // If locking fails, that means someone else is stopping it.
                .ok();

            response(
                StatusCode::SERVICE_UNAVAILABLE,
                splunk_response::SERVER_SHUTDOWN.clone(),
            )
        })
    }

    /// Ok if request is authorized to be done
    fn authorize(&self, req: &Request<Body>) -> Result<(), Response<Body>> {
        match req.headers().get("Authorization") {
            Some(credentials) if credentials.as_bytes() == self.credentials => Ok(()),
            Some(_) => Err(response(
                StatusCode::UNAUTHORIZED,
                splunk_response::INVALID_AUTHORIZATION.clone(),
            )),
            None => Err(response(
                StatusCode::UNAUTHORIZED,
                splunk_response::MISSING_CREDENTIALS.clone(),
            )),
        }
    }
}

/// One http connection
struct Connection {
    source: Arc<SplunkSource>,
}

impl Connection {
    fn new(source: Arc<SplunkSource>) -> Self {
        Connection { source }
    }

    fn is_gzip(req: &Request<Body>) -> Result<bool, Response<Body>> {
        match req.headers().get("Content-Encoding") {
            Some(s) if s.as_bytes() == b"gzip" => Ok(true),
            Some(_) => Err(response(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                splunk_response::UNSUPPORTED_MEDIA_TYPE.clone(),
            )),
            None => Ok(false),
        }
    }

    fn channel(req: &Request<Body>) -> Option<HeaderValue> {
        req.headers()
            .get("x-splunk-request-channel")
            .map(Clone::clone)
    }

    fn host(req: &Request<Body>) -> Option<HeaderValue> {
        req.headers().get("host").map(Clone::clone)
    }

    fn body_to_bytes(req: Request<Body>) -> impl Future<Item = Bytes, Error = crate::Error> {
        req.into_body()
            .map_err(|error| Box::new(error) as crate::Error)
            .concat2()
            .map(|chunk| chunk.into_bytes())
    }

    fn stream_events(
        read: impl Read,
        sink: impl Sink<SinkItem = Event, SinkError = Response<Body>>,
        channel: Option<HeaderValue>,
        host: Option<HeaderValue>,
    ) -> impl Future<Item = Response<Body>, Error = crate::Error> {
        EventStream::new(read, channel, host)
            .forward(sink)
            .then(Self::ok_success())
    }

    fn ok_success<T>(
    ) -> impl FnOnce(Result<T, Response<Body>>) -> future::FutureResult<Response<Body>, crate::Error>
    {
        |result| {
            future::ok(match result {
                Ok(_) => response(StatusCode::OK, splunk_response::SUCCESS.clone()),
                Err(response) => response,
            })
        }
    }

    /// Api point corespoding to '/services/collector/event/1.0'
    fn event_api(&self, req: Request<Body>) -> Result<RequestFuture, Response<Body>> {
        // Process header
        self.source.authorize(&req)?;
        let gzip = Self::is_gzip(&req)?;
        let channel = Self::channel(&req);
        let host = Self::host(&req);

        // Construct event parser
        let sink = self.source.sink_with_shutdown();
        if gzip {
            Ok(RequestFuture::from_future(
                Self::body_to_bytes(req)
                    .map(|bytes| GzDecoder::new(bytes.into_buf()))
                    .and_then(move |read| Self::stream_events(read, sink, channel, host)),
            ))
        } else {
            Ok(RequestFuture::from_future(
                Self::body_to_bytes(req)
                    .map(|bytes| bytes.into_buf())
                    .and_then(move |read| Self::stream_events(read, sink, channel, host)),
            ))
        }
    }

    /// Api point corespoding to '/services/collector/raw/1.0'
    fn raw_api(&self, req: Request<Body>) -> Result<RequestFuture, Response<Body>> {
        // Process header
        self.source.authorize(&req)?;
        let gzip = Self::is_gzip(&req)?;
        let channel = Self::channel(&req).ok_or_else(|| {
            response(StatusCode::BAD_REQUEST, splunk_response::NO_CHANNEL.clone())
        })?;
        let host = Self::host(&req);

        // Construct raw parser
        let sink = self.source.sink_with_shutdown();
        Ok(RequestFuture::from_future(
            Self::body_to_bytes(req).and_then(move |bytes| {
                futures::stream::once(raw_event(bytes, gzip, channel, host))
                    .forward(sink)
                    .then(Self::ok_success())
            }),
        ))
    }

    /// Api point corespoding to '/services/collector/health/1.0'
    fn health_api(&self, req: Request<Body>) -> Result<RequestFuture, Response<Body>> {
        // Process header
        self.source
            .authorize(&req)
            .map_err(|_| empty_response(StatusCode::BAD_REQUEST))?;

        Ok(match self.source.out.clone().poll_ready() {
            Ok(Async::Ready(())) => empty_response(StatusCode::OK),
            // Since channel of mpsc::Sender increase by one with each sender, technically
            // channel will never be full, and this will never be returned.
            // This behavior dosn't fulfill one of purposes of healthcheck.
            Ok(Async::NotReady) => empty_response(StatusCode::SERVICE_UNAVAILABLE),
            Err(_) => response(
                StatusCode::SERVICE_UNAVAILABLE,
                splunk_response::SERVER_SHUTDOWN.clone(),
            ),
        }
        .into())
    }
}

/// Responds to incoming requests
impl Service for Connection {
    type ReqBody = Body;
    type ResBody = Body;
    type Error = crate::Error;
    type Future = RequestFuture;
    fn call(&mut self, req: Request<Self::ReqBody>) -> Self::Future {
        trace!(request = ?req);
        match (req.method(), req.uri().path()) {
            // Accepts multiple log messages in inline and json format
            (&Method::POST, "/services/collector/event/1.0")
            | (&Method::POST, "/services/collector/event")
            | (&Method::POST, "/services/collector") => self.event_api(req).into(),
            // Accepts log message in raw format
            (&Method::POST, "/services/collector/raw/1.0")
            | (&Method::POST, "/services/collector/raw") => self.raw_api(req).into(),
            // Accepts healthcheck requests
            (&Method::GET, "/services/collector/health/1.0")
            | (&Method::GET, "/services/collector/health") => self.health_api(req).into(),
            // Accepts querying for options
            (&Method::OPTIONS, "/services/collector/event/1.0")
            | (&Method::OPTIONS, "/services/collector/event")
            | (&Method::OPTIONS, "/services/collector")
            | (&Method::OPTIONS, "/services/collector/raw/1.0")
            | (&Method::OPTIONS, "/services/collector/raw") => {
                empty_response_with_header(StatusCode::OK, &[("Allow", "POST")]).into()
            }
            // Accepts querying for options
            (&Method::OPTIONS, "/services/collector/health/1.0")
            | (&Method::OPTIONS, "/services/collector/health") => {
                empty_response_with_header(StatusCode::OK, &[("Allow", "GET")]).into()
            }
            // Unknown request
            _ => empty_response(StatusCode::NOT_FOUND).into(),
        }
    }
}

/// Returned by Connector as a response to a request.
enum RequestFuture {
    /// Response is already known
    Done(Option<Response<Body>>),
    /// Response is yet to be computed
    Future(Box<dyn Future<Item = Response<Body>, Error = crate::Error> + Send>),
}

impl RequestFuture {
    fn from_future(
        f: impl Future<Item = Response<Body>, Error = crate::Error> + 'static + Send,
    ) -> Self {
        RequestFuture::Future(Box::new(f) as _)
    }
}

impl Future for RequestFuture {
    type Item = Response<Body>;
    type Error = crate::Error;
    fn poll(&mut self) -> Result<Async<Self::Item>, Self::Error> {
        match self {
            RequestFuture::Done(done) => {
                Ok(Async::Ready(done.take().expect("cannot poll Future twice")))
            }
            RequestFuture::Future(future) => future.poll(),
        }
        .map(|asyn| {
            asyn.map(|response| {
                trace!(?response);
                response
            })
        })
    }
}

impl From<Response<Body>> for RequestFuture {
    fn from(r: Response<Body>) -> Self {
        RequestFuture::Done(Some(r))
    }
}

impl From<Result<RequestFuture, Response<Body>>> for RequestFuture {
    fn from(r: Result<RequestFuture, Response<Body>>) -> Self {
        match r {
            Ok(future) => future,
            Err(response) => response.into(),
        }
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
    channel: Option<ValueKind>,
    /// Extracted default time
    time: Option<ValueKind>,
    /// Remaining extracted default values
    extractors: [DefaultExtractor; 4],
}

impl<R: Read> EventStream<R> {
    fn new(data: R, channel: Option<HeaderValue>, host: Option<HeaderValue>) -> Self {
        EventStream {
            data,
            events: 0,
            channel: channel.map(|value| value.as_bytes().into()),
            time: None,
            extractors: [
                DefaultExtractor::new_with(&event::HOST, host.map(|value| value.as_bytes().into())),
                DefaultExtractor::new(&INDEX),
                DefaultExtractor::new(&SOURCE),
                DefaultExtractor::new(&SOURCETYPE),
            ],
        }
    }
}

impl<R: Read> Stream for EventStream<R> {
    type Item = Event;
    type Error = Response<Body>;
    fn poll(&mut self) -> Result<Async<Option<Event>>, Response<Body>> {
        // Parse JSON object
        let mut json = match from_reader_take::<_, Value>(&mut self.data) {
            Ok(Some(json)) => json,
            Ok(None) => {
                return if self.events == 0 {
                    Err(response(
                        StatusCode::BAD_REQUEST,
                        splunk_response::NO_DATA.clone(),
                    ))
                } else {
                    Ok(Async::Ready(None))
                }
            }
            Err(error) => {
                error!(message = "Malformed request body",%error);
                return Err(event_error("Invalid data format", 6, self.events));
            }
        };

        // Concstruct Event from parsed json event
        let mut event = Event::new_empty_log();
        let log = event.as_mut_log();

        // Process event field
        match json.get_mut("event") {
            Some(event) => match event.take() {
                Value::String(string) => {
                    if string.is_empty() {
                        return Err(event_error("Event field cannot be blank", 13, self.events));
                    }
                    log.insert_explicit(event::MESSAGE.clone(), string.into())
                }
                Value::Object(object) => {
                    if object.is_empty() {
                        return Err(event_error("Event field cannot be blank", 13, self.events));
                    }
                    for (name, value) in object {
                        insert(log, name, value);
                    }
                }
                _ => {
                    return Err(event_error("Invalid data format", 6, self.events));
                }
            },
            None => {
                return Err(event_error("Event field is required", 12, self.events));
            }
        }

        // Process channel field
        if let Some(Value::String(guid)) = json.get_mut("channel").map(Value::take) {
            log.insert_explicit(CHANNEL.clone(), guid.into());
        } else if let Some(guid) = self.channel.as_ref() {
            log.insert_explicit(CHANNEL.clone(), guid.clone());
        }

        // Process fields field
        if let Some(Value::Object(object)) = json.get_mut("fields").map(Value::take) {
            for (name, value) in object {
                insert(log, name, value);
            }
        }

        // Process time field
        let parsed_time = match json.get_mut("time").map(Value::take) {
            Some(Value::Number(time)) => Some(Some(time)),
            Some(Value::String(time)) => Some(time.parse::<serde_json::Number>().ok()),
            _ => None,
        };
        match parsed_time {
            None => (),
            Some(Some(t)) => {
                if let Some(t) = t.as_u64() {
                    self.time = Some(Utc.timestamp(t as i64, 0).into());
                } else if let Some(t) = t.as_f64() {
                    self.time = Some(
                        Utc.timestamp(
                            t.floor() as i64,
                            (t.fract() * 1000.0 * 1000.0 * 1000.0) as u32,
                        )
                        .into(),
                    );
                } else {
                    return Err(event_error("Invalid data format", 6, self.events));
                }
            }
            Some(None) => {
                return Err(event_error("Invalid data format", 6, self.events));
            }
        }

        // Add time field
        if let Some(time) = self.time.as_ref() {
            log.insert_explicit(event::TIMESTAMP.clone(), time.clone());
        }

        // Extract default extracted fields
        for de in self.extractors.iter_mut() {
            de.extract(log, &mut json);
        }

        self.events += 1;

        Ok(Async::Ready(Some(event)))
    }
}

/// Maintains last known extracted value of field and uses it in the absence of field.
struct DefaultExtractor {
    field: &'static Atom,
    value: Option<ValueKind>,
}

impl DefaultExtractor {
    fn new(field: &'static Atom) -> Self {
        DefaultExtractor { field, value: None }
    }

    fn new_with(field: &'static Atom, value: impl Into<Option<ValueKind>>) -> Self {
        DefaultExtractor {
            field,
            value: value.into(),
        }
    }

    fn extract(&mut self, log: &mut LogEvent, value: &mut Value) {
        // Process json_field
        if let Some(Value::String(new_value)) = value.get_mut(self.field.as_ref()).map(Value::take)
        {
            self.value = Some(new_value.into());
        }

        // Add data field
        if let Some(index) = self.value.as_ref() {
            log.insert_explicit(self.field.clone(), index.clone());
        }
    }
}

/// Creates event from raw request
fn raw_event(
    bytes: Bytes,
    gzip: bool,
    channel: HeaderValue,
    host: Option<HeaderValue>,
) -> Result<Event, Response<Body>> {
    // Process gzip
    let bytes = if gzip {
        let mut data = Vec::new();
        match GzDecoder::new(bytes.into_buf()).read_to_end(&mut data) {
            Ok(0) => {
                return Err(response(
                    StatusCode::BAD_REQUEST,
                    splunk_response::NO_DATA.clone(),
                ))
            }
            Ok(_) => data.into(),
            Err(error) => {
                error!(message = "Malformed request body",%error);
                return Err(event_error("Invalid data format", 6, 0));
            }
        }
    } else {
        bytes
    };

    // Construct event
    let mut event = Event::new_empty_log();
    let log = event.as_mut_log();

    // Add message
    log.insert_explicit(event::MESSAGE.clone(), bytes.into());

    // Add channel
    log.insert_explicit(CHANNEL.clone(), channel.as_bytes().into());

    // Add host
    if let Some(host) = host {
        log.insert_explicit(event::HOST.clone(), host.as_bytes().into());
    }

    Ok(event)
}

/// Recursevly inserts json values to event under given name
pub fn insert(event: &mut LogEvent, name: String, value: Value) {
    match value {
        Value::String(string) => {
            event.insert_explicit(name.into(), string.into());
        }
        Value::Number(number) => {
            let val: ValueKind = if let Some(val) = number.as_i64() {
                val.into()
            } else if let Some(val) = number.as_f64() {
                val.into()
            } else {
                number.to_string().into()
            };

            event.insert_explicit(name.into(), val);
        }
        Value::Bool(b) => {
            event.insert_explicit(name.into(), b.into());
        }
        Value::Null => {
            event.insert_explicit(name.into(), "".into());
        }
        Value::Array(array) => {
            for (i, element) in array.into_iter().enumerate() {
                let element_name = format!("{}[{}]", name, i);
                insert(event, element_name, element);
            }
        }
        Value::Object(object) => {
            for (key, value) in object.into_iter() {
                let item_name = format!("{}.{}", name, key);
                insert(event, item_name, value);
            }
        }
    }
}

/// As serde_json::from_reader, but doesn't require that all data has to be consumed,
/// nor that it has to exist.
pub fn from_reader_take<R, T>(rdr: R) -> Result<Option<T>, serde_json::Error>
where
    R: Read,
    T: de::DeserializeOwned,
{
    use serde_json::de::Read;
    let mut reader = IoRead::new(rdr);
    match reader.peek()? {
        None => Ok(None),
        Some(_) => Deserialize::deserialize(&mut Deserializer::new(reader)).map(|data| Some(data)),
    }
}

/// Response without body
fn empty_response(code: StatusCode) -> Response<Body> {
    let mut res = Response::default();
    *res.status_mut() = code;
    res
}

/// Response without body
fn empty_response_with_header(
    code: StatusCode,
    header: &[(&'static str, &'static str)],
) -> Response<Body> {
    let mut res = Response::default();
    *res.status_mut() = code;
    let headers = res.headers_mut();
    for &(key, value) in header {
        headers.insert(key, HeaderValue::from_static(value));
    }
    res
}

/// Response with body
fn response(code: StatusCode, body: impl Into<Body>) -> Response<Body> {
    let mut res = Response::new(body.into());
    *res.status_mut() = code;
    res
}

/// Error happened during parsing of events
fn event_error(text: &str, code: u16, event: usize) -> Response<Body> {
    let body = json!({
        "text":text,
        "code":code,
        "invalid-event-number":event
    });
    match serde_json::to_string(&body) {
        Ok(string) => response(StatusCode::BAD_REQUEST, string),
        Err(error) => {
            error!("Error encoding json body: {}", error);
            response(
                StatusCode::INTERNAL_SERVER_ERROR,
                splunk_response::SERVER_ERROR.clone(),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SplunkConfig;
    use crate::buffers::Acker;
    use crate::runtime::Runtime;
    use crate::sinks::splunk_hec::{Encoding, HecSinkConfig};
    use crate::sinks::{util::Compression, Healthcheck, RouterSink};
    use crate::test_util::{self, collect_n};
    use crate::{
        event::{self, Event},
        topology::config::{GlobalOptions, SinkConfig, SourceConfig},
    };
    use futures::{stream, sync::mpsc, Sink};
    use http::Method;
    use std::net::SocketAddr;

    /// Splunk token
    const TOKEN: &'static str = "token";

    const CHANNEL_CAPACITY: usize = 1000;

    fn source(rt: &mut Runtime) -> (mpsc::Receiver<Event>, SocketAddr) {
        test_util::trace_init();
        let (sender, recv) = mpsc::channel(CHANNEL_CAPACITY);
        let address = test_util::next_addr();
        rt.spawn(
            SplunkConfig {
                address,
                token: TOKEN.to_owned(),
            }
            .build("default", &GlobalOptions::default(), sender)
            .unwrap(),
        );
        (recv, address)
    }

    fn sink(
        address: SocketAddr,
        encoding: Encoding,
        compression: Compression,
    ) -> (RouterSink, Healthcheck) {
        HecSinkConfig {
            host: format!("http://{}", address),
            token: TOKEN.to_owned(),
            encoding,
            compression: Some(compression),
            ..HecSinkConfig::default()
        }
        .build(Acker::Null)
        .unwrap()
    }

    fn start(
        encoding: Encoding,
        compression: Compression,
    ) -> (Runtime, RouterSink, mpsc::Receiver<Event>) {
        let mut rt = test_util::runtime();
        let (source, address) = source(&mut rt);
        let (sink, health) = sink(address, encoding, compression);
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

        assert_eq!(event.as_log()[&event::MESSAGE], message.into());
    }

    #[test]
    fn one_simple_text_event() {
        let message = "one_simple_text_event";
        let (mut rt, sink, source) = start(Encoding::Text, Compression::Gzip);

        let event = channel_n(vec![message], sink, source, &mut rt).remove(0);

        assert_eq!(event.as_log()[&event::MESSAGE], message.into());
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
            assert_eq!(event.as_log()[&event::MESSAGE], msg.into());
        }
    }

    #[test]
    fn one_simple_json_event() {
        let message = "one_simple_json_event";
        let (mut rt, sink, source) = start(Encoding::Json, Compression::Gzip);

        let event = channel_n(vec![message], sink, source, &mut rt).remove(0);

        assert_eq!(event.as_log()[&event::MESSAGE], message.into());
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
            assert_eq!(event.as_log()[&event::MESSAGE], msg.into());
        }
    }

    #[test]
    fn json_event() {
        let (mut rt, sink, source) = start(Encoding::Json, Compression::Gzip);

        let mut event = Event::new_empty_log();
        event
            .as_mut_log()
            .insert_explicit("greeting".into(), "hello".into());
        event
            .as_mut_log()
            .insert_explicit("name".into(), "bob".into());

        let pump = sink.send(event);
        let _ = rt.block_on(pump).unwrap();
        let event = rt.block_on(collect_n(source, 1)).unwrap().remove(0);

        assert_eq!(event.as_log()[&"greeting".into()], "hello".into());
        assert_eq!(event.as_log()[&"name".into()], "bob".into());
    }

    #[test]
    fn raw() {
        let message = "raw";
        let mut rt = test_util::runtime();
        let (source, address) = source(&mut rt);

        assert_eq!(200, post(address, "services/collector/raw", message));

        let event = rt.block_on(collect_n(source, 1)).unwrap().remove(0);
        assert_eq!(event.as_log()[&event::MESSAGE], message.into());
        assert_eq!(event.as_log()[&super::CHANNEL], "guid".into());
    }

    #[test]
    fn no_data() {
        let mut rt = test_util::runtime();
        let (_source, address) = source(&mut rt);

        assert_eq!(400, post(address, "services/collector/event", ""));
    }

    #[test]
    fn invalid_token() {
        let mut rt = test_util::runtime();
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
    fn partial() {
        let message = r#"{"event":"first"}{"event":"second""#;
        let mut rt = test_util::runtime();
        let (source, address) = source(&mut rt);

        assert_eq!(400, post(address, "services/collector/event", message));

        let event = rt.block_on(collect_n(source, 1)).unwrap().remove(0);
        assert_eq!(event.as_log()[&event::MESSAGE], "first".into());
    }

    #[test]
    fn default() {
        let message = r#"{"event":"first","source":"main"}{"event":"second"}{"event":"third","source":"secondary"}"#;
        let mut rt = test_util::runtime();
        let (source, address) = source(&mut rt);

        assert_eq!(200, post(address, "services/collector/event", message));

        let events = rt.block_on(collect_n(source, 3)).unwrap();

        assert_eq!(events[0].as_log()[&event::MESSAGE], "first".into());
        assert_eq!(events[0].as_log()[&"source".into()], "main".into());

        assert_eq!(events[1].as_log()[&event::MESSAGE], "second".into());
        assert_eq!(events[1].as_log()[&"source".into()], "main".into());

        assert_eq!(events[2].as_log()[&event::MESSAGE], "third".into());
        assert_eq!(events[2].as_log()[&"source".into()], "secondary".into());
    }
}

use crate::{
    event::{self, Event, LogEvent, ValueKind},
    topology::config::{DataType, GlobalOptions, SourceConfig},
};
use bytes::{buf::IntoBuf, Bytes};
use chrono::{DateTime, TimeZone, Utc};
use codec::BytesDelimitedCodec;
use flate2::read::GzDecoder;
use futures::{future, sync::mpsc, Async, Future, Sink, Stream};
use hyper::service::{service_fn_ok, Service};
use hyper::{Body, Chunk, Method, Request, Response, Server, StatusCode, Uri};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::{
    io::{self, Read},
    net::{Ipv4Addr, SocketAddr},
};
use stream_cancel::{Trigger, Tripwire};
use string_cache::DefaultAtom as Atom;

// TODO: HTTPS

lazy_static! {
    pub static ref CHANNEL: Atom = Atom::from("channel");
    pub static ref INDEX: Atom = Atom::from("index");
    pub static ref SOURCE: Atom = Atom::from("source");
    pub static ref SOURCETYPE: Atom = Atom::from("sourcetype");
}

mod splunk_response {
    use bytes::Bytes;
    use lazy_static::lazy_static;
    use serde_json::json;
    lazy_static! {
        pub static ref INVALID_AUTHORIZATION: Bytes =
            r#"{"text":"Invalid authorization","code":3}"#.into();
        pub static ref MISSING_CREDENTIALS: Bytes =
            r#"{"text":"Token is required","code":2}"#.into();
        pub static ref NO_DATA: Bytes = r#"{"text":"No data","code":5}"#.into();
        pub static ref SUCCESS: Bytes = r#"{"text":"Success","code":0}"#.into();
        pub static ref SERVER_ERROR: Bytes = r#"{"text":"Internal server error","code":8}"#.into();
        pub static ref SERVER_SHUTDOWN: Bytes =
            r#"{"text":"Server is shuting down","code":9}"#.into();
        pub static ref UNSUPPORTED_MEDIA_TYPE: Bytes =
            r#"{"text":"unsupported content encoding"}"#.into();
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
    SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 8088)
}

#[typetag::serde(name = "splunk_hec")]
impl SourceConfig for SplunkConfig {
    fn build(
        &self,
        name: &str,
        globals: &GlobalOptions,
        out: mpsc::Sender<Event>,
    ) -> crate::Result<super::Source> {
        let (trigger, tripwire) = Tripwire::new();

        let source = Arc::new(SplunkSource::new(self, out, trigger));

        let service = move || future::ok::<_, String>(Connection::new(&source));

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

struct SplunkSource {
    /// Sorce output
    out: mpsc::Sender<Event>,
    /// Trigger for ending http server
    trigger: Mutex<Option<Trigger>>,

    credentials: Bytes,
}

impl SplunkSource {
    fn new(config: &SplunkConfig, out: mpsc::Sender<Event>, trigger: Trigger) -> Self {
        SplunkSource {
            credentials: format!("Splunk {}", config.token).into(),
            out,
            trigger: Mutex::new(Some(trigger)),
        }
    }

    /// Stops source
    fn stop(&self) {
        // If locking fails, that means someone else is closing it.
        self.trigger.try_lock().map(|mut lock| {
            lock.take();
        });
    }

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
    fn new(source: &Arc<SplunkSource>) -> Self {
        Connection {
            source: source.clone(),
        }
    }
}

impl Service for Connection {
    type ReqBody = Body;
    type ResBody = Body;
    type Error = crate::Error;
    type Future = RequestFuture;
    fn call(&mut self, req: Request<Self::ReqBody>) -> Self::Future {
        match (req.method(), req.uri().path()) {
            // Accepts multiple log messages in inline and json format
            (&Method::POST, "/services/collector/event/1.0")
            | (&Method::POST, "/services/collector/event")
            | (&Method::POST, "/services/collector") => {
                // Perform authorization
                if let Err(error) = self.source.authorize(&req) {
                    return error.into();
                }

                // Detect gzip
                let gzip = match req.headers().get("Content-Encoding") {
                    Some(s) if s.as_bytes() == b"gzip" => true,
                    Some(_) => {
                        return response(
                            StatusCode::UNSUPPORTED_MEDIA_TYPE,
                            splunk_response::UNSUPPORTED_MEDIA_TYPE.clone(),
                        )
                        .into()
                    }
                    None => false,
                };

                // Construct event parser
                let source = self.source.clone();
                let prelude = req
                    .into_body()
                    .map_err(|error| Box::new(error) as crate::Error)
                    .concat2()
                    .map(|chunk| chunk.into_bytes().into_buf());
                let sink = source.out.clone().sink_map_err(move |_| {
                    // Sink has been closed so server should stop listening
                    source.stop();
                    response(
                        StatusCode::SERVICE_UNAVAILABLE,
                        splunk_response::SERVER_SHUTDOWN.clone(),
                    )
                });
                let finish = |result| {
                    future::ok(match result {
                        Ok(()) => response(StatusCode::OK, splunk_response::SUCCESS.clone()),
                        Err(response) => response,
                    })
                };

                // Combine parts depending on gzip
                if gzip {
                    RequestFuture::from_future(prelude.map(GzDecoder::new).and_then(move |read| {
                        EventStream::new(read)
                            .forward(sink)
                            .map(|(_)| ())
                            .then(finish)
                    }))
                } else {
                    RequestFuture::from_future(prelude.and_then(move |read| {
                        EventStream::new(read)
                            .forward(sink)
                            .map(|(_)| ())
                            .then(finish)
                    }))
                }
            }
            // Accepts multiple log messages in raw format
            (&Method::POST, "/services/collector/raw/1.0")
            | (&Method::POST, "/services/collector/raw") => {
                unimplemented!();
            }
            // Accepts healthcheck requests
            (&Method::GET, "/services/collector/health/1.0")
            | (&Method::POST, "/services/collector/health") => {
                unimplemented!();
            }
            // Invalid request
            _ => {
                unimplemented!();
            }
        }
        // Ok(Response::new(Body::empty()))
    }
}

enum RequestFuture {
    Done(Option<Response<Body>>),
    Future(Box<dyn Future<Item = Response<Body>, Error = crate::Error> + Send>),
}

impl RequestFuture {
    fn from_future<F: Future<Item = Response<Body>, Error = crate::Error> + 'static + Send>(
        f: F,
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
            RequestFuture::Future(future) => return future.poll(),
        }
    }
}

impl From<Response<Body>> for RequestFuture {
    fn from(r: Response<Body>) -> Self {
        RequestFuture::Done(Some(r))
    }
}

fn response(code: StatusCode, body: impl Into<Body>) -> Response<Body> {
    let mut res = Response::new(body.into());
    *res.status_mut() = code;
    res
}

fn json_response(code: StatusCode, body: Value) -> Response<Body> {
    match serde_json::to_string(&body) {
        Ok(string) => response(code, string),
        Err(error) => {
            error!("Error encoding json body: {}", error);
            response(
                StatusCode::INTERNAL_SERVER_ERROR,
                splunk_response::SERVER_ERROR.clone(),
            )
        }
    }
}

fn event_error(text: &str, code: u16, event: usize) -> Response<Body> {
    json_response(
        StatusCode::BAD_REQUEST,
        json!({
            "text":text,
            "code":code,
            "invalid-event-number":event
        }),
    )
}

/// If errors it's done with input.
struct EventStream<R: Read> {
    /// Remaining request with JSON events
    data: R,
    /// Count of sended events
    events: usize,
    /// Extracted default time
    time: Option<ValueKind>,
    /// Remaining extracted default values
    extractors: [DefaultExtractor; 4],
}

impl<R: Read> EventStream<R> {
    fn new(data: R) -> Self {
        EventStream {
            data,
            events: 0,
            time: None,
            extractors: [
                DefaultExtractor::new(&event::HOST),
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
        let mut json = match serde_json::from_reader::<_, Value>(&mut self.data) {
            Ok(json) => json,
            Err(error) => {
                return if error.is_eof() {
                    if self.events == 0 {
                        Err(response(
                            StatusCode::BAD_REQUEST,
                            splunk_response::NO_DATA.clone(),
                        ))
                    } else {
                        // Assume EOF occured because data was empty
                        Ok(Async::Ready(None))
                    }
                } else {
                    error!(message = "Malformed request body",%error);
                    Err(event_error("Invalid data format", 6, self.events))
                };
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
        }

        // Process fields field
        if let Some(Value::Object(object)) = json.get_mut("fields").map(Value::take) {
            for (name, value) in object {
                insert(log, name, value);
            }
        }

        // Process time field
        let parsed_time = match json.get_mut("time").map(Value::take) {
            Some(Value::Number(time)) => Some(time.as_u64()),
            Some(Value::String(time)) => Some(time.parse::<u64>().ok()),
            _ => None,
        };
        match parsed_time {
            None => (),
            Some(Some(t)) => self.time = Some(Utc.timestamp(t as i64, 0).into()),
            Some(None) => return Err(event_error("Invalid data format", 6, self.events)),
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

struct DefaultExtractor {
    field: &'static Atom,
    value: Option<ValueKind>,
}

impl DefaultExtractor {
    fn new(field: &'static Atom) -> Self {
        DefaultExtractor { field, value: None }
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

// struct KnownRead<R: Read> {
//     read: R,
//     done: u64,
//     len: u64,
// }

// impl<R: Read> KnownRead<R> {
//     fn new(read: R, len: u64) -> Self {
//         KnownRead { read, done: 0, len }
//     }

//     fn is_empty(&self) -> bool {
//         self.done >= self.len
//     }

//     fn done(&mut self, r: io::Result<usize>) -> io::Result<usize> {
//         unimplemented!();
//     }
// }

// impl<R: Read> io::Read for KnownRead<R> {
//     fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
//         self.done(self.read.read(buf))
//     }

//     // fn read_vectored(&mut self, bufs: &mut [IoSliceMut]) -> Result<usize> { ... }
//     // unsafe fn initializer(&self) -> Initializer { ... }
//     // fn read_to_end(&mut self, buf: &mut Vec<u8>) -> Result<usize> { ... }
//     // fn read_to_string(&mut self, buf: &mut String) -> Result<usize> { ... }
//     // fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> { ... }
// }

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

use chrono::{Date, Utc};
use elastic_responses::{bulk::BulkErrorsResponse, parse};
use futures::{
    stream::FuturesUnordered,
    sync::oneshot::{spawn, SpawnHandle},
    Async, AsyncSink, Future, Sink, Stream,
};
use hyper::{client::HttpConnector, Body, Client, Request, Uri};
use log::info;
use serde::Serialize;
use serde_json::json;
use std::{marker::PhantomData, mem};
use tokio::executor::DefaultExecutor;
use uuid::Uuid;
use Record;

pub trait Document {
    type Body: Serialize;

    fn app_id(&self) -> &str;
    fn id(&self) -> Uuid;
    fn ty(&self) -> &str;
    fn dt(&self) -> Date<Utc>;
    fn body(&self) -> Self::Body;

    fn index(&self) -> String {
        format!("a-{}-{}", self.app_id(), self.dt().format("%F"))
    }
}

// for testing
impl<T: Serialize> Document for T {
    type Body = serde_json::Value;

    fn app_id(&self) -> &str {
        "12345"
    }

    fn id(&self) -> Uuid {
        Uuid::new_v4()
    }

    fn ty(&self) -> &str {
        "log_lines"
    }

    fn dt(&self) -> Date<Utc> {
        Utc::today()
    }

    fn body(&self) -> Self::Body {
        json!({ "msg": self })
    }
}

impl Document for Record {
    type Body = serde_json::Value;

    fn app_id(&self) -> &str {
        "12345"
    }

    fn id(&self) -> Uuid {
        Uuid::new_v4()
    }

    fn ty(&self) -> &str {
        "log_lines"
    }

    fn dt(&self) -> Date<Utc> {
        Utc::today()
    }

    fn body(&self) -> Self::Body {
        json!({ "msg": self.line })
    }
}

pub struct ElasticseachSink<T: Document> {
    client: Client<HttpConnector, Body>,
    buffer: Vec<u8>,
    buffer_limit: usize,
    buffered_lines: usize,
    in_flight_requests: FuturesUnordered<SpawnHandle<(), ()>>,
    in_flight_limit: usize,
    _pd: PhantomData<T>,
}

impl<T: Document> ElasticseachSink<T> {
    pub fn new() -> Self {
        let client: Client<_, Body> = Client::builder()
            .executor(DefaultExecutor::current())
            .build_http();

        Self {
            client,
            buffer: Vec::new(),
            // TODO: configurable
            buffer_limit: 2 * 1024 * 1024,
            buffered_lines: 0,
            in_flight_requests: FuturesUnordered::new(),
            in_flight_limit: 3, // TODO: configurable
            _pd: PhantomData,
        }
    }

    // TODO: do better than string errors
    fn add_to_buffer(&mut self, msg: T) -> Result<(), String> {
        let action = json!({
            "index": {
                "_index": msg.index(),
                "_type": msg.ty(),
                "_id": msg.id().to_string(),
            }
        });

        serde_json::to_writer(&mut self.buffer, &action)
            .map_err(|e| format!("serialization error! {}", e))?;
        self.buffer.push(b'\n');

        serde_json::to_writer(&mut self.buffer, &msg.body())
            .map_err(|e| format!("serialization error! {}", e))?;
        self.buffer.push(b'\n');

        self.buffered_lines += 1;
        Ok(())
    }

    fn spawn_request(&mut self, body: Vec<u8>) -> SpawnHandle<(), ()> {
        // TODO: configurable
        let uri: Uri = "http://localhost:9200/_bulk".parse().unwrap();

        let request = Request::post(uri)
            .header("Content-Type", "application/x-ndjson")
            .body(body.into())
            .unwrap();

        let request = self
            .client
            .request(request)
            .map_err(|e| println!("error sending request: {:?}", e))
            .and_then(|response| {
                let (parts, body) = response.into_parts();
                info!("got response! {:?}", parts);
                body.concat2()
                    .map(move |body| {
                        let parsed: Result<BulkErrorsResponse, _> =
                            parse().from_reader(parts.status.as_u16(), body.as_ref());

                        // TODO: do something with result
                        // info!("res body:\n{:#?}", parsed);
                    }).map_err(|e| println!("error reading body: {:?}", e))
            });

        spawn(request, &DefaultExecutor::current())
    }
}

impl<T: Document> Sink for ElasticseachSink<T> {
    type SinkItem = T;
    type SinkError = String; // TODO: better than string errors

    fn start_send(
        &mut self,
        item: Self::SinkItem,
    ) -> Result<AsyncSink<Self::SinkItem>, Self::SinkError> {
        if self.buffer.len() >= self.buffer_limit {
            self.poll_complete()?;

            if self.buffer.len() >= self.buffer_limit {
                return Ok(AsyncSink::NotReady(item));
            }
        }

        self.add_to_buffer(item)?;
        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Result<Async<()>, Self::SinkError> {
        loop {
            if self.buffer.is_empty() && self.in_flight_requests.is_empty() {
                return Ok(Async::Ready(()));

            // do we have records to send and room for another request?
            } else if !self.buffer.is_empty()
                && self.in_flight_requests.len() < self.in_flight_limit
            {
                info!(
                    "preparing to send request of {} messages ({} bytes)",
                    self.buffered_lines,
                    self.buffer.len(),
                );

                // existing buffer becomes request body, replace with fresh buffer
                // TODO: use a Buf instead
                let body = mem::replace(&mut self.buffer, Vec::new());
                self.buffered_lines = 0;

                let request = self.spawn_request(body);
                self.in_flight_requests.push(request);

            // do we have in flight requests we need to poll?
            } else if !self.in_flight_requests.is_empty() {
                if let Ok(Async::NotReady) = self.in_flight_requests.poll() {
                    return Ok(Async::NotReady);
                }

            // catch any unexpected states instead of looping forever
            } else {
                panic!("this should only be possible if in_flight_limit < 1, which is broken")
            }
        }
    }
}

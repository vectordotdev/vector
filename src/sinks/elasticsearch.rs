use chrono::{Date, Utc};
use futures::{try_ready, Async, AsyncSink, Future, Sink, Stream};
use hyper::{
    client::{HttpConnector, ResponseFuture},
    Body, Client, Request, Uri,
};
use log::info;
use serde::Serialize;
use serde_json::{json, Value};
use std::{marker::PhantomData, mem};
use tokio::executor::DefaultExecutor;
use uuid::Uuid;

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

pub struct ElasticseachSink<T: Document> {
    client: Client<HttpConnector, Body>,
    buffer: Vec<u8>,
    buffer_limit: usize,
    buffered_lines: usize,
    state: SinkState,
    _pd: PhantomData<T>,
}

enum SinkState {
    // zero in-flight requests
    Ready,
    // one in-flight request
    Waiting {
        body: Vec<u8>, // keep this around for retries
        response_future: ResponseFuture,
    },
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
            state: SinkState::Ready,
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

    fn initiate_request(&mut self, body: Vec<u8>) -> ResponseFuture {
        // TODO: configurable
        let uri: Uri = "http://localhost:9200/_bulk".parse().unwrap();

        let request = Request::put(uri)
            .header("Content-Type", "application/x-ndjson")
            .body(body.into())
            .unwrap();

        self.client.request(request)
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
            self.state = match self.state {
                SinkState::Ready { .. } => if !self.buffer.is_empty() {
                    // if we're ready and have messages to send, initiate sending them
                    info!(
                        "preparing to send request of {} messages ({} bytes)",
                        self.buffered_lines,
                        self.buffer.len(),
                    );

                    // existing buffer becomes request body, replace with fresh buffer
                    // TODO: use a Buf instead
                    let body = mem::replace(&mut self.buffer, Vec::new());
                    self.buffered_lines = 0;

                    let response_future = self.initiate_request(body.clone());

                    SinkState::Waiting {
                        body,
                        response_future,
                    }
                } else {
                    return Ok(Async::Ready(()));
                },

                SinkState::Waiting {
                    body: ref _body,
                    ref mut response_future,
                } => {
                    let response =
                        try_ready!(response_future.poll().map_err(|e| format!("err: {}", e)));
                    info!("got response! {:?}", response);
                    // info!("req body was:\n{}", std::str::from_utf8(body).unwrap());
                    if response.status().is_success() {
                        // done, go back to ready state
                        SinkState::Ready
                    } else {
                        // request failed
                        // TODO: retry with body
                        response
                            .into_body()
                            .concat2()
                            .map(|body| {
                                let parsed: Value = serde_json::from_slice(&body).unwrap();
                                info!(
                                    "res body:\n{}",
                                    serde_json::to_string_pretty(&parsed).unwrap()
                                )
                            }).map_err(|e| println!("body err: {:?}", e))
                            .wait() // TODO: waiting is BAD, find a better way to get errors
                            .unwrap();

                        // probably actually go back to waiting on retry request
                        SinkState::Ready
                    }
                }
            }
        }
    }
}

use crate::record::Record;
use chrono::{Date, Utc};
use elastic_responses::{bulk::BulkErrorsResponse, parse};
use futures::{
    stream::FuturesUnordered,
    sync::oneshot::{self, SpawnHandle},
    Async, AsyncSink, Future, Sink, Stream,
};
use hyper::{client::HttpConnector, Body, Client, Request, Uri};
use log::{error, info};
use serde::Serialize;
use serde_json::json;
use std::{marker::PhantomData, mem};
use tokio::executor::DefaultExecutor;
use tokio_retry::{
    strategy::{jitter, ExponentialBackoff},
    Retry,
};
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

pub struct ElasticsearchSink<T> {
    client: Client<HttpConnector, Body>,
    buffer: Vec<u8>,
    buffer_limit: usize,
    buffered_lines: usize,
    in_flight_requests: FuturesUnordered<SpawnHandle<(), String>>,
    in_flight_limit: usize,
    _pd: PhantomData<T>,
}

impl<T: Document> ElasticsearchSink<T> {
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
    fn add_to_buffer(&mut self, msg: &T) -> Result<(), String> {
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

    fn spawn_request(&mut self, body: Vec<u8>) -> SpawnHandle<(), String> {
        // this is cheap and reuses the same connection pools, etc
        // TODO: try to make the whole client Send + Sync so we don't need this?
        let client = self.client.clone();

        // before jitter, this gives us 15ms, 225ms, and 3.375s retries
        let retry_strategy = ExponentialBackoff::from_millis(15).map(jitter).take(3);

        // TODO: request ids for logging
        let request = Retry::spawn(retry_strategy, move || {
            // TODO: configurable
            let uri: Uri = "http://localhost:9200/_bulk".parse().unwrap();

            let request = Request::post(uri)
                .header("Content-Type", "application/x-ndjson")
                .body(body.clone().into()) // TODO: don't actually clone the whole vec everytime
                .unwrap();

            client
                .request(request)
                .and_then(|response| {
                    let (parts, body) = response.into_parts();
                    info!("got response headers! status code {:?}", parts.status);
                    body.concat2().map(|body| (parts, body))
                })
                .map_err(|e| format!("request error: {:?}", e))
                .and_then(|(parts, body)| {
                    parse::<BulkErrorsResponse>()
                        .from_reader(parts.status.as_u16(), body.as_ref())
                        .map_err(|e| format!("response error: {:?}", e))
                })
                .and_then(|response| {
                    // TODO: use the response to build a new body for retries that include
                    // only the failed items
                    if response.is_err() {
                        Err(format!("{} bulk items failed", response.iter().count()))
                    } else {
                        info!("all bulk items succeeded!");
                        Ok(())
                    }
                })
        })
        .map_err(|e| match e {
            tokio_retry::Error::OperationError(e) => {
                format!("retry limited exhausted, dropping request: {}", e)
            }
            tokio_retry::Error::TimerError(e) => format!("timer error during retry: {}", e),
        });

        oneshot::spawn(request, &DefaultExecutor::current())
    }
}

impl<T: Document> Sink for ElasticsearchSink<T> {
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

        self.add_to_buffer(&item)?;
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
                match self.in_flight_requests.poll() {
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Ok(Async::Ready(Some(()))) => {} // request finished normally, continue
                    Err(e) => error!("{}", e), // request finished with an error, just log and continue
                    Ok(Async::Ready(None)) => {
                        unreachable!("got Ready(None) with requests in flight")
                    }
                }

            // catch any unexpected states instead of looping forever
            } else {
                panic!("this should only be possible if in_flight_limit < 1, which is broken")
            }
        }
    }
}

impl ElasticsearchSink<Record> {
    pub fn build() -> super::RouterSink {
        Box::new(Self::new().sink_map_err(|e| error!("es sink error: {:?}", e)))
    }

    pub fn healthcheck() -> super::Healthcheck {
        let client: Client<_, Body> = Client::builder()
            .executor(DefaultExecutor::current())
            .build_http();

        let uri: Uri = "http://localhost:9200/_cluster/health".parse().unwrap();

        let request = Request::post(uri).body(Body::empty()).unwrap();

        let healthcheck = client
            .request(request)
            .map_err(|err| err.to_string())
            .and_then(|response| {
                if response.status() == hyper::StatusCode::OK {
                    Ok(())
                } else {
                    Err(format!("Unexpected status: {}", response.status()))
                }
            });

        Box::new(healthcheck)
    }
}

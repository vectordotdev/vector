use super::config::{Encoding, LokiConfig, OutOfOrderAction};
use super::service::LokiService;
use crate::config::log_schema;
use crate::config::SinkContext;
use crate::http::{Auth, HttpClient};
use crate::internal_events::{
    LokiEventUnlabeled, LokiEventsProcessed, LokiOutOfOrderEventDropped,
    LokiOutOfOrderEventRewritten, TemplateRenderingFailed,
};
use crate::sinks::util::buffer::loki::{
    GlobalTimestamps, Labels, LokiBatch, LokiEvent, LokiRecord, PartitionKey,
};
use crate::sinks::util::encoding::{EncodingConfig, EncodingConfiguration};
use crate::sinks::util::UriSerde;
use crate::template::Template;
use futures::stream::{BoxStream, Stream, StreamExt};
use http::Request;
use hyper::Body;
use pin_project::pin_project;
use shared::encode_logfmt;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::sync::mpsc;
use tower::{Service, ServiceExt};
use tracing_futures::Instrument;
use vector_core::buffers::Acker;
use vector_core::event::{self, Event, EventStatus, Value};
use vector_core::partition::Partitioner;
use vector_core::sink::StreamSink;
use vector_core::stream::batcher::Batcher;

#[derive(Clone)]
pub struct KeyPartitioner(Option<Template>);

impl KeyPartitioner {
    pub const fn new(template: Option<Template>) -> Self {
        Self(template)
    }
}

impl Partitioner for KeyPartitioner {
    type Item = Event;
    type Key = Option<String>;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        self.0.as_ref().and_then(|t| {
            t.render_string(item)
                .map_err(|error| {
                    emit!(&TemplateRenderingFailed {
                        error,
                        field: Some("tenant_id"),
                        drop_event: false,
                    })
                })
                .ok()
        })
    }
}

struct RecordPartitionner;

impl Partitioner for RecordPartitionner {
    type Item = LokiRecord;
    type Key = PartitionKey;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        item.partition.clone()
    }
}

#[derive(Clone)]
pub struct RequestBuilder {
    uri: String,
    auth: Option<Auth>,
}

impl RequestBuilder {
    pub fn new(endpoint: UriSerde, auth: Option<Auth>) -> Self {
        let uri = format!("{}loki/api/v1/push", endpoint.uri);

        Self { uri, auth }
    }
}

impl RequestBuilder {
    fn build(
        &self,
        key: PartitionKey,
        value: Vec<LokiRecord>,
    ) -> crate::Result<http::Request<hyper::Body>> {
        let tenant_id = key.tenant_id;
        let batch = LokiBatch::from(value);
        let body = serde_json::to_vec(&batch)?;

        emit!(&LokiEventsProcessed {
            byte_size: body.len(),
        });

        let mut req = http::Request::post(&self.uri).header("Content-Type", "application/json");

        if let Some(tenant_id) = tenant_id {
            req = req.header("X-Scope-OrgID", tenant_id);
        }

        let body = hyper::Body::from(body);
        let mut req = req.body(body).unwrap();

        if let Some(auth) = &self.auth {
            auth.apply(&mut req);
        }

        Ok(req)
    }
}

#[derive(Clone)]
pub(super) struct EventEncoder {
    key_partitioner: KeyPartitioner,
    encoding: EncodingConfig<Encoding>,
    labels: HashMap<Template, Template>,
    remove_label_fields: bool,
    remove_timestamp: bool,
}

impl EventEncoder {
    fn build_labels(&self, event: &Event) -> Vec<(String, String)> {
        self.labels
            .iter()
            .filter_map(|(key_template, value_template)| {
                if let (Ok(key), Ok(value)) = (
                    key_template.render_string(event),
                    value_template.render_string(event),
                ) {
                    Some((key, value))
                } else {
                    None
                }
            })
            .collect()
    }

    fn remove_label_fields(&self, event: &mut Event) {
        if self.remove_label_fields {
            for template in self.labels.values() {
                if let Some(fields) = template.get_fields() {
                    for field in fields {
                        event.as_mut_log().remove(&field);
                    }
                }
            }
        }
    }

    pub(super) fn encode_event(&self, mut event: Event) -> LokiRecord {
        let tenant_id = self.key_partitioner.partition(&event);
        let mut labels = self.build_labels(&event);
        self.remove_label_fields(&mut event);

        let timestamp = match event.as_log().get(log_schema().timestamp_key()) {
            Some(event::Value::Timestamp(ts)) => ts.timestamp_nanos(),
            _ => chrono::Utc::now().timestamp_nanos(),
        };

        if self.remove_timestamp {
            event.as_mut_log().remove(log_schema().timestamp_key());
        }

        self.encoding.apply_rules(&mut event);
        let log = event.into_log();
        let event = match &self.encoding.codec() {
            Encoding::Json => {
                serde_json::to_string(&log).expect("json encoding should never fail.")
            }

            Encoding::Text => log
                .get(log_schema().message_key())
                .map(Value::to_string_lossy)
                .unwrap_or_default(),

            Encoding::Logfmt => encode_logfmt::to_string(log.into_parts().0)
                .expect("Logfmt encoding should never fail."),
        };

        // If no labels are provided we set our own default
        // `{agent="vector"}` label. This can happen if the only
        // label is a templatable one but the event doesn't match.
        if labels.is_empty() {
            emit!(&LokiEventUnlabeled);
            labels = vec![("agent".to_string(), "vector".to_string())]
        }

        let partition = PartitionKey::new(tenant_id, &mut labels);

        LokiRecord {
            labels,
            event: LokiEvent { timestamp, event },
            partition,
        }
    }
}

struct FilterEncoderCache {
    partition: Option<(PartitionKey, Labels)>,
    latest_timestamp: Option<i64>,
    global_timestamps: GlobalTimestamps,
}

impl FilterEncoderCache {
    fn update(&mut self, record: &LokiRecord) {
        let partition = &record.partition;
        if self.latest_timestamp.is_none() {
            self.partition = Some((record.partition.clone(), record.labels.clone()));
            self.latest_timestamp = self.global_timestamps.take(partition);
        }
    }
}

#[pin_project]
struct FilterEncoder<St> {
    #[pin]
    input: St,
    encoder: EventEncoder,
    cache: FilterEncoderCache,
    out_of_order_action: OutOfOrderAction,
}

impl<St> FilterEncoder<St> {
    fn new(
        input: St,
        encoder: EventEncoder,
        global_timestamps: GlobalTimestamps,
        out_of_order_action: OutOfOrderAction,
    ) -> Self {
        Self {
            input,
            encoder,
            cache: FilterEncoderCache {
                partition: None,
                latest_timestamp: None,
                global_timestamps,
            },
            out_of_order_action,
        }
    }
}

impl<St> Stream for FilterEncoder<St>
where
    St: Stream<Item = Event> + Unpin,
{
    type Item = LokiRecord;

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        match this.input.as_mut().poll_next(cx) {
            Poll::Ready(Some(item)) => {
                let mut item = this.encoder.encode_event(item);

                this.cache.update(&item);

                // TODO: gauge/count of labels.
                let latest_timestamp = this.cache.latest_timestamp.unwrap_or(item.event.timestamp);

                if item.event.timestamp < latest_timestamp {
                    match this.out_of_order_action {
                        OutOfOrderAction::Drop => {
                            emit!(&LokiOutOfOrderEventDropped);
                            Poll::Ready(None)
                        }
                        OutOfOrderAction::RewriteTimestamp => {
                            emit!(&LokiOutOfOrderEventRewritten);
                            item.event.timestamp = latest_timestamp;
                            Poll::Ready(Some(item))
                        }
                    }
                } else {
                    Poll::Ready(Some(item))
                }
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[derive(Clone)]
pub struct LokiSink {
    acker: Acker,
    client: HttpClient,
    request_builder: RequestBuilder,
    pub(super) encoder: EventEncoder,
    max_batch_size: usize,
    max_batch_bytes: usize,
    timeout: Duration,
    out_of_order_action: OutOfOrderAction,
}

impl LokiSink {
    #[allow(clippy::missing_const_for_fn)] // const cannot run destructor
    pub fn new(config: LokiConfig, client: HttpClient, cx: SinkContext) -> crate::Result<Self> {
        Ok(Self {
            acker: cx.acker(),
            client,
            request_builder: RequestBuilder::new(config.endpoint, config.auth),
            encoder: EventEncoder {
                key_partitioner: KeyPartitioner::new(config.tenant_id),
                encoding: config.encoding,
                labels: config.labels,
                remove_label_fields: config.remove_label_fields,
                remove_timestamp: config.remove_timestamp,
            },
            max_batch_size: config.batch.max_size.unwrap_or(100_000),
            max_batch_bytes: config.batch.max_bytes.unwrap_or(102_400),
            timeout: Duration::from_secs(config.batch.timeout_secs.unwrap_or(1)),
            out_of_order_action: config.out_of_order_action,
        })
    }
}

#[async_trait::async_trait]
impl StreamSink for LokiSink {
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let io_bandwidth = 64;
        let (io_tx, io_rx) = mpsc::channel::<Request<Body>>(io_bandwidth);
        let service = tower::ServiceBuilder::new().service(LokiService::new(self.client.clone()));
        let io = run_io(io_rx, service, self.acker).in_current_span();
        let _ = tokio::spawn(io);

        let filter = FilterEncoder::new(
            input,
            self.encoder.clone(),
            GlobalTimestamps::default(),
            self.out_of_order_action.clone(),
        );

        let record_partitionner = RecordPartitionner;
        let request_builder = self.request_builder.clone();

        let batcher = Batcher::new(
            filter,
            record_partitionner,
            self.timeout,
            NonZeroUsize::new(self.max_batch_size).unwrap(),
            NonZeroUsize::new(self.max_batch_bytes),
        )
        .map(|(key, value)| {
            let builder = request_builder.clone();
            tokio::spawn(async move { builder.build(key, value) })
        })
        .buffer_unordered(io_bandwidth);

        tokio::pin!(batcher);

        while let Some(batch_join) = batcher.next().await {
            match batch_join {
                Ok(batch_request) => match batch_request {
                    Ok(request) => {
                        if io_tx.send(request).await.is_err() {
                            error!(
                            "Sink I/O channel should not be closed before sink itself is closed."
                        );
                            return Err(());
                        }
                    }
                    Err(error) => {
                        error!("Sink was unable to construct a payload body: {}", error);
                        return Err(());
                    }
                },
                Err(error) => {
                    error!("Task failed to properly join: {}", error);
                    return Err(());
                }
            }
        }

        Ok(())
    }
}

async fn run_io<S>(mut rx: mpsc::Receiver<http::Request<Body>>, mut service: S, acker: Acker)
where
    S: Service<http::Request<Body>>,
    S::Future: Send + 'static,
    S::Response: AsRef<EventStatus> + Send + 'static,
    S::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    let mut seqno = 0;
    while let Some(req) = rx.recv().await {
        let svc = service
            .ready()
            .await
            .expect("should not get error when waiting for svc readiness");
        let size = req.body().size_hint().0;
        let result = svc
            .call(req)
            .instrument(info_span!("request", request_id = %seqno))
            .await
            .map(|res| *res.as_ref())
            .map_err(|error| {
                error!("Sink IO failed with error: {:?}.", error);
                EventStatus::Failed
            });
        if result.is_ok() {
            acker.ack(size);
        }
        seqno += 1;
    }
}

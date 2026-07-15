//! The `iggy` stream sink: OTLP events → Obstack v3 envelopes → Iggy.

use std::collections::HashMap;
use std::sync::Arc;

use futures::StreamExt as _;
use vrl::value::Value;

use super::config::IggySinkConfig;
use super::otlp;
use super::proto::WriteBatch;
use super::publisher::IggyPublisher;
use crate::sinks::prelude::*;

pub(super) struct IggySink {
    publisher: Arc<IggyPublisher>,
    config: IggySinkConfig,
}

impl IggySink {
    pub(super) async fn connect(config: IggySinkConfig) -> crate::Result<Self> {
        let lanes = config.lanes.unwrap_or((config.shards.min(16)) as usize);
        let publisher = IggyPublisher::connect(
            &config.connection_string,
            &config.stream,
            &config.topic,
            config.shards,
            config.max_message_bytes,
            lanes,
        )
        .await?;
        Ok(IggySink {
            publisher: Arc::new(publisher),
            config,
        })
    }

    /// The OTLP-decoded root value of an event (logs/metrics are `Log`,
    /// traces are `Trace`); other event types are not produced by the
    /// `opentelemetry` source and are ignored.
    fn event_value(event: &Event) -> Option<&Value> {
        match event {
            Event::Log(log) => Some(log.value()),
            Event::Trace(trace) => Some(trace.value()),
            Event::Metric(_) => None,
        }
    }

    /// Resolve the tenant for one OTLP-decoded value: the configured
    /// `tenant_attribute` on the first resource, else the config default.
    fn tenant_of(&self, value: &Value) -> String {
        resource_tenant(value, &self.config.tenant_attribute)
            .unwrap_or_else(|| self.config.tenant.clone())
    }

    async fn publish_chunk(&self, chunk: Vec<Event>) -> Result<(), ()> {
        // Group rows by tenant; a WriteBatch is single-tenant.
        let mut batches: HashMap<String, WriteBatch> = HashMap::new();
        let mut finalizers = EventFinalizers::default();
        for mut event in chunk {
            finalizers.merge(event.take_finalizers());
            let Some(value) = Self::event_value(&event) else {
                continue;
            };
            let tenant = self.tenant_of(value);
            let batch = batches
                .entry(tenant.clone())
                .or_insert_with(|| WriteBatch::new(tenant));
            otlp::decode_event(value, batch);
        }

        let mut result = Ok(());
        for (_tenant, batch) in batches {
            if batch.is_empty() {
                continue;
            }
            if let Err(error) = self.publisher.publish(batch).await {
                tracing::error!(message = "Failed to publish batch to Iggy.", %error);
                result = Err(());
                break;
            }
        }

        let status = if result.is_ok() {
            EventStatus::Delivered
        } else {
            EventStatus::Rejected
        };
        finalizers.update_status(status);
        result
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let batch_events = self.config.batch_events.max(1);
        let mut chunks = input.ready_chunks(batch_events);
        while let Some(chunk) = chunks.next().await {
            // A publish failure has already finalized the batch as Rejected
            // and been surfaced; keep consuming so upstream backpressure and
            // retries behave, rather than tearing down the sink.
            let _ = self.publish_chunk(chunk).await;
        }
        // Input closed: close the Iggy connection lanes cleanly.
        let _ = self.publisher.shutdown().await;
        Ok(())
    }
}

#[async_trait::async_trait]
impl StreamSink<Event> for IggySink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

/// Read `resourceLogs|resourceMetrics|resourceSpans[0].resource.attributes`
/// for `key`, returning its string value.
fn resource_tenant(value: &Value, key: &str) -> Option<String> {
    let obj = value.as_object()?;
    let resources = obj
        .get("resourceLogs")
        .or_else(|| obj.get("resourceMetrics"))
        .or_else(|| obj.get("resourceSpans"))?
        .as_array()?;
    let first = resources.first()?.as_object()?;
    let attrs = first.get("resource")?.as_object()?.get("attributes")?.as_array()?;
    for kv in attrs {
        let kv = kv.as_object()?;
        let k = kv.get("key").and_then(value_as_string);
        if k.as_deref() == Some(key) {
            return kv
                .get("value")
                .and_then(|v| v.as_object())
                .and_then(|o| o.get("stringValue"))
                .and_then(value_as_string);
        }
    }
    None
}

fn value_as_string(v: &Value) -> Option<String> {
    match v {
        Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
        _ => None,
    }
}

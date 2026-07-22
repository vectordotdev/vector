//! The `iggy` stream sink: strict OTLP resources -> Obstack producer topics.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::StreamExt as _;
use vrl::value::Value;

use super::config::IggySinkConfig;
use super::datadog;
use super::otlp;
use super::proto::{PRODUCER_PARTITIONS, ProducerIdentity, WriteBatch};
use super::publisher::IggyPublisher;
use crate::sinks::prelude::*;

pub(super) struct IggySink {
    publisher: Arc<IggyPublisher>,
    config: IggySinkConfig,
}

impl IggySink {
    pub(super) async fn connect(config: IggySinkConfig) -> crate::Result<Self> {
        validate_config(&config)?;
        let lanes = config.lanes.unwrap_or(16);
        let publisher = IggyPublisher::connect(
            &config.connection_string,
            &config.stream,
            config.max_message_bytes,
            lanes,
            config.replication_factor,
            config.max_active_topics,
            config.bootstrap_timeout,
        )
        .await?;
        Ok(Self {
            publisher: Arc::new(publisher),
            config,
        })
    }

    fn event_value(event: &Event) -> Option<&Value> {
        match event {
            Event::Log(log) => Some(log.value()),
            Event::Trace(trace) => Some(trace.value()),
            Event::Metric(_) => None,
        }
    }

    /// Validate every resource before returning any decoded rows. One OTLP
    /// request may legitimately contain resources from different tenants and
    /// producers; they are isolated before decoding and publication.
    fn decode_event(
        &self,
        value: &Value,
    ) -> Result<Vec<(ProducerIdentity, WriteBatch)>, ValidationError> {
        let (resource_key, resources) = resources_of(value)?;
        if resources.is_empty() {
            return Err(ValidationError("OTLP event has no resources".into()));
        }

        let mut decoded = Vec::with_capacity(resources.len());
        for resource_group in resources {
            let attrs = resource_attributes(resource_group)?;
            let tenant = exactly_one_string(attrs, &self.config.tenant_attribute)?;
            let producer = canonical_producer(attrs)?;
            let identity =
                ProducerIdentity::new(tenant.clone(), producer, &self.config.topic_prefix)
                    .map_err(|error| ValidationError(error.to_string()))?;

            let mut isolated = value.clone();
            isolated
                .as_object_mut()
                .ok_or_else(|| ValidationError("OTLP event root must be an object".into()))?
                .insert(
                    resource_key.into(),
                    Value::Array(vec![resource_group.clone()]),
                );
            let mut batch = WriteBatch::new(tenant);
            otlp::decode_event(&isolated, &mut batch);
            decoded.push((identity, batch));
        }
        Ok(decoded)
    }

    async fn publish_chunk(&self, chunk: Vec<Event>) {
        let mut batches: HashMap<ProducerIdentity, WriteBatch> = HashMap::new();
        let mut accepted_finalizers = EventFinalizers::default();

        for mut event in chunk {
            let decoded = Self::event_value(&event)
                .ok_or_else(|| {
                    ValidationError(
                        "Iggy only accepts OTLP-decoded signals or Datadog traces".into(),
                    )
                })
                .and_then(|value| {
                    datadog::normalize(value, &self.config.tenant_attribute)
                        .map_err(ValidationError)
                        .and_then(|normalized| match normalized {
                            Some(normalized) => self.decode_event(&normalized),
                            None => self.decode_event(value),
                        })
                });
            let finalizers = event.take_finalizers();
            match decoded {
                Ok(resources) => {
                    accepted_finalizers.merge(finalizers);
                    for (producer, resource_batch) in resources {
                        let batch = batches
                            .entry(producer)
                            .or_insert_with(|| WriteBatch::new(resource_batch.tenant.clone()));
                        batch.logs.extend(resource_batch.logs);
                        batch.samples.extend(resource_batch.samples);
                        batch.exemplars.extend(resource_batch.exemplars);
                        batch.spans.extend(resource_batch.spans);
                    }
                }
                Err(error) => {
                    tracing::warn!(message = "Rejecting invalid OTLP event before Iggy publication.", %error);
                    finalizers.update_status(EventStatus::Rejected);
                }
            }
        }

        // Validate first, then publish. Any partial Iggy publication marks the
        // entire accepted Vector chunk retryable, conservatively replaying it.
        let mut publish_failed = false;
        for (producer, batch) in batches {
            if batch.is_empty() {
                continue;
            }
            if let Err(error) = self.publisher.publish(producer, batch).await {
                tracing::error!(message = "Failed to publish batch to Iggy.", %error);
                publish_failed = true;
                break;
            }
        }
        accepted_finalizers.update_status(if publish_failed {
            EventStatus::Errored
        } else {
            EventStatus::Delivered
        });
    }

    async fn run_inner(self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        while let Some(first) = input.next().await {
            let mut bytes = first.estimated_json_encoded_size_of().get();
            let mut chunk = vec![first];
            let deadline = Instant::now() + self.config.batch_timeout;
            while chunk.len() < self.config.batch_events && bytes < self.config.batch_bytes {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break;
                }
                match tokio::time::timeout(remaining, input.next()).await {
                    Ok(Some(event)) => {
                        bytes = bytes.saturating_add(event.estimated_json_encoded_size_of().get());
                        chunk.push(event);
                    }
                    Ok(None) | Err(_) => break,
                }
            }
            self.publish_chunk(chunk).await;
        }
        if let Err(error) = self.publisher.shutdown().await {
            tracing::warn!(message = "Iggy client shutdown failed", %error);
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl StreamSink<Event> for IggySink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

#[derive(Debug)]
struct ValidationError(String);

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn validate_config(config: &IggySinkConfig) -> crate::Result<()> {
    if config.partitions != PRODUCER_PARTITIONS {
        return Err(format!("Obstack requires exactly {PRODUCER_PARTITIONS} partitions").into());
    }
    if config.topic_prefix.is_empty()
        || config.tenant_attribute.is_empty()
        || config.batch_events == 0
        || config.batch_bytes == 0
        || config.batch_timeout == Duration::ZERO
    {
        return Err("topic_prefix, tenant_attribute, and batching limits must be non-empty".into());
    }
    Ok(())
}

/// Return the one supported signal key and all of its resource containers.
fn resources_of(value: &Value) -> Result<(&'static str, &[Value]), ValidationError> {
    let object = value
        .as_object()
        .ok_or_else(|| ValidationError("OTLP event root must be an object".into()))?;
    let present = ["resourceLogs", "resourceMetrics", "resourceSpans"]
        .into_iter()
        .filter_map(|key| object.get(key).map(|value| (key, value)))
        .collect::<Vec<_>>();
    if present.len() != 1 {
        return Err(ValidationError(
            "OTLP event must contain exactly one signal resource array".into(),
        ));
    }
    let (key, resources) = present[0];
    let resources = resources
        .as_array()
        .ok_or_else(|| ValidationError(format!("{key} must be an array")))?;
    Ok((key, resources))
}

fn resource_attributes(resource_group: &Value) -> Result<&[Value], ValidationError> {
    let resource = resource_group
        .as_object()
        .and_then(|object| object.get("resource"))
        .and_then(Value::as_object)
        .ok_or_else(|| {
            ValidationError("each OTLP resource container requires a resource object".into())
        })?;
    resource
        .get("attributes")
        .and_then(Value::as_array)
        .ok_or_else(|| ValidationError("each OTLP resource requires an attributes array".into()))
}

fn exactly_one_string(attrs: &[Value], key: &str) -> Result<String, ValidationError> {
    let matches = attrs
        .iter()
        .filter(|attribute| attribute_key(attribute).as_deref() == Some(key))
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(ValidationError(format!(
            "resource requires exactly one {key} attribute"
        )));
    }
    attribute_string(matches[0]).ok_or_else(|| {
        ValidationError(format!(
            "resource attribute {key} must be a non-empty string"
        ))
    })
}

fn optional_unique_string(attrs: &[Value], key: &str) -> Result<Option<String>, ValidationError> {
    let matches = attrs
        .iter()
        .filter(|attribute| attribute_key(attribute).as_deref() == Some(key))
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        return Err(ValidationError(format!(
            "resource attribute {key} must not be duplicated"
        )));
    }
    matches
        .first()
        .map(|attribute| {
            attribute_string(attribute).ok_or_else(|| {
                ValidationError(format!(
                    "resource attribute {key} must be a non-empty string"
                ))
            })
        })
        .transpose()
}

fn canonical_producer(attrs: &[Value]) -> Result<String, ValidationError> {
    if let Some(host) = optional_unique_string(attrs, "host.id")? {
        return Ok(format!("host.id:{host}"));
    }
    if let Some(instance) = optional_unique_string(attrs, "service.instance.id")? {
        return Ok(format!("service.instance.id:{instance}"));
    }
    let service = optional_unique_string(attrs, "service.name")?
        .ok_or_else(|| ValidationError("resource has no usable producer identity".into()))?;
    match optional_unique_string(attrs, "service.namespace")? {
        Some(namespace) => Ok(format!("service:{namespace}/{service}")),
        None => Ok(format!("service:{service}")),
    }
}

fn attribute_key(attribute: &Value) -> Option<String> {
    attribute
        .as_object()
        .and_then(|object| object.get("key"))
        .and_then(value_as_string)
}

fn attribute_string(attribute: &Value) -> Option<String> {
    let value = attribute
        .as_object()?
        .get("value")?
        .as_object()?
        .get("stringValue")?;
    value_as_string(value).filter(|value| !value.is_empty())
}

fn value_as_string(value: &Value) -> Option<String> {
    match value {
        Value::Bytes(bytes) => std::str::from_utf8(bytes).ok().map(str::to_owned),
        _ => None,
    }
}

#[cfg(test)]
mod validation_tests {
    use super::*;

    fn attr(key: &str, value: serde_json::Value) -> Value {
        Value::from(serde_json::json!({"key": key, "value": value}))
    }

    #[test]
    fn producer_precedence_and_namespace_are_stable() {
        let attrs = vec![
            attr("service.name", serde_json::json!({"stringValue": "api"})),
            attr(
                "service.namespace",
                serde_json::json!({"stringValue": "shop"}),
            ),
            attr(
                "service.instance.id",
                serde_json::json!({"stringValue": "instance"}),
            ),
            attr("host.id", serde_json::json!({"stringValue": "host"})),
        ];
        assert_eq!(canonical_producer(&attrs).unwrap(), "host.id:host");
        assert_eq!(
            canonical_producer(&attrs[..3]).unwrap(),
            "service.instance.id:instance"
        );
        assert_eq!(canonical_producer(&attrs[..2]).unwrap(), "service:shop/api");
    }

    #[test]
    fn duplicate_tenant_and_non_string_producer_are_invalid() {
        let tenant = attr("obstack.tenant.id", serde_json::json!({"stringValue": "a"}));
        assert!(exactly_one_string(&[tenant.clone(), tenant], "obstack.tenant.id").is_err());
        let attrs = vec![attr("host.id", serde_json::json!({"intValue": 1}))];
        assert!(canonical_producer(&attrs).is_err());
    }
}

use super::config::{Encoding, LokiConfig};
use crate::config::log_schema;
use crate::event::{self, Event, Value};
use crate::http::Auth;
use crate::internal_events::{LokiEventUnlabeled, LokiEventsProcessed, TemplateRenderingFailed};
use crate::sinks::util::buffer::loki::{LokiEvent, LokiRecord, PartitionKey};
use crate::sinks::util::encoding::{EncodingConfig, EncodingConfiguration};
use crate::sinks::util::http::HttpSink;
use crate::sinks::util::{PartitionInnerBuffer, UriSerde};
use crate::template::Template;
use shared::encode_logfmt;
use std::collections::HashMap;

pub struct LokiSink {
    endpoint: UriSerde,
    encoding: EncodingConfig<Encoding>,

    tenant_id: Option<Template>,
    labels: HashMap<Template, Template>,

    remove_label_fields: bool,
    remove_timestamp: bool,

    auth: Option<Auth>,
}

impl LokiSink {
    #[allow(clippy::missing_const_for_fn)] // const cannot run destructor
    pub fn new(config: LokiConfig) -> Self {
        Self {
            endpoint: config.endpoint,
            encoding: config.encoding,
            tenant_id: config.tenant_id,
            labels: config.labels,
            remove_label_fields: config.remove_label_fields,
            remove_timestamp: config.remove_timestamp,
            auth: config.auth,
        }
    }
}

#[async_trait::async_trait]
impl HttpSink for LokiSink {
    type Input = PartitionInnerBuffer<LokiRecord, PartitionKey>;
    type Output = PartitionInnerBuffer<serde_json::Value, PartitionKey>;

    fn encode_event(&self, mut event: Event) -> Option<Self::Input> {
        let tenant_id = self.tenant_id.as_ref().and_then(|t| {
            t.render_string(&event)
                .map_err(|error| {
                    emit!(&TemplateRenderingFailed {
                        error,
                        field: Some("tenant_id"),
                        drop_event: false,
                    })
                })
                .ok()
        });

        let mut labels = Vec::new();

        for (key_template, value_template) in &self.labels {
            if let (Ok(key), Ok(value)) = (
                key_template.render_string(&event),
                value_template.render_string(&event),
            ) {
                labels.push((key, value));
            }
        }

        if self.remove_label_fields {
            for template in self.labels.values() {
                if let Some(fields) = template.get_fields() {
                    for field in fields {
                        event.as_mut_log().remove(&field);
                    }
                }
            }
        }

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

        let key = PartitionKey::new(tenant_id, &mut labels);

        let event = LokiEvent { timestamp, event };
        Some(PartitionInnerBuffer::new(
            LokiRecord {
                labels,
                event,
                partition: key.clone(),
            },
            key,
        ))
    }

    async fn build_request(&self, output: Self::Output) -> crate::Result<http::Request<Vec<u8>>> {
        let (json, key) = output.into_parts();
        let tenant_id = key.tenant_id;

        let body = serde_json::to_vec(&json).unwrap();

        emit!(&LokiEventsProcessed {
            byte_size: body.len(),
        });

        let uri = format!("{}loki/api/v1/push", self.endpoint.uri);

        let mut req = http::Request::post(uri).header("Content-Type", "application/json");

        if let Some(tenant_id) = tenant_id {
            req = req.header("X-Scope-OrgID", tenant_id);
        }

        let mut req = req.body(body).unwrap();

        if let Some(auth) = &self.auth {
            auth.apply(&mut req);
        }

        Ok(req)
    }
}

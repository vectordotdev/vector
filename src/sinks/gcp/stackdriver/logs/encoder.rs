//! Encoding for the `gcp_stackdriver_logs` sink.

use std::{collections::HashMap, io};

use bytes::BytesMut;
use serde_json::{json, to_vec, Map};
use vector_lib::lookup::lookup_v2::ConfigValuePath;
use vrl::path::PathPrefix;

use crate::{
    sinks::{prelude::*, util::encoding::Encoder as SinkEncoder},
    template::TemplateRenderingError,
};

use super::config::{StackdriverLogName, StackdriverResource};

#[derive(Clone, Debug)]
pub(super) struct StackdriverLogsEncoder {
    transformer: Transformer,
    log_id: Template,
    log_name: StackdriverLogName,
    resource: StackdriverResource,
    severity_key: Option<ConfigValuePath>,
}

impl StackdriverLogsEncoder {
    /// Creates a new `StackdriverLogsEncoder`.
    pub(super) const fn new(
        transformer: Transformer,
        log_id: Template,
        log_name: StackdriverLogName,
        resource: StackdriverResource,
        severity_key: Option<ConfigValuePath>,
    ) -> Self {
        Self {
            transformer,
            log_id,
            log_name,
            resource,
            severity_key,
        }
    }

    pub(super) fn encode_event(&self, event: Event) -> Option<serde_json::Value> {
        let mut labels = HashMap::with_capacity(self.resource.labels.len());
        for (key, template) in &self.resource.labels {
            let value = template
                .render_string(&event)
                .map_err(|error| {
                    emit!(crate::internal_events::TemplateRenderingError {
                        error,
                        field: Some("resource.labels"),
                        drop_event: true,
                    });
                })
                .ok()?;
            labels.insert(key.clone(), value);
        }
        let log_name = self
            .log_name(&event)
            .map_err(|error| {
                emit!(crate::internal_events::TemplateRenderingError {
                    error,
                    field: Some("log_id"),
                    drop_event: true,
                });
            })
            .ok()?;

        let mut log = event.into_log();
        let severity = self
            .severity_key
            .as_ref()
            .and_then(|key| log.remove((PathPrefix::Event, &key.0)))
            .map(remap_severity)
            .unwrap_or_else(|| 0.into());

        let mut event = Event::Log(log);
        self.transformer.transform(&mut event);

        let log = event.into_log();

        let mut entry = Map::with_capacity(5);
        entry.insert("logName".into(), json!(log_name));
        entry.insert("jsonPayload".into(), json!(log));
        entry.insert("severity".into(), json!(severity));
        entry.insert(
            "resource".into(),
            json!({
                "type": self.resource.type_,
                "labels": labels,
            }),
        );

        // If the event contains a timestamp, send it in the main message so gcp can pick it up.
        if let Some(timestamp) = log.get_timestamp() {
            entry.insert("timestamp".into(), json!(timestamp));
        }

        Some(json!(entry))
    }

    fn log_name(&self, event: &Event) -> Result<String, TemplateRenderingError> {
        use StackdriverLogName::*;

        let log_id = self.log_id.render_string(event)?;

        Ok(match &self.log_name {
            BillingAccount(acct) => format!("billingAccounts/{}/logs/{}", acct, log_id),
            Folder(folder) => format!("folders/{}/logs/{}", folder, log_id),
            Organization(org) => format!("organizations/{}/logs/{}", org, log_id),
            Project(project) => format!("projects/{}/logs/{}", project, log_id),
        })
    }
}

pub(super) fn remap_severity(severity: Value) -> Value {
    let n = match severity {
        Value::Integer(n) => n - n % 100,
        Value::Bytes(s) => {
            let s = String::from_utf8_lossy(&s);
            match s.parse::<usize>() {
                Ok(n) => (n - n % 100) as i64,
                Err(_) => match s.to_uppercase() {
                    s if s.starts_with("EMERG") || s.starts_with("FATAL") => 800,
                    s if s.starts_with("ALERT") => 700,
                    s if s.starts_with("CRIT") => 600,
                    s if s.starts_with("ERR") || s == "ER" => 500,
                    s if s.starts_with("WARN") => 400,
                    s if s.starts_with("NOTICE") => 300,
                    s if s.starts_with("INFO") => 200,
                    s if s.starts_with("DEBUG") || s.starts_with("TRACE") => 100,
                    s if s.starts_with("DEFAULT") => 0,
                    _ => {
                        warn!(
                            message = "Unknown severity value string, using DEFAULT.",
                            value = %s,
                            internal_log_rate_limit = true
                        );
                        0
                    }
                },
            }
        }
        value => {
            warn!(
                message = "Unknown severity value type, using DEFAULT.",
                ?value,
                internal_log_rate_limit = true
            );
            0
        }
    };
    Value::Integer(n)
}

impl SinkEncoder<Vec<Event>> for StackdriverLogsEncoder {
    fn encode_input(
        &self,
        events: Vec<Event>,
        writer: &mut dyn io::Write,
    ) -> io::Result<(usize, GroupedCountByteSize)> {
        let mut byte_size = telemetry().create_request_count_byte_size();
        let mut n_events = events.len();
        let mut body = BytesMut::new();

        let mut entries = Vec::with_capacity(n_events);
        for event in &events {
            let size = event.estimated_json_encoded_size_of();
            if let Some(data) = self.encode_event(event.clone()) {
                byte_size.add_event(event, size);
                entries.push(data)
            } else {
                // encode_event() emits the `TemplateRenderingError` internal event,
                // which emits an `EventsDropped`, so no need to here.
                n_events -= 1;
            }
        }

        let events = json!({ "entries": entries });

        body.extend(&to_vec(&events)?);

        let body = body.freeze();

        write_all(writer, n_events, body.as_ref()).map(|()| (body.len(), byte_size))
    }
}

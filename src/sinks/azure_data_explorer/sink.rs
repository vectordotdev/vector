//! Sink and partitioner implementation for the `azure_data_explorer` sink.
//!
//! `AdxPartitioner` resolves the target ADX table name for each event using the
//! following precedence:
//!
//! 1. `table_field` - if set, look up that event field and use its string value.
//! 2. `table` template - render the `Template` for the event.
//! 3. `default_table` - fall back to this static name.
//! 4. Drop the event (emit `TemplateRenderingError`).
//!
//! Events are then grouped into per-table batches by `batched_partitioned`, and
//! each batch is built into an `HttpRequest<String>` (the `String` is the table
//! name) by `AzureDataExplorerRequestBuilder`.

use crate::{
    sinks::{
        prelude::*,
        util::http::{HttpJsonBatchSizer, HttpRequest},
    },
    template::Template,
};

use super::request_builder::AzureDataExplorerRequestBuilder;

// ---------------------------------------------------------------------------
// Partitioner
// ---------------------------------------------------------------------------

/// Resolves the ADX table name for a single event.
pub(super) struct AdxPartitioner {
    /// If set, look up this event field for the table name (highest priority).
    pub table_field: Option<String>,
    /// Template to render for the table name.
    pub table: Option<Template>,
    /// Static fallback when neither `table_field` nor `table` yields a value.
    pub default_table: Option<String>,
}

impl Partitioner for AdxPartitioner {
    type Item = Event;
    /// `None` means the event should be dropped.
    type Key = Option<String>;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        // 1. Try table_field first (only applicable to log events).
        if let Some(ref field) = self.table_field {
            if let Event::Log(log) = item {
                if let Some(table) = log
                    .get(field.as_str())
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .filter(|s| !s.is_empty())
                {
                    return Some(table);
                }
            }
        }

        // 2. Try template rendering.
        if let Some(ref tmpl) = self.table {
            match tmpl.render_string(item) {
                Ok(table) if !table.is_empty() => return Some(table),
                Ok(_) => {} // empty render, fall through
                Err(error) => {
                    if self.default_table.is_none() {
                        // No fallback: drop the event.
                        emit!(TemplateRenderingError {
                            error,
                            field: Some("table"),
                            drop_event: true,
                        });
                        return None;
                    }
                    emit!(TemplateRenderingError {
                        error,
                        field: Some("table"),
                        drop_event: false,
                    });
                }
            }
        }

        // 3. Fall back to default_table.
        if let Some(ref default) = self.default_table {
            return Some(default.clone());
        }

        // 4. No table resolved: drop.
        None
    }
}

// ---------------------------------------------------------------------------
// Sink
// ---------------------------------------------------------------------------

pub(super) struct AzureDataExplorerSink<S> {
    service: S,
    batch_settings: BatcherSettings,
    request_builder: AzureDataExplorerRequestBuilder,
    partitioner: AdxPartitioner,
}

impl<S> AzureDataExplorerSink<S>
where
    S: Service<HttpRequest<String>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    pub(super) fn new(
        service: S,
        batch_settings: BatcherSettings,
        request_builder: AzureDataExplorerRequestBuilder,
        partitioner: AdxPartitioner,
    ) -> Self {
        Self {
            service,
            batch_settings,
            request_builder,
            partitioner,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let batch_settings = self.batch_settings;

        input
            .batched_partitioned(
                self.partitioner,
                batch_settings.timeout,
                |_| batch_settings.as_item_size_config(HttpJsonBatchSizer),
            )
            // Drop batches whose partition key couldn't be resolved (table name is None).
            .filter_map(|(key, batch)| async move { key.map(move |k| (k, batch)) })
            .request_builder(
                default_request_builder_concurrency_limit(),
                self.request_builder,
            )
            .filter_map(|request| async move {
                match request {
                    Err(error) => {
                        emit!(SinkRequestBuildError { error });
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(self.service)
            .run()
            .await
    }
}

#[async_trait::async_trait]
impl<S> StreamSink<Event> for AzureDataExplorerSink<S>
where
    S: Service<HttpRequest<String>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(
        self: Box<Self>,
        input: futures_util::stream::BoxStream<'_, Event>,
    ) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_log_event(fields: &[(&str, &str)]) -> Event {
        let mut log = LogEvent::default();
        for (k, v) in fields {
            log.insert(*k, *v);
        }
        Event::Log(log)
    }

    fn make_partitioner(
        table_field: Option<&str>,
        table: Option<&str>,
        default_table: Option<&str>,
    ) -> AdxPartitioner {
        AdxPartitioner {
            table_field: table_field.map(String::from),
            table: table.map(|s| s.try_into().expect("valid template")),
            default_table: default_table.map(String::from),
        }
    }

    #[test]
    fn table_field_takes_precedence() {
        let p = make_partitioner(Some("adx_table"), Some("static_table"), Some("default"));
        let event = make_log_event(&[("adx_table", "from_field"), ("msg", "hello")]);
        assert_eq!(p.partition(&event), Some("from_field".to_string()));
    }

    #[test]
    fn static_template_used_when_no_field() {
        let p = make_partitioner(None, Some("my_table"), Some("default"));
        let event = make_log_event(&[("msg", "hello")]);
        assert_eq!(p.partition(&event), Some("my_table".to_string()));
    }

    #[test]
    fn default_table_used_when_template_has_missing_key() {
        // Template references a field that's not in the event
        let p = make_partitioner(None, Some("{{ missing_field }}_logs"), Some("fallback"));
        let event = make_log_event(&[("msg", "hello")]);
        // Template rendering fails -> falls through to default_table
        assert_eq!(p.partition(&event), Some("fallback".to_string()));
    }

    #[test]
    fn event_dropped_when_no_table_resolves() {
        // table_field not present, no template, no default
        let p = make_partitioner(Some("adx_table"), None, None);
        let event = make_log_event(&[("msg", "hello")]); // "adx_table" field missing
        // table_field not found, no template, no default -> drop (None)
        assert_eq!(p.partition(&event), None);
    }

    #[test]
    fn dynamic_template_resolves_from_event_field() {
        let p = make_partitioner(None, Some("{{ env }}_logs"), None);
        let event = make_log_event(&[("env", "production"), ("msg", "hello")]);
        assert_eq!(p.partition(&event), Some("production_logs".to_string()));
    }
}

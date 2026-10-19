//! Implementation of the `clickhouse` sink.

use std::collections::BTreeMap;

use super::{config::Format, request_builder::ClickhouseRequestBuilder};
use crate::sinks::{prelude::*, util::http::HttpRequest};

pub struct ClickhouseSink<S> {
    batch_settings: BatcherSettings,
    service: S,
    database: ConfinedTemplate,
    table: ConfinedTemplate,
    headers: BTreeMap<String, ConfinedTemplate>,
    format: Format,
    request_builder: ClickhouseRequestBuilder,
}

impl<S> ClickhouseSink<S>
where
    S: Service<HttpRequest<PartitionKey>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    pub const fn new(
        batch_settings: BatcherSettings,
        service: S,
        database: ConfinedTemplate,
        table: ConfinedTemplate,
        headers: BTreeMap<String, ConfinedTemplate>,
        format: Format,
        request_builder: ClickhouseRequestBuilder,
    ) -> Self {
        Self {
            batch_settings,
            service,
            database,
            table,
            headers,
            format,
            request_builder,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let batch_settings = self.batch_settings;

        input
            .batched_partitioned(
                KeyPartitioner::new(self.database, self.table, self.headers, self.format),
                batch_settings.timeout,
                |_| batch_settings.as_byte_size_config(),
            )
            .filter_map(|(key, batch)| async move { key.map(move |k| (k, batch)) })
            .request_builder(
                default_request_builder_concurrency_limit(),
                self.request_builder,
            )
            .filter_map(|request| async {
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
impl<S> StreamSink<Event> for ClickhouseSink<S>
where
    S: Service<HttpRequest<PartitionKey>> + Send + 'static,
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

/// Groups events that share a destination, format, and rendered HTTP headers.
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct PartitionKey {
    pub database: String,
    pub table: String,
    pub format: Format,
    pub headers: BTreeMap<String, String>,
}

/// Renders destination and header templates before batching.
struct KeyPartitioner {
    database: ConfinedTemplate,
    table: ConfinedTemplate,
    headers: BTreeMap<String, ConfinedTemplate>,
    format: Format,
}

impl KeyPartitioner {
    const fn new(
        database: ConfinedTemplate,
        table: ConfinedTemplate,
        headers: BTreeMap<String, ConfinedTemplate>,
        format: Format,
    ) -> Self {
        Self {
            database,
            table,
            headers,
            format,
        }
    }

    fn render(template: &ConfinedTemplate, item: &Event, field: &'static str) -> Option<String> {
        template
            .render_string(item)
            .map_err(|error| {
                emit!(TemplateRenderingError {
                    error,
                    field: Some(field),
                    drop_event: true,
                });
            })
            .ok()
    }
}

impl Partitioner for KeyPartitioner {
    type Item = Event;
    type Key = Option<PartitionKey>;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        let database = Self::render(&self.database, item, "database_key")?;
        let table = Self::render(&self.table, item, "table_key")?;

        let headers = self
            .headers
            .iter()
            .map(|(name, template)| {
                Self::render(template, item, "request.headers").map(|value| (name.clone(), value))
            })
            .collect::<Option<BTreeMap<_, _>>>()?;

        Some(PartitionKey {
            database,
            table,
            format: self.format,
            headers,
        })
    }
}

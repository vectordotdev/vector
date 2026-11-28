//! Implementation of the `clickhouse` sink.

use super::{config::Format, request_builder::ClickhouseRequestBuilder};
use crate::sinks::{
    prelude::*,
    util::{http::HttpRequest, partitioner::render_partition_value},
};
use vector_lib::internal_event::emit;

pub(super) struct ClickhouseSink<S> {
    batch_settings: BatcherSettings,
    service: S,
    database: ConfinedTemplate,
    table: ConfinedTemplate,
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
    pub(super) const fn new(
        batch_settings: BatcherSettings,
        service: S,
        database: ConfinedTemplate,
        table: ConfinedTemplate,
        format: Format,
        request_builder: ClickhouseRequestBuilder,
    ) -> Self {
        Self {
            batch_settings,
            service,
            database,
            table,
            format,
            request_builder,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let batch_settings = self.batch_settings;

        input
            .batched_partitioned(
                KeyPartitioner::new(self.database, self.table, self.format),
                batch_settings.timeout,
                |_| batch_settings.as_byte_size_config(),
            )
            .filter_map(|result| async move { result.ok() })
            .request_builder(
                default_request_builder_concurrency_limit(),
                self.request_builder,
            )
            .filter_map(|request| async {
                match request {
                    Err(error) => {
                        emit(SinkRequestBuildError { error });
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

/// `PartitionKey` used to partition events by (database, table) pair.
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub(crate) struct PartitionKey {
    pub database: String,
    pub table: String,
    pub format: Format,
}

/// `KeyPartitioner` that partitions events by (database, table) pair.
struct KeyPartitioner {
    database: ConfinedTemplate,
    table: ConfinedTemplate,
    format: Format,
}

impl KeyPartitioner {
    const fn new(database: ConfinedTemplate, table: ConfinedTemplate, format: Format) -> Self {
        Self {
            database,
            table,
            format,
        }
    }
}

impl Partitioner for KeyPartitioner {
    type Item = Event;
    type Key = PartitionKey;
    type Error = crate::template::TemplateRenderingError;

    fn partition(
        &self,
        item: &Self::Item,
    ) -> Result<PartitionDecision<Self::Key>, PreparationError<Self::Error>> {
        let PartitionDecision::Route(database) =
            render_partition_value(self.database.render_string(item), "database_key")?
        else {
            return Ok(PartitionDecision::Discard);
        };
        let PartitionDecision::Route(table) =
            render_partition_value(self.table.render_string(item), "table_key")?
        else {
            return Ok(PartitionDecision::Discard);
        };
        Ok(PartitionDecision::Route(PartitionKey {
            database,
            table,
            format: self.format,
        }))
    }
}

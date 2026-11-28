use crate::sinks::{
    doris::request_builder::DorisRequestBuilder, prelude::*, util::http::HttpRequest,
};
use vector_lib::internal_event::emit;

use crate::sinks::util::partitioner::render_partition_value;

pub(super) struct DorisSink<S> {
    batch_settings: BatcherSettings,
    service: S,
    request_builder: DorisRequestBuilder,
    database: ConfinedTemplate,
    table: ConfinedTemplate,
}

impl<S> DorisSink<S>
where
    S: Service<HttpRequest<DorisPartitionKey>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    pub(super) const fn new(
        service: S,
        batch_settings: BatcherSettings,
        database: ConfinedTemplate,
        table: ConfinedTemplate,
        request_builder: DorisRequestBuilder,
    ) -> Self {
        DorisSink {
            batch_settings,
            service,
            request_builder,
            database,
            table,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let batch_settings = self.batch_settings;
        let key_partitioner = DorisKeyPartitioner::new(self.database, self.table);
        input
            .batched_partitioned(key_partitioner, batch_settings.timeout, |_| {
                batch_settings.as_byte_size_config()
            })
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
impl<S> StreamSink<Event> for DorisSink<S>
where
    S: Service<HttpRequest<DorisPartitionKey>> + Send + 'static,
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
pub(super) struct DorisPartitionKey {
    pub database: String,
    pub table: String,
}

/// `KeyPartitioner` that partitions events by (database, table) pair.
struct DorisKeyPartitioner {
    database: ConfinedTemplate,
    table: ConfinedTemplate,
}

impl DorisKeyPartitioner {
    const fn new(database: ConfinedTemplate, table: ConfinedTemplate) -> Self {
        Self { database, table }
    }
}

impl Partitioner for DorisKeyPartitioner {
    type Item = Event;
    type Key = DorisPartitionKey;
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
        Ok(PartitionDecision::Route(DorisPartitionKey {
            database,
            table,
        }))
    }
}

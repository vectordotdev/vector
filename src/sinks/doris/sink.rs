use crate::sinks::{
    doris::request_builder::DorisRequestBuilder, prelude::*, util::http::HttpRequest,
};

pub struct DorisSink<S> {
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
    pub const fn new(
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

/// PartitionKey used to partition events by (database, table) pair.
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct DorisPartitionKey {
    pub database: String,
    pub table: String,
}

/// KeyPartitioner that partitions events by (database, table) pair.
struct DorisKeyPartitioner {
    database: ConfinedTemplate,
    table: ConfinedTemplate,
}

impl DorisKeyPartitioner {
    const fn new(database: ConfinedTemplate, table: ConfinedTemplate) -> Self {
        Self { database, table }
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

impl Partitioner for DorisKeyPartitioner {
    type Item = Event;
    type Key = Option<DorisPartitionKey>;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        let database = Self::render(&self.database, item, "database_key")?;
        let table = Self::render(&self.table, item, "table_key")?;
        Some(DorisPartitionKey { database, table })
    }
}

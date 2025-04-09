use crate::sinks::{prelude::*, util::http::HttpRequest};
use crate::sinks::doris::request_builder::DorisRequestBuilder;
use super::{config::DorisConfig, config::DorisFormat};

pub struct DorisSink<S> {
    batch_settings: BatcherSettings,
    service: S,
    config: DorisConfig,
    request_builder: DorisRequestBuilder
}

impl<S> DorisSink<S>
where
    S: Service<HttpRequest<DorisPartitionKey>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    pub fn new(
        batch_settings: BatcherSettings,
        service: S,
        config: DorisConfig,
        request_builder: DorisRequestBuilder
    ) -> Self {
        DorisSink {
            batch_settings,
            service,
            config,
            request_builder,
        }
    }
    
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let batch_settings = self.batch_settings;
        let key_partitioner = DorisKeyPartitioner::new(
            self.config.database, 
            self.config.table, 
            self.config.format
        );
        
        input
            .batched_partitioned(key_partitioner, || batch_settings.as_byte_size_config())
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
pub(super) struct DorisPartitionKey {
    pub database: String,
    pub table: String,
    pub format: DorisFormat,
}

/// KeyPartitioner that partitions events by (database, table) pair.
struct DorisKeyPartitioner {
    database: Template,
    table: Template,
    format: DorisFormat,
}

impl DorisKeyPartitioner {
    const fn new(database: Template, table: Template, format: DorisFormat) -> Self {
        Self {
            database,
            table,
            format,
        }
    }
    
    fn render(template: &Template, item: &Event, field: &'static str) -> Option<String> {
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
        Some(DorisPartitionKey {
            database,
            table,
            format: self.format,
        })
    }
}